#![cfg_attr(
    all(not(debug_assertions), target_os = "windows"),
    windows_subsystem = "windows"
)]

use tauri::Manager;

mod config;
mod install;
mod mrpack;

fn main() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            install_mrpack,
            get_installed_metadata,
            is_launcher_installed,
            get_total_memory,
            remove_installation,
            open_game_folder
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

fn user_agent(app_handle: &tauri::AppHandle) -> String {
    format!(
        "VanillaFinestInstaller/{} (+https://github.com/Hiveko75/vanilla-finest-installer)",
        app_handle.package_info().version
    )
}

#[tauri::command]
async fn is_launcher_installed() -> bool {
    match install::launcher_path().await {
        Ok(path) => tokio::fs::try_exists(path.join("launcher_profiles.json"))
            .await
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// The installer's file of an earlier install in this game folder (pack version, edition), if any.
#[tauri::command]
async fn get_installed_metadata(game_dir: String) -> Option<serde_json::Value> {
    let dir = install::game_dir_path(&game_dir).await.ok()?;
    install::read_meta(&dir).await.map(|m| m.metadata)
}

/// Total memory of this computer in bytes (for the memory slider).
#[tauri::command]
fn get_total_memory() -> u64 {
    let mut system = sysinfo::System::new();
    system.refresh_memory();
    system.total_memory()
}

#[tauri::command]
async fn install_mrpack(
    app_handle: tauri::AppHandle,
    request: install::InstallRequest,
) -> Result<(), String> {
    let handle = app_handle.clone();
    let progress = move |payload: serde_json::Value| {
        let _ = handle.emit_all("install:progress", payload);
    };
    install::install(&request, &user_agent(&app_handle), &progress)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Removes an installation (see `install::uninstall`).
#[tauri::command]
async fn remove_installation(
    profile_id: String,
    game_dir: String,
) -> Result<install::RemoveResult, String> {
    install::uninstall(&profile_id, &game_dir)
        .await
        .map_err(|e| format!("{e:#}"))
}

/// Opens an installation's game folder (a folder name inside .minecraft) in the file manager.
#[tauri::command]
async fn open_game_folder(game_dir: String) -> Result<(), String> {
    install::open_game_folder(&game_dir)
        .await
        .map_err(|e| format!("{e:#}"))
}
