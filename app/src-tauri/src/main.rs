use std::path::PathBuf;

use poe_layouts_core::{inspect_layout_database, LayoutDbSummary};

#[tauri::command]
fn inspect_layout_db(input: Option<PathBuf>) -> Result<LayoutDbSummary, String> {
    let input = input.unwrap_or_else(|| PathBuf::from("app/public/data/layouts.bin"));
    inspect_layout_database(&input).map_err(|error| error.to_string())
}

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![inspect_layout_db])
        .run(tauri::generate_context!())
        .expect("failed to run poe-layouts tauri app");
}
