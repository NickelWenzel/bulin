pub mod background;
pub mod render;
pub mod ui;

/// Shared application state
pub struct App {
    pub name: String,
    pub value: f32,
}

impl Default for App {
    fn default() -> Self {
        Self {
            name: "My App".to_string(),
            value: 0.0,
        }
    }
}

impl App {
    pub fn new() -> Self {
        Self::default()
    }
}

impl eframe::App for App {
    fn update(&mut self, ctx: &egui::Context, _frame: &mut eframe::Frame) {
        egui::CentralPanel::default().show(ctx, |ui| {
            ui.heading("My App");
            ui.horizontal(|ui| {
                let name_label = ui.label("Your name: ");
                ui.text_edit_singleline(&mut self.name)
                    .labelled_by(name_label.id);
            });
            ui.add(egui::Slider::new(&mut self.value, 0.0..=100.0).text("value"));
            if ui.button("Increment").clicked() {
                self.value += 1.0;
            }
            ui.label(format!("Hello '{}', value is {}", self.name, self.value));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_app_creation() {
        let app = App::new();
        assert_eq!(app.name, "My App");
        assert_eq!(app.value, 0.0);
    }
}
