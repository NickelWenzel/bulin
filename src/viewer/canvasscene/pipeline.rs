use crate::viewer::canvasscene::uniforms;

use iced::{futures::executor::block_on, Rectangle};
use iced_wgpu::wgpu;

use std::borrow::Cow;

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    default_uniforms: wgpu::Buffer,
    custom_uniforms: Option<wgpu::Buffer>,
    uniform_bind_group: wgpu::BindGroup,
    custom_uniform_bind_group: Option<wgpu::BindGroup>,
    pub version: usize,
}

impl Pipeline {
    pub fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        shader: &str,
        custom_uniform_definition: &str,
        custom_uniforms_size: u64,
        version: usize,
    ) -> Result<Self, wgpu::Error> {
        let default_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bulin_canvas.pipeline.uniforms"),
            size: std::mem::size_of::<uniforms::DefaultUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("bulin_canvas.pipeline.uniform_bind_group_layout"),
            entries: &[wgpu::BindGroupLayoutEntry {
                binding: 0,
                visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
                ty: wgpu::BindingType::Buffer {
                    ty: wgpu::BufferBindingType::Uniform,
                    has_dynamic_offset: false,
                    min_binding_size: None,
                },
                count: None,
            }],
        });

        let uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bulin_canvas.pipeline.uniform_bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(
                    default_uniforms.as_entire_buffer_binding(),
                ),
            }],
        });

        let (custom_uniforms, custom_layout, custom_uniform_bind_group) = if custom_uniforms_size
            > 0
        {
            let custom_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bulin_canvas.pipeline.custom"),
                size: custom_uniforms_size,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
                mapped_at_creation: false,
            });
            let custom_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                label: Some("bulin_canvas.pipeline.custom_uniform_bind_group_layout"),
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
            let custom_uniform_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
                label: Some("bulin_canvas.pipeline.custom_uniform_bind_group"),
                layout: &custom_layout,
                entries: &[wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(
                        custom_uniforms.as_entire_buffer_binding(),
                    ),
                }],
            });
            (
                Some(custom_uniforms),
                Some(custom_layout),
                Some(custom_uniform_bind_group),
            )
        } else {
            (None, None, None)
        };

        let pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
            label: Some("bulin_canvas.pipeline.layout"),
            bind_group_layouts: if let Some(custom_layout) = &custom_layout {
                &[&layout, custom_layout]
            } else {
                &[&layout]
            },
            push_constant_ranges: &[],
        };

        let pipeline_layout = device.create_pipeline_layout(&pipeline_layout_descriptor);

        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(concat!(
                include_str!("shaders/uniforms.wgsl"),
                "\n",
                include_str!("shaders/vertex_shader.wgsl"),
            ))),
        });

        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.fragment_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                format!(
                    "{}\n{}\n{}",
                    include_str!("shaders/uniforms.wgsl"),
                    custom_uniform_definition,
                    shader
                )
                .as_str(),
            )),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bulin_canvas.pipeline.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &vertex_shader,
                entry_point: "vs_main",
                buffers: &[],
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &fragment_shader,
                entry_point: "fs_main",
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview: None,
        });

        if let Some(error) = block_on(device.pop_error_scope()) {
            Err(error)
        } else {
            Ok(Self {
                pipeline,
                default_uniforms,
                custom_uniforms,
                uniform_bind_group,
                custom_uniform_bind_group,
                version,
            })
        }
    }

    pub fn update_default_buffer(
        &mut self,
        queue: &wgpu::Queue,
        default_uniforms: &uniforms::DefaultUniforms,
    ) {
        queue.write_buffer(
            &self.default_uniforms,
            0,
            bytemuck::bytes_of(default_uniforms),
        );
    }

    pub fn update_custom_buffer(
        &mut self,
        queue: &wgpu::Queue,
        custom_uniforms: &uniforms::CustomUniforms,
    ) {
        if let Some(custom_uniforms_buffer) = &self.custom_uniforms {
            queue.write_buffer(
                custom_uniforms_buffer,
                0,
                bytemuck::cast_slice(custom_uniforms),
            );
        }
    }

    pub fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: Rectangle<u32>,
    ) {
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("fill color test"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        pass.set_pipeline(&self.pipeline);
        pass.set_viewport(
            viewport.x as f32,
            viewport.y as f32,
            viewport.width as f32,
            viewport.height as f32,
            0.0,
            1.0,
        );

        pass.set_bind_group(0, &self.uniform_bind_group, &[]);
        if let Some(custom_uniform_bind_group) = &self.custom_uniform_bind_group {
            pass.set_bind_group(1, custom_uniform_bind_group, &[]);
        }

        pass.draw(0..3, 0..1);
    }
}
