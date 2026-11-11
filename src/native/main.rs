#![cfg_attr(all(windows, not(debug_assertions)), windows_subsystem = "windows")]

mod app;
mod bridge;
mod plugin_network;
mod profile;
#[cfg(windows)]
mod windows_webview;

use tauri::Manager;

fn main() -> std::process::ExitCode {
    match run() {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("Rustcord: {error}");
            std::process::ExitCode::FAILURE
        }
    }
}

fn run() -> Result<(), Box<dyn std::error::Error>> {
    let options = app::Options::parse()?;
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .max_blocking_threads(16)
        .thread_keep_alive(std::time::Duration::from_secs(10))
        .thread_name("rustcord-io")
        .enable_all()
        .build()?;
    tauri::async_runtime::set(runtime.handle().clone());
    let app = tauri::Builder::default()
        .setup(move |app| app::setup(app, options))
        .invoke_handler(tauri::generate_handler![
            bridge::vencord,
            bridge::editor_load,
            bridge::editor_save
        ])
        .build(tauri::generate_context!())?;
    app.run(|handle, event| {
        if matches!(event, tauri::RunEvent::Exit)
            && let Some(profile) = handle.try_state::<profile::Profile>()
        {
            profile.unlock_on_exit();
        }
    });
    Ok(())
}
