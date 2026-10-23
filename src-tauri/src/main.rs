// Don't open a console window alongside the app on Windows release builds.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::sync::Arc;
use serde::{Deserialize, Serialize};

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

const RELEASES_URL: &str = "https://github.com/gunpowderstudios/phone-flasher/releases";

#[derive(Deserialize)]
struct GithubRelease { tag_name: String, html_url: String }

#[derive(Serialize)]
struct UpdateReport { message: String }

#[tauri::command]
fn app_version(app: AppHandle) -> String {
    app.package_info().version.to_string()
}

#[tauri::command]
async fn check_app_update(app: AppHandle) -> Result<UpdateReport, String> {
    let installed = app.package_info().version.to_string();
    blocking(move || {
        let response: GithubRelease = ureq::get("https://api.github.com/repos/gunpowderstudios/phone-flasher/releases/latest")
            .set("User-Agent", "Flasher-update-check")
            .timeout(std::time::Duration::from_secs(12))
            .call()
            .map_err(|e| format!("Cannot check published releases: {e}. Draft releases are not included."))?
            .into_json()
            .map_err(|e| format!("Invalid release information: {e}"))?;
        if !response.html_url.starts_with(RELEASES_URL) {
            return Err("Unexpected release URL".into());
        }
        let latest = response.tag_name.trim_start_matches('v');
        let current = semver::Version::parse(&installed).map_err(|e| e.to_string())?;
        let remote = semver::Version::parse(latest).map_err(|e| format!("Invalid release version: {e}"))?;
        let message = if remote > current {
            format!("New version {remote} available (installed: {current}). Select Open downloads to install it.")
        } else {
            format!("You're up to date (version {current}).")
        };
        Ok(UpdateReport { message })
    }).await?
}

#[tauri::command]
fn open_release_page() -> Result<(), String> {
    #[cfg(target_os = "macos")]
    let mut command = std::process::Command::new("open");
    #[cfg(target_os = "windows")]
    let mut command = {
        let mut cmd = std::process::Command::new("cmd");
        cmd.args(["/C", "start", ""]);
        cmd
    };
    #[cfg(all(not(target_os = "macos"), not(target_os = "windows")))]
    let mut command = std::process::Command::new("xdg-open");
    command.arg(RELEASES_URL).spawn().map_err(|e| format!("Could not open downloads: {e}"))?;
    Ok(())
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
            app_version,
            check_app_update,
            open_release_page,
        ])
        .run(tauri::generate_context!())
        .expect("Flasher failed to start");
}
