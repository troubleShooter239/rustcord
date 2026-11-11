fn main() {
    let commands = &["vencord", "editor_load", "editor_save"];
    let manifest = tauri_build::AppManifest::new().commands(commands);
    tauri_build::try_build(tauri_build::Attributes::new().app_manifest(manifest))
        .expect("could not configure the Tauri application");
}
