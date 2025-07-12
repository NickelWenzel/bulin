use crate::App;
use eframe::egui;
use egui_code_editor::{CodeEditor, ColorTheme};

/// UI implementation for the application
impl App {
    /// Main UI layout
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        // Split layout: Code editor on left, GPU texture on right
        ui.with_layout(
            egui::Layout::left_to_right(egui::Align::Min).with_cross_justify(true),
            |ui| {
                // Left panel: Code editor
                let font_size = 12.;
                ui.vertical(|ui| {
                    CodeEditor::default()
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
                        .show(ui, &mut self.code);
                });

                ui.separator();

                // Right panel: GPU texture display
                ui.vertical(|ui| {
                    if let Some(ref texture) = self.gpu_texture {
                        ui.add(
                            egui::Image::from_texture(texture)
                                .shrink_to_fit()
                                .maintain_aspect_ratio(false),
                        );
                    } else {
                        ui.label("GPU texture not yet rendered...");
                    }
                });
            },
        );
    }

    /// Menu bar implementation
    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    let app_new = {
                        let renderer = self.renderer.lock().unwrap();
                        Self::new(renderer.device.clone(), renderer.queue.clone())
                    };
                    *self = app_new;
                }
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
        });
    }
}
