//! Double-buffered render-to-texture shader widget.
//!
//! The flower shader is rendered into an offscreen "back" texture. Once the
//! GPU confirms that submission is complete, front/back are swapped and the
//! widget blits the freshly finished texture to the screen. Until then, the
//! previous frame keeps being shown.

use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};

use iced::futures::executor::block_on;
use iced::wgpu;
use iced::widget::shader::{self};
use tracing::{debug, warn};

use super::PrimitiveData;

const OFFSCREEN_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8UnormSrgb;
const BLIT_WGSL: &str = include_str!("shaders/blit.wgsl");
const DEFAULT_WIDTH: u32 = 1000;
const DEFAULT_HEIGHT: u32 = 1000;

struct Target {
    _texture: wgpu::Texture,
    view: wgpu::TextureView,
    bind_group: wgpu::BindGroup, // texture + sampler, used by the blit pass
}

struct UniformsBuffer {
    uniforms: wgpu::Buffer,
    uniforms_layout: wgpu::BindGroupLayout,
    uniforms_bind_group: wgpu::BindGroup,
}

pub struct Pipeline {
    // GPU objects
    offscreen: wgpu::RenderPipeline,
    blit: wgpu::RenderPipeline,
    uniforms: Option<UniformsBuffer>,

    // Double buffer
    targets: [Target; 2],
    front: usize,

    // Completion tracking
    generation: u64,
    in_flight: Option<u64>,
    done: Arc<AtomicU64>,

    // Primitive data
    primitive_data: Arc<PrimitiveData>,
}

impl Pipeline {}

impl shader::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        device.on_uncaptured_error(Arc::new(|e| debug!("WGPU error: {e:#}")));

        let empty_primitive_data = PrimitiveData::empty();

        // --- Uniforms -----------------------------------------------------
        let uniforms = create_uniforms_buffer(device, empty_primitive_data.uniforms_size());

        // --- Offscreen pipeline (the custom shader) -----------------------
        let offscreen = create_offscreen_pipeline(
            device,
            uniforms.as_ref().map(|u| &u.uniforms_layout),
            &empty_primitive_data.whole_shader(),
        )
        .expect("This should never fail");

        // --- Blit pipeline (offscreen texture -> widget) ------------------
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("bulin.sampler"),
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            ..Default::default()
        });

        let blit_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bulin.blit.layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let blit_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin.blit.shader"),
            source: wgpu::ShaderSource::Wgsl(BLIT_WGSL.into()),
        });

        let blit_pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bulin.blit.pipeline_layout"),
            bind_group_layouts: &[Some(&blit_layout)],
            immediate_size: 0,
        });

        let blit = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bulin.blit.pipeline"),
            layout: Some(&blit_pipeline_layout),
            vertex: wgpu::VertexState {
                module: &blit_shader,
                entry_point: Some("vs_main"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &blit_shader,
                entry_point: Some("fs_main"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format, // the format iced renders the UI with
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            cache: None,
            multiview_mask: None,
        });

        // --- Targets -------------------------------------------------
        let make_target = || {
            let texture = device.create_texture(&wgpu::TextureDescriptor {
                label: Some("bulin.offscreen.texture"),
                size: wgpu::Extent3d {
                    width: DEFAULT_WIDTH,
                    height: DEFAULT_HEIGHT,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format: OFFSCREEN_FORMAT,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            });

            let view = texture.create_view(&Default::default());

            let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bulin.blit.bind_group"),
                layout: &blit_layout,
                entries: &[
                    wgpu::BindGroupEntry {
                        binding: 0,
                        resource: wgpu::BindingResource::TextureView(&view),
                    },
                    wgpu::BindGroupEntry {
                        binding: 1,
                        resource: wgpu::BindingResource::Sampler(&sampler),
                    },
                ],
            });

            Target {
                _texture: texture,
                view,
                bind_group,
            }
        };

        let targets = [make_target(), make_target()];

        Self {
            offscreen,
            blit,
            uniforms,
            targets,
            front: 0,
            generation: 0,
            in_flight: None,
            done: Arc::new(AtomicU64::new(0)),
            primitive_data: Arc::new(empty_primitive_data),
        }
    }
}

