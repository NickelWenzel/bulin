use crate::{
    App,
    render::{Renderer, update_texture},
};
use eframe::{
    egui,
    egui_wgpu::{self, CallbackResources, RenderState},
};
use egui::PaintCallbackInfo;
use egui_code_editor::{CodeEditor, ColorTheme};

/// UI implementation for the application
impl App {
    /// Main UI layout
    pub fn show(&mut self, ctx: &egui::Context, frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            // Split layout: Code editor on left, GPU texture on right
            ui.with_layout(
                egui::Layout::left_to_right(egui::Align::Min).with_cross_justify(true),
                |ui| {
                    // Left panel: Code editor
                    let font_size = 12.;
                    ui.vertical(|ui| {
                        if CodeEditor::default()
                            .id_source("code_editor")
                            .with_fontsize(font_size)
                            .with_rows({
                                let line_height =
                                    ui.fonts(|f| f.row_height(&egui::FontId::monospace(font_size)));
                                let available_height = ui.available_height();
                                (available_height / line_height).max(1.0) as usize
                            })
                            .with_theme(ColorTheme::GITHUB_DARK)
                            .with_numlines(true)
                            .desired_width(0.5 * ui.available_width())
                            .show(ui, &mut self.fragment_shader)
                            .response
                            .changed()
                        {
                            if let Some(RenderState {
                                device,
                                queue,
                                target_format,
                                ..
                            }) = frame.wgpu_render_state()
                            {
                                // Create or update the texture with the new shader code
                                update_texture(
                                    device,
                                    queue,
                                    *target_format,
                                    &self.texture_view,
                                    &self.fragment_shader,
                                );
                            }
                        }
                    });

                    ui.separator();

                    // Right panel: GPU texture display
                    ui.vertical(|ui| {
                        egui::Frame::canvas(ui.style()).show(ui, |ui| {
                            self.show_render(ui);
                        });
                    });
                },
            );
        });
    }

    fn show_render(&mut self, ui: &mut egui::Ui) {
        let (rect, ..) = ui.allocate_exact_size(ui.available_size(), egui::Sense::hover());

        ui.painter().add(egui_wgpu::Callback::new_paint_callback(
            rect,
            TextureCallback,
        ));
    }

    /// Menu bar implementation
    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    self.fragment_shader.clear()
                }
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
        });
    }
}

struct TextureCallback;

impl egui_wgpu::CallbackTrait for TextureCallback {
    fn paint(
        &self,
        _info: PaintCallbackInfo,
        render_pass: &mut wgpu::RenderPass<'static>,
        callback_resources: &CallbackResources,
    ) {
        let renderer: &Renderer = callback_resources.get().unwrap();
        renderer.render(render_pass);
    }
}
