use crate::viewer::canvasscene::uniforms;

use iced::{futures::executor::block_on, Rectangle};
use iced_wgpu::wgpu;

use std::borrow::Cow;

pub struct Pipeline {
    pipeline: wgpu::RenderPipeline,
    uniforms: wgpu::Buffer,
    uniform_bind_group: wgpu::BindGroup,
    pub version: usize,
}

impl Pipeline {
    pub fn new(device: &wgpu::Device, format: wgpu::TextureFormat, shader: &str, version: usize) -> Result<Self, wgpu::Error> {
        let uniforms = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("bulin_canvas.pipeline.uniforms"),
            size: std::mem::size_of::<uniforms::Uniforms>() as u64,
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
                resource: wgpu::BindingResource::Buffer(uniforms.as_entire_buffer_binding()),
            }],
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("bulin_canvas.pipeline.layout"),
            bind_group_layouts: &[&layout],
            push_constant_ranges: &[],
        });

        let vertex_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(concat!(
                include_str!("shaders/uniforms.wgsl"),
                "\n",
                include_str!("shaders/vertex_shader.wgsl"),
            ))),
        });


        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.fragment_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                format!("{}\n{}", include_str!("shaders/uniforms.wgsl"), shader).as_str(),
            )),
        });

        if let Some(error) = block_on( device.pop_error_scope()) {
            return Err(error);
        }

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

        Ok(Self {
            pipeline,
            uniforms,
            uniform_bind_group,
            version
        })
    }

    pub fn update(&mut self, queue: &wgpu::Queue, uniforms: &uniforms::Uniforms) {
        queue.write_buffer(&self.uniforms, 0, bytemuck::bytes_of(uniforms));
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
        //pass.set_scissor_rect(viewport.x, viewport.y, viewport.width, viewport.height);
        pass.set_bind_group(0, &self.uniform_bind_group, &[]);

        pass.draw(0..3, 0..1);
    }
}
