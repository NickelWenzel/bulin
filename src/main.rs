#![warn(clippy::all)]
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")] // hide console window on Windows in release

#[cfg(not(target_arch = "wasm32"))]
#[tokio::main]
async fn main() -> eframe::Result<()> {
    use bulin::App;
    use eframe::egui;

    // Set up logging
    env_logger::init();

    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default()
            .with_inner_size([800.0, 600.0])
            .with_min_inner_size([600.0, 400.0]),
        ..Default::default()
    };

    eframe::run_native(
        "Bulin",
        options,
        Box::new(|cc| {
            let wgpu_render_state = cc
                .wgpu_render_state
                .as_ref()
                .ok_or_else(|| anyhow::anyhow!("WGPU render state is not available"))?;

            Ok(Box::new(App::new(
                wgpu_render_state.device.clone(),
                wgpu_render_state.queue.clone(),
            )))
        }),
    )
}

#[cfg(target_arch = "wasm32")]
fn main() {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    wasm_bindgen_futures::spawn_local(async move {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id("the_canvas_id")
            .expect("Failed to find the_canvas_id")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("Canvas element was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                eframe::WebOptions::default(),
                Box::new(|cc| {
                    let wgpu_render_state = cc
                        .wgpu_render_state
                        .as_ref()
                        .ok_or_else(|| anyhow::anyhow!("WGPU render state is not available"))?;

                    Ok(Box::new(bulin::App::new(
                        wgpu_render_state.device.clone(),
                        wgpu_render_state.queue.clone(),
                    )))
                }),
            )
            .await;

        // Handle loading completion
        if let Some(loading_text) = document.get_element_by_id("loading_text") {
            match start_result {
                Ok(_) => {
                    loading_text.remove();
                }
                Err(e) => {
                    loading_text.set_inner_html(
                        "<p>The app has crashed. See the developer console for details.</p>",
                    );
                    panic!("Failed to start eframe: {e:?}");
                }
            }
        }
    });
}
