use crate::App;
use eframe::egui;

/// UI implementation for the application
impl App {
    pub fn ui(&mut self, ui: &mut egui::Ui) {
        ui.heading("My App UI");

        ui.horizontal(|ui| {
            let name_label = ui.label("Your name: ");
            ui.text_edit_singleline(&mut self.name)
                .labelled_by(name_label.id);
        });

        ui.add(egui::Slider::new(&mut self.value, 0.0..=100.0).text("value"));

        if ui.button("Increment").clicked() {
            self.value += 1.0;
        }

        if ui.button("Reset").clicked() {
            self.value = 0.0;
        }

        ui.separator();

        ui.label(format!("Hello '{}', value is {}", self.name, self.value));

        // Progress bar
        ui.add(egui::ProgressBar::new(self.value / 100.0).show_percentage());
    }

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
