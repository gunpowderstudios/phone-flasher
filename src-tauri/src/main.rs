// Don't open a console window alongside the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;

use flasher_core::{DeviceCard, FirmwareSummary};
use flasher_engine::{Details, Engine, RestoreEvent, ToolReport};
use tauri::{AppHandle, Emitter, State};

type SharedEngine<'a> = State<'a, Arc<Engine>>;

/// Every command runs its (blocking) tool calls off the UI thread.
async fn blocking<T: Send + 'static>(f: impl FnOnce() -> T + Send + 'static) -> Result<T, String> {
    tauri::async_runtime::spawn_blocking(f).await.map_err(|e| e.to_string())
}

#[tauri::command]
async fn tool_report(engine: SharedEngine<'_>) -> Result<ToolReport, String> {
    let e = engine.inner().clone();
    blocking(move || e.tool_report()).await
}

#[tauri::command]
async fn scan_devices(engine: SharedEngine<'_>) -> Result<Vec<DeviceCard>, String> {
    let e = engine.inner().clone();
    blocking(move || e.scan()).await
}

#[tauri::command]
async fn device_details(engine: SharedEngine<'_>, id: String) -> Result<Details, String> {
    let e = engine.inner().clone();
    blocking(move || e.details(&id)).await?
}

#[tauri::command]
async fn pair_device(engine: SharedEngine<'_>, udid: String) -> Result<String, String> {
    let e = engine.inner().clone();
    blocking(move || e.pair(&udid)).await?
}

#[tauri::command]
async fn latest_firmware(identifier: String) -> Result<FirmwareSummary, String> {
    blocking(move || flasher_engine::firmware_for(&identifier)).await?
}

#[tauri::command]
async fn start_restore(
    app: AppHandle,
    engine: SharedEngine<'_>,
    id: String,
    override_blocked: bool,
) -> Result<(), String> {
    let e = engine.inner().clone();
    blocking(move || {
        e.start_restore(&id, override_blocked, move |event| {
            let name = match event {
                RestoreEvent::Log { .. } => "restore-log",
                RestoreEvent::Done { .. } => "restore-done",
            };
            let _ = app.emit(name, event);
        })
    })
    .await?
}

#[tauri::command]
fn cancel_restore(engine: SharedEngine<'_>) {
    engine.cancel_restore();
}

#[tauri::command]
fn restore_running(engine: SharedEngine<'_>) -> bool {
    engine.is_restoring()
}

fn main() {
    tauri::Builder::default()
        .manage(Arc::new(Engine::new()))
        .invoke_handler(tauri::generate_handler![
            tool_report,
            scan_devices,
            device_details,
            pair_device,
            latest_firmware,
            start_restore,
            cancel_restore,
            restore_running,
        ])
        .run(tauri::generate_context!())
        .expect("Flasher failed to start");
}
