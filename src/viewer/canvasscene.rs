mod pipeline;
mod uniforms;

use std::sync::{Arc, Mutex};

use pipeline::{create_offscreen_bind_group, Pipeline};

use iced_wgpu::wgpu;

use iced::mouse;
use iced::widget::shader::{self, Viewport};
use iced::Rectangle;

#[derive(Debug, Clone)]
pub enum PipelineUpdate {
    Texture(Arc<wgpu::TextureView>),
}

type Texture = Arc<Mutex<Option<wgpu::TextureView>>>;

#[derive(Clone)]
pub struct CanvasScene {
    texture: Texture,
}

impl CanvasScene {
    pub fn new() -> Self {
        Self {
            texture: Arc::new(Mutex::new(None)),
        }
    }

    pub fn update(&mut self, message: PipelineUpdate) {
        match message {
            PipelineUpdate::Texture(texture) => {
                *self.texture.lock().unwrap() = Arc::try_unwrap(texture).ok();
            }
        }
    }
}

impl<Message> shader::Program<Message> for CanvasScene {
    type State = ();
    type Primitive = Primitive;

    fn draw(
        &self,
        _state: &Self::State,
        _cursor: mouse::Cursor,
        _bounds: Rectangle,
    ) -> Self::Primitive {
        Primitive {
            texture: self.texture.clone(),
        }
    }
}

#[derive(Debug)]
pub struct Primitive {
    texture: Texture,
}

impl shader::Primitive for Primitive {
    fn prepare(
        &self,
        device: &wgpu::Device,
        _queue: &wgpu::Queue,
        format: wgpu::TextureFormat,
        storage: &mut shader::Storage,
        _bounds: &Rectangle,
        _viewport: &Viewport,
    ) {
        if !storage.has::<Pipeline>() {
            storage.store(Pipeline::new(device, format));
        }

        if let Some(texture) = self.texture.lock().unwrap().as_ref() {
            // At this point our pipeline should always be initialized
            let pipeline = storage.get::<Pipeline>().unwrap();
            storage.store(create_offscreen_bind_group(
                device,
                texture,
                &pipeline.offscreen_bind_group_layout,
                &pipeline.sampler,
            ));
        }
    }

    fn render(
        &self,
        encoder: &mut wgpu::CommandEncoder,
        storage: &shader::Storage,
        target: &wgpu::TextureView,
        viewport: &Rectangle<u32>,
    ) {
        // At this point our pipeline should always be initialized
        let pipeline = storage.get::<Pipeline>().unwrap();
        if let Some(offscreen_bind_group) = storage.get::<wgpu::BindGroup>() {
            // Render primitive
            pipeline.render(offscreen_bind_group, target, encoder, viewport);
        }
    }
}
