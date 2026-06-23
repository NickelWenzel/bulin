use crate::viewer::canvasscene::{uniforms, UniformRenderData};

use iced::{futures::executor::block_on, wgpu::PipelineCompilationOptions};
use iced_wgpu::{primitive, wgpu};

use std::borrow::Cow;

use super::{VersionedShader, VersionedUniformRenderData};

pub struct Pipeline {
    pipeline: Option<wgpu::RenderPipeline>,
    default_data: BufferData,
    custom_data: CustomBufferData,
    vertex_shader: wgpu::ShaderModule,
    fragment_shader: wgpu::ShaderModule,
    format: wgpu::TextureFormat,
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

impl primitive::Pipeline for Pipeline {
    fn new(device: &wgpu::Device, _queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self
    where
        Self: Sized,
    {
        Self::create(device, format)
    }
}

impl Pipeline {
    fn create(device: &wgpu::Device, format: wgpu::TextureFormat) -> Self {
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

        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.fragment_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed("")),
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
            fragment_shader,
            format,
        }
    }

    pub fn update(
        &mut self,
        device: &wgpu::Device,
        versioned_fragment_shader: &VersionedShader,
        versioned_custom_uniforms: &VersionedUniformRenderData,
    ) -> Result<&mut Self, String> {
        let (custom_uniforms_need_update, shader_needs_update) = self.update_versions(
            versioned_custom_uniforms.version,
            versioned_fragment_shader.version,
        );

        if !(custom_uniforms_need_update || shader_needs_update) {
            return Ok(self);
        }

        if custom_uniforms_need_update {
            match Pipeline::create_custom_uniforms(device, &versioned_custom_uniforms.data) {
                Ok(custom_uniforms_data) => self.custom_data.buffer_data = custom_uniforms_data,
                Err(e) => {
                    self.pipeline = None;
                    return Err(e);
                }
            }
        }

        if shader_needs_update {
            match Pipeline::create_fragment_shader(
                device,
                &versioned_fragment_shader.data,
                &versioned_custom_uniforms.data.uniforms_str,
            ) {
                Ok(fragment_shader) => self.fragment_shader = fragment_shader,
                Err(e) => {
                    self.pipeline = None;
                    return Err(e);
                }
            }
        }

        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let pipeline_layout_descriptor = wgpu::PipelineLayoutDescriptor {
            label: Some("bulin_canvas.pipeline.layout"),
            bind_group_layouts: if let Some(BufferData {
                layout: custom_layout,
                ..
            }) = &self.custom_data.buffer_data
            {
                &[&self.default_data.layout, custom_layout]
            } else {
                &[&self.default_data.layout]
            },
            push_constant_ranges: &[],
        };

        let pipeline_layout = device.create_pipeline_layout(&pipeline_layout_descriptor);

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("bulin_canvas.pipeline.pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &self.vertex_shader,
                entry_point: Some("vs_main"),
                buffers: &[],
                compilation_options: PipelineCompilationOptions::default(),
            },
            primitive: wgpu::PrimitiveState::default(),
            depth_stencil: None,
            multisample: Default::default(),
            fragment: Some(wgpu::FragmentState {
                module: &self.fragment_shader,
                entry_point: Some("fs_main"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: self.format,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: PipelineCompilationOptions::default(),
            }),
            multiview: None,
            cache: None,
        });

        if let Some(error) = block_on(device.pop_error_scope()) {
            self.pipeline = None;
            return Err(error.to_string());
        }
        self.pipeline = Some(pipeline);
        Ok(self)
    }

    fn update_versions(
        &mut self,
        custom_uniforms_version: usize,
        fragment_shader_version: usize,
    ) -> (bool, bool) {
        let custom_uniforms_need_update =
            self.custom_data.uniforms_version < custom_uniforms_version;
        let shader_needs_update = self.custom_data.uniforms_version < custom_uniforms_version
            || self.custom_data.shader_version < fragment_shader_version;

        self.custom_data.uniforms_version = custom_uniforms_version;
        self.custom_data.shader_version = fragment_shader_version;

        (custom_uniforms_need_update, shader_needs_update)
    }

    fn create_fragment_shader(
        device: &wgpu::Device,
        fragment_shader: &str,
        custom_uniforms: &str,
    ) -> Result<wgpu::ShaderModule, String> {
        device.push_error_scope(wgpu::ErrorFilter::Validation);
        let fragment_shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("bulin_canvas.pipeline.fragment_shader"),
            source: wgpu::ShaderSource::Wgsl(Cow::Borrowed(
                format!(
                    "{}\n{}\n{}",
                    include_str!("shaders/uniforms.wgsl"),
                    custom_uniforms,
                    fragment_shader
                )
                .as_str(),
            )),
        });
        if let Some(error) = block_on(device.pop_error_scope()) {
            return Err(error.to_string());
        }
        Ok(fragment_shader)
    }

    fn create_custom_uniforms(
        device: &wgpu::Device,
        custom_uniforms: &UniformRenderData,
    ) -> Result<Option<BufferData>, String> {
        let buffer_size = custom_uniforms.uniforms_bytes.read().unwrap().len() as u64;
        if buffer_size == 0 {
            return Ok(None);
        }

        device.push_error_scope(wgpu::ErrorFilter::Validation);
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
                resource: wgpu::BindingResource::Buffer(custom_uniforms.as_entire_buffer_binding()),
            }],
        });

        if let Some(error) = block_on(device.pop_error_scope()) {
            return Err(error.to_string());
        }

        Ok(Some(BufferData {
            buffer: custom_uniforms,
            layout: custom_layout,
            bind_group: custom_uniform_bind_group,
        }))
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

    pub fn draw(&self, render_pass: &mut wgpu::RenderPass<'_>) {
        let Some(pipeline) = &self.pipeline else {
            return;
        };

        render_pass.set_pipeline(pipeline);

        render_pass.set_bind_group(0, &self.default_data.bind_group, &[]);
        if let Some(custom_buffer_data) = &self.custom_data.buffer_data {
            render_pass.set_bind_group(1, &custom_buffer_data.bind_group, &[]);
        }

        render_pass.draw(0..3, 0..1);
    }
}
