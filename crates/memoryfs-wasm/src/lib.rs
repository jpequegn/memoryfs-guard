use wasm_bindgen::prelude::wasm_bindgen;

#[wasm_bindgen]
#[must_use]
pub fn version() -> String {
    memoryfs_core::version().to_owned()
}
