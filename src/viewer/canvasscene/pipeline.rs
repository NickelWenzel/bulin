use crate::viewer::canvasscene::uniforms;

use iced::Rectangle;
use iced_wgpu::wgpu;

use std::borrow::Cow;

use super::{VersionedShader, VersionedUniformRenderData};

pub struct Pipeline {
    pipeline: Option<wgpu::RenderPipeline>,
    default_data: BufferData,
    custom_data: CustomBufferData,
    vertex_shader: wgpu::ShaderModule,
    fragment_shader: Option<wgpu::ShaderModule>,
}

pub struct BufferData {
    pub buffer: wgpu::Buffer,
    pub layout: wgpu::BindGroupLayout,
    pub bind_group: wgpu::BindGroup,
}

pub struct CustomBufferData {
    pub buffer_data: Option<BufferData>,
    pub shader_version: usize,
    pub uniforms_version: usize,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device) -> Self {
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
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

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("bulin_canvas.pipeline.uniform_bind_group"),
            layout: &layout,
            entries: &[wgpu::BindGroupEntry {
                binding: 0,
                resource: wgpu::BindingResource::Buffer(buffer.as_entire_buffer_binding()),
            }],
        });

        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(concat!(
                include_str!("shaders/uniforms.wgsl"),
                "\n",
                include_str!("shaders/vertex_shader.wgsl"),
            ))),
        });

        Self {
            pipeline: None,
            default_data: BufferData {
                buffer,
                layout,
                bind_group,
            },
            custom_data: CustomBufferData {
                buffer_data: None,
                shader_version: 0,
                uniforms_version: 0,
            },
            vertex_shader,
            fragment_shader: None,
        }
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        fragment_shader: &VersionedShader,
        custom_uniforms: &VersionedUniformRenderData,
    ) -> &mut Self {
        if self.update_custom_uniforms(device, custom_uniforms)
            || self.update_fragment_shader(device, fragment_shader, custom_uniforms)
        {
            if let Some(fragment_shader) = &self.fragment_shader {
                let pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
                    label: Some("bulin_canvas.pipeline.layout"),
                    bind_group_layouts: if let Some(BufferData {
                        buffer: _,
                        layout: custom_layout,
                        bind_group: _,
                    }) = &self.custom_data.buffer_data
                    {
                        &[&self.default_data.layout, custom_layout]
                    } else {
                        &[&self.default_data.layout]
                    },
                    push_constant_ranges: &[],
                };

                let pipeline_layout = device.create_pipeline_layout(&pipeline_layout_descriptor);

                self.pipeline = Some(device.create_render_pipeline(
                    &wgpu::RenderPipelineDescriptor {
                        label: Some("bulin_canvas.pipeline.pipeline"),
                        layout: Some(&pipeline_layout),
                        vertex: wgpu::VertexState {
                            module: &self.vertex_shader,
                            entry_point: "vs_main",
                            buffers: &[],
                        },
                        primitive: wgpu::PrimitiveState::default(),
                        depth_stencil: None,
                        multisample: Default::default(),
                        fragment: Some(wgpu::FragmentState {
                            module: fragment_shader,
                            entry_point: "fs_main",
                            targets: &[Some(wgpu::ColorTargetState {
                                format,
                                blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                                write_mask: wgpu::ColorWrites::ALL,
                            })],
                        }),
                        multiview: None,
                    },
                ))
            }
        }
        self
    }

    fn update_fragment_shader(
        &mut self,
        device: &wgpu::Device,
        fragment_shader: &VersionedShader,
        custom_uniforms: &VersionedUniformRenderData,
    ) -> bool {
        if self.custom_data.uniforms_version >= custom_uniforms.version
            || self.custom_data.shader_version >= fragment_shader.version
        {
            return false;
        }

        self.fragment_shader = Some(
            device.create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("bulin_canvas.pipeline.fragment_shader"),
                source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                    format!(
                        "{}\n{}\n{}",
                        include_str!("shaders/uniforms.wgsl"),
                        custom_uniforms.data.uniforms_str,
                        fragment_shader.data
                    )
                    .as_str(),
                )),
            }),
        );
        true
    }

    fn update_custom_uniforms(
        &mut self,
        device: &wgpu::Device,
        custom_uniforms: &VersionedUniformRenderData,
    ) -> bool {
        if self.custom_data.uniforms_version >= custom_uniforms.version {
            return false;
        }

        let buffer_size = custom_uniforms.data.uniforms_bytes.read().unwrap().len() as u64;

        self.custom_data.buffer_data = if buffer_size > 0 {
            let custom_uniforms = device.create_buffer(&wgpu::BufferDescriptor {
                label: Some("bulin_canvas.pipeline.custom"),
                size: buffer_size,
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
            Some(BufferData {
                buffer: custom_uniforms,
                layout: custom_layout,
                bind_group: custom_uniform_bind_group,
            })
        } else {
            None
        };

        true
    }

    pub fn update_default_buffer(
        &self,
        queue: &wgpu::Queue,
        default_uniforms: &uniforms::DefaultUniforms,
    ) -> &Self {
        queue.write_buffer(
            &self.default_data.buffer,
            0,
            bytemuck::bytes_of(default_uniforms),
        );
        self
    }

    pub fn update_custom_buffer(
        &self,
        queue: &wgpu::Queue,
        custom_uniforms: &uniforms::CustomUniforms,
    ) -> &Self {
        if let Some(custom_data) = &self.custom_data.buffer_data {
            queue.write_buffer(
                &custom_data.buffer,
                0,
                bytemuck::cast_slice(custom_uniforms),
            );
        }
        self
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

        if let Some(pipeline) = &self.pipeline {
            pass.set_pipeline(pipeline);
        }
        pass.set_viewport(
            viewport.x as f32,
            viewport.y as f32,
            viewport.width as f32,
            viewport.height as f32,
            0.0,
            1.0,
        );

        pass.set_bind_group(0, &self.default_data.bind_group, &[]);
        if let Some(custom_buffer_data) = &self.custom_data.buffer_data {
            pass.set_bind_group(1, &custom_buffer_data.bind_group, &[]);
        }

        pass.draw(0..3, 0..1);
    }
}
