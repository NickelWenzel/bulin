use crate::App;
use eframe::egui;
use egui_code_editor::{CodeEditor, ColorTheme};

/// UI implementation for the application
impl App {
    /// Main UI layout
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("Bulin Cross-Platform GUI App");

        // Top panel with controls
        ui.horizontal(|ui| {
            let name_label = ui.label("App Name:");
            ui.text_edit_singleline(&mut self.name)
                .labelled_by(name_label.id);

            ui.separator();

            ui.add(egui::Slider::new(&mut self.value, 0.0..=100.0).text("Value"));

            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }

            if ui.button("Reset").clicked() {
                self.value = 0.0;
            }
        });

        ui.separator();

        // Split layout: Code editor on left, GPU texture on right
        ui.horizontal(|ui| {
            // Left panel: Code editor
            ui.vertical(|ui| {
                ui.heading("Code Editor");
                ui.label("Edit your code here:");

                let theme = ColorTheme::GITHUB_DARK;

                egui::ScrollArea::vertical()
                    .max_height(300.0)
                    .show(ui, |ui| {
                        CodeEditor::default()
                            .id_source("code_editor")
                            .with_rows(15)
                            .with_fontsize(12.0)
                            .with_theme(theme)
                            .with_syntax(egui_code_editor::Syntax::rust())
                            .with_numlines(true)
                            .show(ui, &mut self.code);
                    });
            });

            ui.separator();

            // Right panel: GPU texture display
            ui.vertical(|ui| {
                ui.heading("GPU Rendered Texture");

                if let Some(ref texture) = self.gpu_texture {
                    ui.add(
                        egui::Image::from_texture(texture)
                            .fit_to_exact_size(egui::Vec2::new(128.0, 128.0)),
                    );
                } else {
                    ui.label("GPU texture not yet rendered...");
                }

                ui.separator();

                ui.label("Background Task Status:");
                ui.label("✓ Background processing active");

                // Progress indicator
                ui.add(egui::ProgressBar::new(self.value / 100.0).show_percentage());
            });
        });

        ui.separator();

        // Bottom status bar
        ui.horizontal(|ui| {
            ui.label(format!("Status: {}", self.name));
            ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
                ui.label(format!("Value: {:.1}", self.value));
            });
        });
    }

    /// Menu bar implementation
    pub fn menu_bar(&mut self, ui: &mut egui::Ui) {
        egui::MenuBar::new().ui(ui, |ui| {
            ui.menu_button("File", |ui| {
                if ui.button("New").clicked() {
                    *self = App::new();
                }
                if ui.button("Quit").clicked() {
                    std::process::exit(0);
                }
            });
            ui.menu_button("Edit", |ui| {
                if ui.button("Reset Value").clicked() {
                    self.value = 0.0;
                }
            });
        });
    }
}
