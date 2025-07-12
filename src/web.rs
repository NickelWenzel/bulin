#[cfg(target_arch = "wasm32")]
use wasm_bindgen::prelude::*;

#[cfg(target_arch = "wasm32")]
#[wasm_bindgen]
pub fn start(canvas_id: &str) -> Result<(), JsValue> {
    use eframe::wasm_bindgen::JsCast as _;

    // Redirect `log` message to `console.log` and friends:
    eframe::WebLogger::init(log::LevelFilter::Debug).ok();

    let web_options = eframe::WebOptions::default();
    let canvas_id = canvas_id.to_string(); // Convert to owned string

    wasm_bindgen_futures::spawn_local(async move {
        let document = web_sys::window()
            .expect("No window")
            .document()
            .expect("No document");

        let canvas = document
            .get_element_by_id(&canvas_id)
            .expect("Failed to find canvas")
            .dyn_into::<web_sys::HtmlCanvasElement>()
            .expect("Canvas element was not a HtmlCanvasElement");

        let start_result = eframe::WebRunner::new()
            .start(
                canvas,
                web_options,
                Box::new(|_| {
                    let app = bulin::App::new();
                    // Note: In WASM, we can't easily use tokio::block_in_place
                    // so we'll initialize without async for now
                    Ok(Box::new(app))
                }),
            )
            .await;

        // Handle loading completion
        if let Some(loading_text) = document.get_element_by_id("loading") {
            match start_result {
                Ok(_) => {
                    loading_text.set_attribute("style", "display: none;").ok();
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

    Ok(())
}

#[cfg(not(target_arch = "wasm32"))]
fn main() {
    // This binary is only for WebAssembly targets
    panic!("This binary is only for WebAssembly targets");
}

#[cfg(target_arch = "wasm32")]
fn main() {
    // This is not used when building for WASM
    // The start function is called from JavaScript
}
