use iced::Rectangle;
use iced_wgpu::wgpu;
use tracing::debug;

use crate::viewer::canvasscene::uniforms::DefaultUniforms;

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    texture_sampler: wgpu::Sampler,
    texture_bind_group: wgpu::BindGroup,
    texture: wgpu::Texture,
    texture_view: wgpu::TextureView,
}

impl Pipeline {
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        shader: &str,
        default_uniforms: &DefaultUniforms,
    ) -> Self {
        device.on_uncaptured_error(Box::new(|e| debug!("WGPU error: {e:#}")));

        let texture = create_texture(device, format, default_uniforms);
        let texture_view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let texture_sampler = device.create_sampler(&wgpu::SamplerDescriptor::default());

        update_texture(
            device,
            queue,
            format,
            &texture_view,
            shader,
            default_uniforms,
        );

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("BindGroupLayout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        multisampled: false,
                        view_dimension: wgpu::TextureViewDimension::D2,
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
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

        let texture_bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Postprocess Bind Group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&texture_view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&texture_sampler),
                },
            ],
        });
        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("Quad Pipeline Layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("Quad Shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shaders/texture.wgsl").into()),
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("Quad Pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: None,
                buffers: &[],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: None,
                targets: &[Some(wgpu::ColorTargetState {
                    format,
                    blend: Some(wgpu::BlendState::REPLACE),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            }),
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        Self {
            pipeline,
            texture_sampler,
            texture_bind_group,
            texture,
            texture_view,
        }
    }

    pub fn update_texture(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        shader: &str,
        default_uniforms: &DefaultUniforms,
    ) {
        update_texture(
            device,
            queue,
            format,
            &self.texture_view,
            shader,
            default_uniforms,
        );
    }

    pub fn render(
        &self,
        target: &wgpu::TextureView,
        encoder: &mut wgpu::CommandEncoder,
        viewport: &Rectangle<u32>,
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

        pass.set_bind_group(0, Some(&self.texture_bind_group), &[]);

        pass.draw(0..3, 0..1);
    }
}

pub fn update_texture(
    device: &wgpu::Device,
    queue: &wgpu::Queue,
    format: wgpu::TextureFormat,
    texture_view: &wgpu::TextureView,
    shader: &str,
    default_uniforms: &DefaultUniforms,
) {
    let buffer = device.create_buffer(&wgpu::BufferDescriptor {
        label: Some("bulin_canvas.pipeline.uniforms"),
        size: std::mem::size_of::<DefaultUniforms>() as u64,
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

    let shader = format!(
        "{}\n{}\n{}",
        include_str!("shaders/uniforms.wgsl"),
        include_str!("shaders/vertex_shader.wgsl"),
        shader
    );

    let shader_module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
        label: Some("Offscreen Shader"),
        source: wgpu::ShaderSource::Wgsl(shader.into()),
    });

    let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
        label: Some("Offscreen Pipeline Layout"),
        bind_group_layouts: &[&layout],
        push_constant_ranges: &[],
    });

    let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
        label: Some("Offscreen Pipeline"),
        layout: Some(&pipeline_layout),
        vertex: wgpu::VertexState {
            module: &shader_module,
            entry_point: None,
            buffers: &[],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        },
        fragment: Some(wgpu::FragmentState {
            module: &shader_module,
            entry_point: None,
            targets: &[Some(wgpu::ColorTargetState {
                format,
                blend: Some(wgpu::BlendState::REPLACE),
                write_mask: wgpu::ColorWrites::ALL,
            })],
            compilation_options: wgpu::PipelineCompilationOptions::default(),
        }),
        primitive: wgpu::PrimitiveState::default(),
        depth_stencil: None,
        multisample: wgpu::MultisampleState::default(),
        multiview: None,
        cache: None,
    });

    let mut encoder = device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
        label: Some("Offscreen Encoder"),
    });

    queue.write_buffer(&buffer, 0, bytemuck::bytes_of(default_uniforms));

    {
        let mut render_pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("Offscreen Pass"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: texture_view,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Clear(wgpu::Color::BLACK),
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
        });

        render_pass.set_pipeline(&pipeline);
        render_pass.set_bind_group(0, &bind_group, &[]);
        render_pass.draw(0..3, 0..1); // full screen triangle
    }

    // Wait for the texture to be ready
    queue.submit(Some(encoder.finish()));
}

fn create_texture(
    device: &wgpu::Device,
    format: wgpu::TextureFormat,
    default_uniforms: &DefaultUniforms,
) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("Offscreen Render Target"),
        size: wgpu::Extent3d {
            width: default_uniforms.resolution[0] as u32,
            height: default_uniforms.resolution[1] as u32,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format,
        usage: wgpu::TextureUsages::RENDER_ATTACHMENT
            | wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_SRC,
        view_formats: &[],
    })
}
