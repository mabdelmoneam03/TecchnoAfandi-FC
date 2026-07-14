#![windows_subsystem = "windows"]

mod activation;
mod downloader;
mod logger;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use serde::Serialize;
use tauri::{Manager, Emitter};
use std::collections::VecDeque;

/// Shared cancellation flag
struct CancelFlag(Arc<AtomicBool>);
struct PauseFlag(Arc<AtomicBool>);

#[derive(Serialize)]
struct FolderDiagnostics {
    exe_dir: String,
    installer_found: bool,
    installer_path: Option<String>,
    top_level_entries: Vec<String>,
}

#[tauri::command]
fn get_exe_dir() -> Result<String, String> {
    let exe = std::env::current_exe()
        .map_err(|e| e.to_string())?
        .parent()
        .ok_or("Cannot get exe dir")?
        .to_path_buf();
    Ok(exe.to_string_lossy().to_string())
}

#[tauri::command]
fn check_game_folder(exe_dir: String) -> bool {
    let base = PathBuf::from(&exe_dir);
    activation::find_installer_xml(&base).is_some()
}

#[tauri::command]
fn get_folder_diagnostics(exe_dir: String) -> FolderDiagnostics {
    let base = PathBuf::from(&exe_dir);
    let found = activation::find_installer_xml(&base);

    let mut entries: Vec<String> = Vec::new();
    if let Ok(rd) = std::fs::read_dir(&base) {
        for e in rd.flatten() {
            if let Some(n) = e.file_name().to_str() {
                entries.push(n.to_string());
            }
        }
    }
    entries.sort();
    entries.truncate(50);

    FolderDiagnostics {
        exe_dir,
        installer_found: found.is_some(),
        installer_path: found.map(|p| p.to_string_lossy().to_string()),
        top_level_entries: entries,
    }
}

#[tauri::command]
fn get_game_version(exe_dir: String) -> Result<(String, String), String> {
    let base = PathBuf::from(&exe_dir);
    let xml_path = activation::find_installer_xml(&base)
        .ok_or_else(|| format!("Cannot find installer XML in {}", exe_dir))?;

    let content = std::fs::read_to_string(&xml_path)
        .map_err(|e| format!("Cannot read XML: {}", e))?;

    let version1 = activation::parse_game_version(&content)
        .ok_or("Version not found in XML")?;

    let v1_clone = version1.clone();
    let version2 = activation::map_version(&v1_clone);
    Ok((version1, version2.to_string()))
}

#[tauri::command]
async fn start_activation(
    app: tauri::AppHandle,
    exe_dir: String,
    selection: String,
) -> Result<(), String> {
    let dir = PathBuf::from(&exe_dir);
    let app_handle = app.clone();
    let sel = selection.clone();

    // Reset flags
    let flag = app.state::<CancelFlag>();
    flag.0.store(false, Ordering::Relaxed);
    let cancel = flag.0.clone();

    let pflag = app.state::<PauseFlag>();
    pflag.0.store(false, Ordering::Relaxed);
    let pause = pflag.0.clone();

    // Build a reusable HTTP client
    let client = reqwest::Client::builder()
        .pool_max_idle_per_host(128) // Crucial for many physical connections
        .http1_only()                // Forces multiple physical TCP connections instead of H2 multiplexing
        .tcp_nodelay(true)
        .connect_timeout(std::time::Duration::from_secs(10))
        .user_agent("TechnoAfandi-FC26-Tool/1.0")
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .map_err(|e| e.to_string())?;

    tokio::spawn(async move {
        activation::run_activation(app_handle, dir, sel, client, cancel, pause).await;
    });

    Ok(())
}

