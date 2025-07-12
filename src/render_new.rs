use crate::error::{AppError, RenderError};
use anyhow::{Context, Result};
use wgpu::*;

/// GPU renderer for creating textures and running shaders
pub struct Renderer {
    pub device: Device,
    pub queue: Queue,
    pub render_pipeline: Option<RenderPipeline>,
    pub texture: Option<Texture>,
    pub texture_view: Option<TextureView>,
}

impl Renderer {
    /// Create a new renderer instance
    pub async fn new() -> Result<Self, AppError> {
        // Initialize wgpu
        let instance = Instance::new(InstanceDescriptor {
            backends: Backends::all(),
            flags: InstanceFlags::default(),
            dx12_shader_compiler: Dx12Compiler::default(),
            gles_minor_version: Gles3MinorVersion::Automatic,
        });

        let adapter = instance
            .request_adapter(&RequestAdapterOptions {
                power_preference: PowerPreference::default(),
                compatible_surface: None,
                force_fallback_adapter: false,
            })
            .await
            .context("Failed to find an appropriate adapter")?;

        let (device, queue) = adapter
            .request_device(
                &DeviceDescriptor {
                    label: None,
                    required_features: Features::empty(),
                    required_limits: Limits::default(),
                },
                None,
            )
            .await
            .context("Failed to create device")?;

        // Create a basic texture for rendering
        let texture = device.create_texture(&TextureDescriptor {
            label: Some("render_texture"),
            size: Extent3d {
                width: 256,
                height: 256,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: TextureDimension::D2,
            format: TextureFormat::Rgba8UnormSrgb,
            usage: TextureUsages::TEXTURE_BINDING
                | TextureUsages::COPY_DST
                | TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });

        let texture_view = texture.create_view(&TextureViewDescriptor::default());

        Ok(Self {
            device,
            queue,
            render_pipeline: None,
            texture: Some(texture),
            texture_view: Some(texture_view),
        })
    }

    /// Initialize the render pipeline with a fragment shader
    pub fn init_pipeline(&mut self, shader_source: &str) -> Result<(), RenderError> {
        let shader = self.device.create_shader_module(ShaderModuleDescriptor {
            label: Some("fragment_shader"),
            source: ShaderSource::Wgsl(shader_source.into()),
        });

        let render_pipeline_layout =
            self.device
                .create_pipeline_layout(&PipelineLayoutDescriptor {
                    label: Some("render_pipeline_layout"),
                    bind_group_layouts: &[],
                    push_constant_ranges: &[],
                });

        let render_pipeline = self
            .device
            .create_render_pipeline(&RenderPipelineDescriptor {
                label: Some("render_pipeline"),
                layout: Some(&render_pipeline_layout),
                vertex: VertexState {
                    module: &shader,
                    entry_point: "vs_main",
                    buffers: &[],
                    compilation_options: PipelineCompilationOptions::default(),
                },
                fragment: Some(FragmentState {
                    module: &shader,
                    entry_point: "fs_main",
                    targets: &[Some(ColorTargetState {
                        format: TextureFormat::Rgba8UnormSrgb,
                        blend: Some(BlendState::REPLACE),
                        write_mask: ColorWrites::ALL,
                    })],
                    compilation_options: PipelineCompilationOptions::default(),
                }),
                primitive: PrimitiveState::default(),
                depth_stencil: None,
                multisample: MultisampleState::default(),
            });

        self.render_pipeline = Some(render_pipeline);
        Ok(())
    }

    /// Render a frame to the texture
    pub fn render_frame(&mut self) -> Result<(), RenderError> {
        if let (Some(pipeline), Some(texture_view)) = (&self.render_pipeline, &self.texture_view) {
            let mut encoder = self
                .device
                .create_command_encoder(&CommandEncoderDescriptor {
                    label: Some("render_encoder"),
                });

            {
                let mut render_pass = encoder.begin_render_pass(&RenderPassDescriptor {
                    label: Some("render_pass"),
                    color_attachments: &[Some(RenderPassColorAttachment {
                        view: texture_view,
                        resolve_target: None,
                        ops: Operations {
                            load: LoadOp::Clear(Color {
                                r: 0.1,
                                g: 0.2,
                                b: 0.3,
                                a: 1.0,
                            }),
                            store: StoreOp::Store,
                        },
                    })],
                    depth_stencil_attachment: None,
                    occlusion_query_set: None,
                    timestamp_writes: None,
                });

                render_pass.set_pipeline(pipeline);
                render_pass.draw(0..6, 0..1); // Draw a quad using vertex shader
            }

            self.queue.submit(std::iter::once(encoder.finish()));
        }

        Ok(())
    }

    /// Get the rendered texture data
    pub fn get_texture_data(&self) -> Result<Vec<u8>, RenderError> {
        // This is a placeholder - in a real implementation, you'd read back the texture data
        // For now, return some sample data
        Ok(vec![0u8; 256 * 256 * 4])
    }
}