impl Pipeline {
    pub(super) fn prepare(
        &mut self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        primitive_data: Arc<PrimitiveData>,
    ) {
        // 1. If the offscreen render we kicked off earlier has completed,
        //    swap front and back. From now on `draw()` samples the new frame.
        if let Some(generation) = self.in_flight
            && self.done.load(Ordering::Acquire) >= generation
        {
            self.front ^= 1;
            self.in_flight = None;
        }

        // 2. Only start a new offscreen render if the previous one finished.
        //    While one is in flight, the front texture keeps being displayed.
        if self.in_flight.is_none() {
            let mut needs_draw = false;

            //The order of the operations matters here
            // 1. Check if buffer needs to be re-allocated
            if self.primitive_data.uniforms_size() != primitive_data.uniforms_size() {
                self.uniforms = create_uniforms_buffer(device, primitive_data.uniforms_size());
                needs_draw = true;
            }

            // 2. Check if buffer content changed
            if let Some(uniforms) = &self.uniforms
                && self.primitive_data.uniforms.uniforms_bytes
                    != primitive_data.uniforms.uniforms_bytes
            {
                queue.write_buffer(
                    &uniforms.uniforms,
                    0,
                    &primitive_data.uniforms.uniforms_bytes,
                );
                needs_draw = true;
            }

            // 3. Check if shader changed
            if (self.primitive_data.shader != primitive_data.shader)
                || (self.primitive_data.uniforms.uniforms_str
                    != primitive_data.uniforms.uniforms_str)
            {
                let offscreen = create_offscreen_pipeline(
                    device,
                    self.uniforms.as_ref().map(|u| &u.uniforms_layout),
                    &primitive_data.whole_shader(),
                );

                match offscreen {
                    Some(offscreen) => {
                        self.offscreen = offscreen;
                        needs_draw = true;
                    }
                    None => {
                        // Shader invalid so we do not need to draw anything
                        needs_draw = false;
                    }
                }
            }

            self.primitive_data = primitive_data;

            if needs_draw {
                let back = &self.targets[self.front ^ 1];

                let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
                    label: Some("bulin.offscreen.encoder"),
                });

                {
                    let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("bulin.offscreen.pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &back.view,
                            depth_slice: None,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        timestamp_writes: None,
                        occlusion_query_set: None,
                        multiview_mask: None,
                    });

                    pass.set_pipeline(&self.offscreen);
                    if let Some(uniforms) = &self.uniforms {
                        pass.set_bind_group(0, &uniforms.uniforms_bind_group, &[]);
                    }
                    pass.draw(0..3, 0..1); // fullscreen triangle
                }

                queue.submit(Some(encoder.finish()));

                // 3. Flip the "done" generation counter once the GPU is through.
                self.generation += 1;
                let generation = self.generation;
                self.in_flight = Some(generation);

                let done = self.done.clone();
                queue.on_submitted_work_done(move || {
                    done.fetch_max(generation, Ordering::AcqRel);
                });
            }
        }
    }

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) -> bool {
        // iced has already set viewport + scissor to the widget bounds.
        // Just blit the front texture.
        render_pass.set_pipeline(&self.blit);
        render_pass.set_bind_group(0, &self.targets[self.front].bind_group, &[]);
        render_pass.draw(0..3, 0..1);

        true // handled here; no need for the fallback `render()` path
    }
}

fn create_offscreen_pipeline(
    device: &wgpu::Device,
    uniforms_layout: Option<&wgpu::BindGroupLayout>,
    shader: &str,
) -> Option<wgpu::RenderPipeline> {
    // Guard shader compilation *and* pipeline creation: a shader that
    // references a bind group the layout doesn't provide only errors at
    // pipeline creation, so popping the scope earlier would let a broken
    // pipeline through.
    let error_scope = device.push_error_scope(wgpu::ErrorFilter::Validation);

    let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("bulin.shader"),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });

    let offscreen_layout = uniforms_layout.map(|uniforms_layout| {
        device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bulin.offscreen.layout"),
            bind_group_layouts: &[Some(uniforms_layout)],
            immediate_size: 0,
        })
    });

    let offscreen = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("bulin.offscreen.pipeline"),
        layout: offscreen_layout.as_ref(),
        vertex: wgpu::VertexState {
            module: &shader,
            entry_point: Some("vs_main"),
            compilation_options: Default::default(),
            buffers: &[],
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader,
            entry_point: Some("fs_main"),
            compilation_options: Default::default(),
            targets: &[Some(OFFSCREEN_FORMAT.into())],
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        cache: None,
        multiview_mask: None,
    });

    if let Some(error) = block_on(error_scope.pop()) {
        warn!("Error creating offscreen pipeline:\n{error}");
        return None;
    }

    Some(offscreen)
}

fn create_uniforms_buffer(device: &wgpu::Device, size: u64) -> Option<UniformsBuffer> {
    if size == 0 {
        return None;
    }

    let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bulin.uniforms"),
        size,
        usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
        mapped_at_creation: false,
    });

    let uniforms_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
        label: Some("bulin.uniforms.layout"),
        entries: &[wgpu::BindGroupLayoutEntry {
            binding: 0,
            visibility: wgpu::ShaderStages::FRAGMENT,
            ty: wgpu::BindingType::Buffer {
                ty: wgpu::BufferBindingType::Uniform,
                has_dynamic_offset: false,
                min_binding_size: None,
            },
            count: None,
        }],
    });

    let uniforms_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("bulin.uniforms.bind_group"),
        layout: &uniforms_layout,
        entries: &[wgpu::BindGroupEntry {
            binding: 0,
            resource: uniforms.as_entire_binding(),
        }],
    });

    Some(UniformsBuffer {
        uniforms,
        uniforms_layout,
        uniforms_bind_group,
    })
}
