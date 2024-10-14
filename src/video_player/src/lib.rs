use wasm_bindgen::prelude::*;
use web_sys::HtmlVideoElement;

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_namespace = console)]
    fn log(s: &str);
}

#[wasm_bindgen]
pub fn play_video(video_id: &str) {
    let window = web_sys::window().unwrap();
    let document = window.document().unwrap();
    let video_element = document
        .get_element_by_id(video_id)
        .unwrap()
        .dyn_into::<HtmlVideoElement>()
        .unwrap();

    video_element.play().unwrap();
    log("Video started!");
}