#[tauri::command]
fn cancel_activation(app: tauri::AppHandle) {
    let flag = app.state::<CancelFlag>();
    flag.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn pause_activation(app: tauri::AppHandle) {
    let flag = app.state::<PauseFlag>();
    flag.0.store(true, Ordering::Relaxed);
}

#[tauri::command]
fn resume_activation(app: tauri::AppHandle) {
    let flag = app.state::<PauseFlag>();
    flag.0.store(false, Ordering::Relaxed);
}

#[tauri::command]
fn open_url(url: String) -> Result<(), String> {
    open::that(&url).map_err(|e| e.to_string())
}

#[tauri::command]
fn clean_temp_files() {
    if let Ok(entries) = std::fs::read_dir(std::env::temp_dir()) {
        for entry in entries.flatten() {
            if let Some(n) = entry.file_name().to_str() {
                if n.starts_with(".dl_part_") {
                    let _ = std::fs::remove_file(entry.path());
                }
            }
        }
    }
}

#[tauri::command]
fn exit_app(app: tauri::AppHandle) {
    app.exit(0);
}

#[tauri::command]
fn get_app_version(app_handle: tauri::AppHandle) -> String {
    app_handle.package_info().version.to_string()
}

#[tauri::command]
fn get_current_exe_path() -> Result<String, String> {
    std::env::current_exe()
        .map(|p| p.to_string_lossy().to_string())
        .map_err(|e| e.to_string())
}

#[tauri::command]
fn copy_and_relaunch(target_path: String) -> Result<(), String> {
    let current = std::env::current_exe().map_err(|e| e.to_string())?;
    let target = std::path::Path::new(&target_path);
    
    // Try to rename the target first (Windows allows renaming running exes) to bypass locks
    let target_old = target.with_extension("exe.old");
    std::fs::rename(target, &target_old).ok();
    // Clean up the renamed file if it exists (might fail if still running, but that's ok, copy will succeed)
    std::fs::remove_file(&target_old).ok();

    // Copy the current exe to target
    std::fs::copy(&current, target).map_err(|e| e.to_string())?;
    
    // Launch the target
    std::process::Command::new(target)
        .spawn()
        .map_err(|e| e.to_string())?;
        
    // Exit current
    std::process::exit(0);
}

fn is_game_folder(path: &std::path::Path) -> bool {
    let dbdata = path.join("dbdata.dll");
    let xml = path.join("__Installer").join("installerdata.xml");
    dbdata.exists() && xml.exists()
}

#[tauri::command]
async fn auto_locate_game() -> Result<Option<String>, String> {
    tokio::task::spawn_blocking(|| {
        let drives = (b'C'..=b'Z')
            .map(|c| format!("{}:\\", c as char))
            .filter(|d| std::path::Path::new(d).exists())
            .collect::<Vec<_>>();

        // Skip folders that are heavily nested or OS specific
        let skip_dirs = [
            "windows", "programdata", "appdata", "system volume information", "$recycle.bin", "temp", "tmp", "perflogs"
        ];

        for drive in drives {
            let mut queue = VecDeque::new();
            queue.push_back(std::path::PathBuf::from(drive));

            while let Some(current) = queue.pop_front() {
                if is_game_folder(&current) {
                    return Ok(Some(current.to_string_lossy().to_string()));
                }

                if let Ok(entries) = std::fs::read_dir(&current) {
                    for entry in entries.flatten() {
                        if let Ok(file_type) = entry.file_type() {
                            if file_type.is_dir() {
                                let name = entry.file_name();
                                let name_str = name.to_string_lossy().to_lowercase();
                                
                                let mut skip = false;
                                for sd in &skip_dirs {
                                    if name_str == *sd {
                                        skip = true;
                                        break;
                                    }
                                }
                                if !skip {
                                    queue.push_back(entry.path());
                                }
                            }
                        }
                    }
                }
            }
        }
        Ok(None)
    }).await.map_err(|e| e.to_string())?
}

fn main() {
    let activator = include_bytes!("../assets/activator.exe");

    tauri::Builder::default()
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_dialog::init())
        .manage(activator.to_vec())
        .manage(CancelFlag(Arc::new(AtomicBool::new(false))))
        .manage(PauseFlag(Arc::new(AtomicBool::new(false))))
        .invoke_handler(tauri::generate_handler![
            get_exe_dir,
            get_app_version,
            get_current_exe_path,
            copy_and_relaunch,
            check_game_folder,
            auto_locate_game,
            get_folder_diagnostics,
            get_game_version,
            start_activation,
            cancel_activation,
            pause_activation,
            resume_activation,
            open_url,
            clean_temp_files,
            exit_app,
            activation::portable_update,
        ])
        .setup(|app| {
            // Clean up any .exe.old left from previous updates
            if let Ok(current) = std::env::current_exe() {
                let target_old = current.with_extension("exe.old");
                std::fs::remove_file(&target_old).ok();
            }
            if let Some(window) = app.get_webview_window("main") {
                let _ = window.set_always_on_top(true);
                let _ = window.set_focus();
                let w = window.clone();
                tauri::async_runtime::spawn(async move {
                    tokio::time::sleep(std::time::Duration::from_millis(150)).await;
                    let _ = w.set_always_on_top(false);
                });
            }
            Ok(())
        })
        .on_window_event(|window, event| {
            if let tauri::WindowEvent::CloseRequested { api, .. } = event {
                let temp_dir = std::env::temp_dir();
                let mut has_parts = false;
                if let Ok(entries) = std::fs::read_dir(&temp_dir) {
                    for entry in entries.flatten() {
                        if let Some(n) = entry.file_name().to_str() {
                            if n.starts_with(".dl_part_") {
                                has_parts = true;
                                break;
                            }
                        }
                    }
                }

                if has_parts {
                    api.prevent_close();
                    let _ = window.emit("show-exit-modal", ());
                }
            }
        })
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
