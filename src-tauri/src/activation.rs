use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::os::windows::process::CommandExt;
use tauri::AppHandle;
use tauri::Emitter;
use tauri::Manager;
use serde::Serialize;

// GitHub hosting strategy:
// - raw.githubusercontent.com works for regular files but returns a small
//   pointer text (~130 bytes) for files stored via Git LFS.
// - media.githubusercontent.com/media/ serves the actual LFS binaries but
//   returns 404 for non-LFS files.
// The download logic below tries raw first, and if it detects an LFS pointer,
// automatically retries with the media URL. So we can safely use raw here.
const ACTIVATION64_URL: &str = "https://raw.githubusercontent.com/mabdelmoneam03/EA-SPORTS-FC-26/main/Activation64.dll";
const CRYPTBASE0_URL: &str = "https://raw.githubusercontent.com/mabdelmoneam03/EA-SPORTS-FC-26/main/CryptBase0.dll";
// FC26.exe is large (~428 MB) so it's hosted on GitHub Releases instead of
// raw/LFS — Releases has no rate-limiting and is served via a fast CDN.
const FC26_URL: &str = "https://github.com/mabdelmoneam03/EA-SPORTS-FC-26/releases/download/V111/FC26.exe";
const ANADIUS64_URL: &str = "https://raw.githubusercontent.com/mabdelmoneam03/EA-SPORTS-FC-26/main/anadius64.dll";
const FMM_ZIP_RAW: &str = "https://raw.githubusercontent.com/mabdelmoneam03/EA-SPORTS-FC-26/main/FMM.zip";
const LE_ZIP_RAW: &str = "https://raw.githubusercontent.com/mabdelmoneam03/EA-SPORTS-FC-26/main/Live%20Editor.zip";

#[derive(Clone, Serialize)]
struct ProgressPayload {
    percent: f64,
    label: String,
}

/// Finds the installer XML file across common EA folder naming conventions.
pub fn find_installer_xml(base: &Path) -> Option<PathBuf> {
    let candidates = [
        "__Installer/installerdata.xml",
        "_installer/installerdata.xml",
        "_Installer/installerdata.xml",
        "__installer/installerdata.xml",
    ];
    for c in candidates {
        let p = base.join(c);
        if p.exists() {
            return Some(p);
        }
    }
    None
}

pub fn parse_game_version(xml: &str) -> Option<String> {
    use quick_xml::Reader;
    use quick_xml::events::Event;
    let mut reader = Reader::from_str(xml);
    let mut buf = Vec::new();
    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Empty(e)) => {
                if e.name().as_ref() == b"gameVersion" {
                    for attr in e.attributes().flatten() {
                        if attr.key.as_ref() == b"version" {
                            return Some(String::from_utf8_lossy(&attr.value).to_string());
                        }
                    }
                }
            }
            Ok(Event::Eof) => break,
            Err(_) => break,
            _ => {}
        }
        buf.clear();
    }
    None
}

pub fn map_version(v1: &str) -> &str {
    match v1 {
        "1.0.127.4828" => "1.0.0",
        "1.0.127.59053" => "1.0.1",
        "1.0.128.4697" => "1.0.2",
        "1.0.128.17361" => "1.0.3",
        "1.0.128.29120" => "1.0.4",
        "1.0.128.37607" => "1.1.0",
        "1.0.128.60171" => "1.1.1",
        "1.0.128.63165" => "1.1.2",
        "1.0.129.1902" => "1.1.3",
        "1.0.129.4059" => "1.2.0",
        "1.0.129.25108" => "1.2.1",
        "1.0.129.30822" => "1.3.0",
        "1.0.130.16994" => "1.4.0",
        "1.0.130.35129" => "1.4.1",
        "1.0.131.24706" => "1.4.2",
        "1.0.131.50017" => "1.4.3",
        "1.0.132.29676" => "1.5.0",
        "1.0.133.14157" => "1.5.1",
        "1.0.133.58379" => "1.5.2",
        "1.0.134.1759" => "1.5.3",
        "1.0.134.63314" => "1.5.4",
        "1.0.135.39173" => "1.5.5",
        "1.0.135.54147" => "1.5.6",
        "1.0.136.4893" => "1.6.0",
        "1.0.136.44486" => "1.6.1",
        "1.0.136.57334" => "1.6.2",
        "1.0.137.49763" => "1.6.3",
        "1.0.138.16746" => "1.6.4",
        _ => "Unknown",
    }
}

/// Detects if a downloaded file is actually a Git LFS pointer text rather
/// than the real binary. LFS pointers are small (~130 bytes) and always
/// contain lines like `version https://git-lfs.github.com/spec/v1` and
/// `oid sha256:...`.
fn is_lfs_pointer(path: &Path) -> bool {
    let metadata = match std::fs::metadata(path) {
        Ok(m) => m,
        Err(_) => return false,
    };
    // LFS pointers are always small. Real binaries never fit in this range.
    if metadata.len() > 1024 {
        return false;
    }
    let content = std::fs::read_to_string(path).unwrap_or_default();
    content.contains("git-lfs") || content.contains("oid sha256")
}

/// SHA-256 hash of a file — for duplicate detection
fn sha256_file(path: &Path) -> Result<String, String> {
    use sha2::{Sha256, Digest};
    let content = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha256::new();
    hasher.update(&content);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}
fn format_time(secs: u64) -> String {
    let m = secs / 60;
    let s = secs % 60;
    if m > 59 {
        let h = m / 60;
        let m = m % 60;
        format!("{:02}:{:02}:{:02}", h, m, s)
    } else {
        format!("{:02}:{:02}", m, s)
    }
}


/// Get remote file size using reqwest
async fn get_remote_file_size(client: &reqwest::Client, url: &str) -> Option<u64> {
    if let Ok(resp) = client.get(url).header("Range", "bytes=0-0").send().await {
        if let Some(cr) = resp.headers().get(reqwest::header::CONTENT_RANGE) {
            if let Ok(cr_str) = cr.to_str() {
                if let Some(total_str) = cr_str.rsplit('/').next() {
                    if let Ok(total) = total_str.trim().parse::<u64>() {
                        if total > 0 { return Some(total); }
                    }
                }
            }
        }
        if let Some(len) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(s) = len.to_str() {
                if let Ok(num) = s.parse::<u64>() {
                    if num > 0 { return Some(num); }
                }
            }
        }
    }
    
    if let Ok(resp) = client.head(url).send().await {
        if let Some(len) = resp.headers().get(reqwest::header::CONTENT_LENGTH) {
            if let Ok(s) = len.to_str() {
                if let Ok(num) = s.parse::<u64>() {
                    if num > 0 { return Some(num); }
                }
            }
        }
    }
    None
}

/// Check if a file already exists and matches the remote file.
/// Returns true if the file should be skipped (already identical).
fn file_already_exists(
    dest: &Path,
    app: &AppHandle,
    label: &str,
    remote_size: Option<u64>,
    progress_end: f64,
) -> bool {
    // File doesn't exist → need to download
    let local_meta = match std::fs::metadata(dest) {
        Ok(m) => m,
        Err(_) => return false,
    };

    let local_size = local_meta.len();

    // Zero-size file → corrupted/incomplete → re-download
    if local_size == 0 {
        return false;
    }

    // If we know the remote size and it doesn't match → re-download
    if let Some(remote) = remote_size {
        if local_size != remote {
            return false;
        }
    }

    // Size matches (or unknown remote size but file exists with content)
    // → compute SHA-256 for verification and logging
    let hash = sha256_file(dest).unwrap_or_else(|_| "unknown".to_string());
    let mb = local_size as f64 / 1_048_576.0;

    let _ = app.emit("activation-progress", ProgressPayload {
        percent: progress_end,
        label: format!("{} · {:.1} MB · ✓ Exists (SHA-256: {}...)",
            label, mb, &hash[..12.min(hash.len())]),
    });

    true // skip download
}


/// Converts a raw.githubusercontent.com URL into the media.githubusercontent.com
/// LFS-media URL that serves the real binary content.
fn to_lfs_media_url(raw_url: &str) -> Option<String> {
    if raw_url.contains("raw.githubusercontent.com/") {
        Some(raw_url.replace(
            "raw.githubusercontent.com/",
            "media.githubusercontent.com/media/",
        ))
    } else {
        None
    }
}

/// Smart download with duplicate detection and LFS fallback.
/// - Checks if file already exists (size + SHA-256) → skips if identical
/// - Tries raw URL first, auto-retries with LFS media URL if needed
async fn download_file_smart(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    app: &AppHandle,
    label: &str,
    progress_start: f64,
    progress_end: f64,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
    game_dir: &Path,
    optimal_connections: usize,
) -> Result<(), String> {
    let mut current_url = url.to_string();
    let mut accumulated_time: u64 = 0;
    let mut retry_count = 0;
    // ━━━ Download with pause/resume support ━━━
    loop {
        // Check if file already exists (might exist after resume)
        let remote_size = get_remote_file_size(client, &current_url).await;
        if file_already_exists(dest, app, label, remote_size, progress_end) {
            // Check if it's an LFS pointer
            if is_lfs_pointer(dest) {
                std::fs::remove_file(dest).ok();
                if let Some(media_url) = to_lfs_media_url(&current_url) {
                    current_url = media_url;
                    let _ = app.emit("activation-progress", ProgressPayload {
                        percent: progress_start,
                        label: format!("{} (LFS — retrying via media URL)", label),
                    });
                    continue;
                } else {
                    return Err(format!("LFS detected but cannot build media URL from: {}", current_url));
                }
            }
            return Ok(());
        }

        let file_size_mb = remote_size.unwrap_or(0) as f64 / 1_048_576.0;
        let mut num_parts = if file_size_mb < 2.0 { 1 } else { optimal_connections };

        // ━━━ FIX: Force num_parts to match existing temp files to prevent chunk corruption upon restart ━━━
        let file_name = dest.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
        let part_temp = std::env::temp_dir();
        let mut existing_parts = 0;
        while part_temp.join(format!(".dl_part_{}_{}", file_name, existing_parts)).exists() {
            existing_parts += 1;
        }
        if existing_parts > 1 {
            num_parts = existing_parts;
            crate::logger::log_msg(game_dir, &format!("Found {} existing parts for {}, forcing num_parts to match.", existing_parts, label));
        }

        // Download attempt
        let result = download_file_stream(client, &current_url, dest, app, label,
            progress_start, progress_end, cancel.clone(), pause.clone(), num_parts, remote_size, accumulated_time).await;

        match result {
            Ok(()) => {
                // Check if it's an LFS pointer
                if is_lfs_pointer(dest) {
                    std::fs::remove_file(dest).ok();
                    if let Some(media_url) = to_lfs_media_url(&current_url) {
                        current_url = media_url;
                        let _ = app.emit("activation-progress", ProgressPayload {
                            percent: progress_start,
                            label: format!("{} (LFS — retrying via media URL)", label),
                        });
                        continue;
                    } else {
                        return Err(format!("LFS detected but cannot build media URL from: {}", current_url));
                    }
                }
                crate::logger::log_msg(game_dir, &format!("✓ Successfully downloaded {}", label));
                break;
            }, // success
            Err((e, t)) if e == "Resumed" => { accumulated_time = t; continue; },
            Err((e, t)) if e.starts_with("Retry") => {
                let actual_err = e.split('|').nth(1).unwrap_or("Unknown");
                accumulated_time = t;
                retry_count += 1;
                crate::logger::log_msg(game_dir, &format!("⚠️ Retry {} for {} (Error: {})", retry_count, label, actual_err));
                let _ = app.emit("activation-progress", ProgressPayload {
                    percent: progress_start,
                    label: format!("{} · Disconnected, retrying ({})...", label, retry_count),
                });
                tokio::time::sleep(std::time::Duration::from_secs(10)).await;
                continue;
            },
            Err((e, _)) => {
                crate::logger::log_msg(game_dir, &format!("❌ Fatal error downloading {}: {}", label, e));
                return Err(e); // real error or cancelled
            }
        }
    }

    Ok(())
}

/// Parallel download using multiple curl.exe connections — like IDM.
/// Splits the file into num_parts chunks, downloads each with a separate
/// curl process using HTTP Range headers, then concatenates them.
/// Falls back to single connection for small files (< 10 MB).


async fn download_file_stream(
    client: &reqwest::Client,
    url: &str,
    dest: &Path,
    app: &AppHandle,
    label: &str,
    progress_start: f64,
    progress_end: f64,
    cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pause: std::sync::Arc<std::sync::atomic::AtomicBool>,
    num_parts: usize,
    pre_fetched_size: Option<u64>,
    accumulated_time: u64,
) -> Result<(), (String, u64)> {
    crate::downloader::download_file_stream_reqwest(
        client, url, dest, app, label, progress_start, progress_end, cancel, pause, num_parts, pre_fetched_size, accumulated_time
    ).await
}

/// Verify a file is a real Windows PE executable (starts with "MZ").
/// Detects LFS pointer files, HTML error pages, and truncated downloads.
fn validate_exe(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let size = metadata.len();

    // Suspiciously small file — likely LFS pointer or error page
    if size < 500 {
        let content = std::fs::read_to_string(path).unwrap_or_default();
        if content.contains("git-lfs") || content.contains("oid sha256") {
            return Err(format!(
                "الملف عبارة عن Git LFS pointer مش ملف حقيقي (حجمه {} بايت). لازم الرابط يخدم الـ binary مباشرة. \n\
                 The file is a Git LFS pointer, not the actual binary (only {} bytes). The URL must serve the actual binary directly.",
                size, size
            ));
        }
        return Err(format!(
            "الملف صغير جداً (حجمه {} بايت) وغالباً تحميله فشل / File is suspiciously small ({} bytes) — download likely failed",
            size, size
        ));
    }

    // Check MZ magic bytes (all valid Windows PE files start with "MZ" = 0x4D 0x5A)
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut buf = [0u8; 2];
    file.read_exact(&mut buf).map_err(|e| e.to_string())?;

    if buf != [0x4D, 0x5A] {
        return Err(format!(
            "الملف مش ملف تنفيذي صحيح (MZ signature مفقود). حجمه {} بايت. \n\
             File is not a valid Windows executable (missing MZ signature). Size: {} bytes.",
            size, size
        ));
    }

    Ok(())
}

fn validate_zip(path: &Path) -> Result<(), String> {
    let metadata = std::fs::metadata(path).map_err(|e| e.to_string())?;
    let size = metadata.len();

    if size < 100 {
        return Err(format!("ملف ZIP صغير جداً ({} بايت) / ZIP file too small ({} bytes)", size, size));
    }

    let file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    
    // Check if it opens as a valid zip archive without errors
    match zip::ZipArchive::new(file) {
        Ok(_) => Ok(()),
        Err(e) => Err(format!(
            "الملف معطوب أو غير مكتمل التحميل / File is corrupted or incomplete download: {}",
            e
        ))
    }
}

fn sha1_file(path: &Path) -> Result<String, String> {
    use sha1::{Sha1, Digest};
    let content = std::fs::read(path).map_err(|e| e.to_string())?;
    let mut hasher = Sha1::new();
    hasher.update(&content);
    let result = hasher.finalize();
    Ok(format!("{:x}", result))
}

fn extract_zip(zip_path: &Path, dest: &Path) -> Result<(), String> {
    let file = std::fs::File::open(zip_path).map_err(|e| e.to_string())?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| e.to_string())?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| e.to_string())?;
        let out_path = dest.join(entry.name());
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path).map_err(|e| e.to_string())?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
            }
            let mut outfile = std::fs::File::create(&out_path).map_err(|e| e.to_string())?;
            std::io::copy(&mut entry, &mut outfile).map_err(|e| e.to_string())?;
        }
    }
    std::fs::remove_file(zip_path).ok();
    Ok(())
}

fn run_hidden(exe_path: &Path, cwd: &Path) -> std::io::Result<std::process::Child> {
    Command::new(exe_path)
        .current_dir(cwd)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
}

fn check_ticket_files(dir: &Path) -> bool {
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if let Some(name_str) = name.to_str() {
                if name_str.starts_with("Denuvo_ticket") {
                    return true;
                }
            }
        }
    }
    false
}

/// Watch a directory for Denuvo_ticket files using a filesystem watcher.
/// Returns true if a ticket file is detected, false if timeout expires.
/// This is INSTANT — no polling delay. The moment the file is created,
/// we detect it.
async fn watch_for_ticket(
    game_dir: &Path,
    timeout_secs: u64,
    cancel: &std::sync::atomic::AtomicBool,
    _pause: &std::sync::atomic::AtomicBool,
    app: &AppHandle,
    progress_base: f64,
) -> bool {
    use notify::{Watcher, RecursiveMode, Event, EventKind};
    use std::sync::mpsc;
    
    let start_time = std::time::Instant::now();

    // First check if ticket already exists
    if check_ticket_files(game_dir) {
        return true;
    }

    // Set up filesystem watcher
    let (tx, rx) = mpsc::channel::<notify::Result<Event>>();

    let mut watcher = match notify::recommended_watcher(tx) {
        Ok(w) => w,
        Err(_) => {
            // Fallback to simple polling if watcher fails
            for _ in 0..(timeout_secs * 5) {
                tokio::time::sleep(std::time::Duration::from_millis(200)).await;
                if check_ticket_files(game_dir) { return true; }
                if cancel.load(std::sync::atomic::Ordering::Relaxed) { return false; }
            }
            return false;
        }
    };

    if watcher.watch(game_dir, RecursiveMode::NonRecursive).is_err() {
        // Fallback
        for _ in 0..(timeout_secs * 5) {
            tokio::time::sleep(std::time::Duration::from_millis(200)).await;
            if check_ticket_files(game_dir) { return true; }
            if cancel.load(std::sync::atomic::Ordering::Relaxed) { return false; }
        }
        return false;
    }

    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(timeout_secs);

    loop {
        if std::time::Instant::now() >= deadline {
            return false; // timeout — no ticket
        }

        if cancel.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }

        // Check for filesystem events (non-blocking with short timeout)
        match rx.recv_timeout(std::time::Duration::from_millis(200)) {
            Ok(Ok(event)) => {
                // Check if the created/modified file is a Denuvo_ticket
                if matches!(event.kind, EventKind::Create(_) | EventKind::Modify(_)) {
                    for path in &event.paths {
                        if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                            if name.starts_with("Denuvo_ticket") {
                                return true; // Found it instantly!
                            }
                        }
                    }
                }
            }
            Ok(Err(_)) => {} // watcher error, continue
            Err(mpsc::RecvTimeoutError::Timeout) => {
                // Also do a quick manual check in case we missed the event
                if check_ticket_files(game_dir) {
                    return true;
                }
            }
            Err(mpsc::RecvTimeoutError::Disconnected) => {
                return check_ticket_files(game_dir);
            }
        }
        
        let elapsed = start_time.elapsed().as_secs();
        let _ = app.emit("activation-progress", ProgressPayload {
            percent: progress_base,
            label: format!("Game loaded — checking for Denuvo tickets... [{}/{}]", format_time(elapsed), format_time(timeout_secs)),
        });
    }
}

fn generate_anadius_cfg(game_dir: &Path, version: &str, selection: &str) -> Result<(), String> {
    let dbdata_path = game_dir.join("dbdata.dll");
    let dll_hash = sha1_file(&dbdata_path)?;
    let dll_name = if selection == "FMM" { "EAAC.dll" } else { "FCLiveEditor.DLL" };

    let cfg_content = format!(
        r#""Config2"
{{
    "Game"
    {{
        "Name"                  "EA SPORTS FC 26 Showcase"
        "Version"               "{version}"
        "ContentId"             "16425677_sc"
        "DenuvoToken"           "PASTE_A_VALID_DENUVO_TOKEN_HERE"
        "DenuvoExeHash"         "26311c7c7af3e6acccfe35c8109051c0dc517d21"
        "DenuvoDllHash"         "{dll_hash}"
        "KeyForLicense"         "WHVu9VkrkjKPTcDIjCceww=="
        "Languages"             "ar_SA,cs_CZ,da_DK,de_DE,en_US,es_ES,es_MX,fr_FR,it_IT,ja_JP,ko_KR,nl_NL,no_NO,pl_PL,pt_BR,pt_PT,ru_RU,sv_SE,tr_TR,zh_CN,zh_HK"
        "Language"              "all"
        "LanguageRegistryKey"   "SOFTWARE\\EA Sports\\EA SPORTS FC 26\\Locale"
    }}
    "Emulator"
    {{
        "LoadExtraDLLs"         "{dll_name}"
        "LoadExtraDLLsFromMain" "FAKE/CryptBase0.dll"
        "PretendConnected"      "false"
        "FakeAuth"              "false"
    }}
    "User"
    {{
        "Username"              "3LAA_RA'FAT"
    }}
    "Achievements"
    {{
        "AchievementsSet"       "50072_16425677_50844"
        "AchievementNames"
        {{
            "1"                 "Masterplan"
            "2"                 "Treble Glory"
            "3"                 "Challenge Accepted"
            "4"                 "Expect the Unexpected"
            "5"                 "Legend on the Pitch"
            "6"                 "European Glory"
            "7"                 "Tactical Sync"
            "8"                 "Campeones"
            "9"                 "We're Going Up"
            "10"                "Top of the Pyramid"
            "11"                "First of Many"
            "12"                "Collect Them All"
            "13"                "Very Particular Set of Skills"
            "14"                "Shop 'Til You Drop"
            "15"                "Gold Standard"
            "16"                "In a Rush"
            "17"                "Football Friend"
            "18"                "KO Kings"
            "19"                "Dead-ball Specialist"
            "20"                "Intuition and Execution"
            "21"                "Power Shot"
            "22"                "Bring It On"
            "23"                "Surgical Aim"
            "24"                "Bullseye"
            "25"                "PlayStyles+"
            "26"                "Tactical Mastermind"
            "27"                "Clean Sheet"
            "28"                "Authenticity"
            "29"                "Squad Builder Extraordinaire"
            "30"                "Bounty Buster"
            "31"                "Full Chemistry Charge"
            "32"                "Tactical Designer"
            "33"                "Defensive Dynamo"
            "34"                "Final Evolutionary Stage"
            "35"                "Champion's Debut"
            "36"                "Mythic Milestone"
            "37"                "Makeover Maestro"
            "38"                "Event Explorer"
            "39"                "European Legend"
            "40"                "Best of Five"
            "41"                "Football is Everything"
            "42"                "One Season, wonderful"
            "43"                "All Aboard the Premium Track"
        }}
    }}
}}"#,
        version = version,
        dll_hash = dll_hash,
        dll_name = dll_name
    );

    let cfg_path = game_dir.join("anadius.cfg");
    std::fs::write(&cfg_path, cfg_content).map_err(|e| e.to_string())
}

pub async fn run_activation(app: AppHandle, game_dir: PathBuf, selection: String, client: reqwest::Client, cancel: std::sync::Arc<std::sync::atomic::AtomicBool>, pause: std::sync::Arc<std::sync::atomic::AtomicBool>) {
    let _ = std::fs::remove_file(game_dir.join("TechnoAfandi.log"));
    crate::logger::log_msg(&game_dir, &format!("--- NEW ACTIVATION STARTED: {} ---", selection));
    let app_progress = app.clone();
    let emit_progress = move |percent: f64, label: &str| {
        // crate::logger::log_msg(&game_dir_clone, label); // Too verbose to pass game_dir here, we'll log major steps manually
        let _ = app_progress.emit("activation-progress", ProgressPayload {
            percent,
            label: label.to_string(),
        });
    };

    let app_done = app.clone();
    let emit_done = move |success: bool, msg: &str| {
        let _ = app_done.emit("activation-done", serde_json::json!({
            "success": success,
            "message": msg
        }));
    };

    // Clean old ticket files
    if let Ok(entries) = std::fs::read_dir(&game_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            if let Some(n) = name.to_str() {
                if n.starts_with("Denuvo_ticket") {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
    }

    crate::logger::log_msg(&game_dir, "Cleaned up old Denuvo tickets.");

    let optimal_connections = 8;

    emit_progress(0.0, "Starting downloads...");

    // Step 1: Activation64.dll (0-5%)
    let dest = game_dir.join("FAKE/Activation64.dll");
    crate::logger::log_msg(&game_dir, "Starting download: Activation64.dll");
    if let Err(e) = download_file_smart(&client, ACTIVATION64_URL, &dest, &app, "Activation64.dll", 0.0, 5.0, cancel.clone(), pause.clone(), &game_dir, optimal_connections).await {
        crate::logger::log_msg(&game_dir, &format!("ERROR: Failed Activation64.dll: {}", e));
        emit_done(false, &format!("Failed to download Activation64.dll: {}", e));
        return;
    }

    // Step 2: CryptBase0.dll (5-10%)
    let dest = game_dir.join("FAKE/CryptBase0.dll");
    crate::logger::log_msg(&game_dir, "Starting download: CryptBase0.dll");
    if let Err(e) = download_file_smart(&client, CRYPTBASE0_URL, &dest, &app, "CryptBase0.dll", 5.0, 10.0, cancel.clone(), pause.clone(), &game_dir, optimal_connections).await {
        crate::logger::log_msg(&game_dir, &format!("ERROR: Failed CryptBase0.dll: {}", e));
        emit_done(false, &format!("Failed to download CryptBase0.dll: {}", e));
        return;
    }

    // Step 3: FC26.exe (10-30%)
    let fc26_path = game_dir.join("FC26.exe");
    crate::logger::log_msg(&game_dir, "Starting download: FC26.exe");
    if let Err(e) = download_file_smart(&client, FC26_URL, &fc26_path, &app, "FC26.exe", 10.0, 30.0, cancel.clone(), pause.clone(), &game_dir, optimal_connections).await {
        crate::logger::log_msg(&game_dir, &format!("ERROR: Failed FC26.exe: {}", e));
        emit_done(false, &format!("Failed to download FC26.exe: {}", e));
        return;
    }
    // ⚠️ Validate BEFORE running to avoid the "Unsupported 16-Bit Application" Windows popup
    if let Err(e) = validate_exe(&fc26_path) {
        emit_done(false, &format!("FC26.exe invalid: {}", e));
        return;
    }

    // Step 4: anadius64.dll (30-35%)
    let dest = game_dir.join("anadius64.dll");
    crate::logger::log_msg(&game_dir, "Starting download: anadius64.dll");
    if let Err(e) = download_file_smart(&client, ANADIUS64_URL, &dest, &app, "anadius64.dll", 30.0, 35.0, cancel.clone(), pause.clone(), &game_dir, optimal_connections).await {
        crate::logger::log_msg(&game_dir, &format!("ERROR: Failed anadius64.dll: {}", e));
        emit_done(false, &format!("Failed to download anadius64.dll: {}", e));
        return;
    }

    // Step 5: Selected package ZIP (35-45%)
    // Download to TEMP folder so ZIP never appears in game directory
    let zip_url = if selection == "FMM" { FMM_ZIP_RAW } else { LE_ZIP_RAW };
    let zip_filename = if selection == "FMM" { "FMM.zip" } else { "Live_Editor.zip" };
    let temp_dir = std::env::temp_dir();
    let zip_path = temp_dir.join(zip_filename);
    crate::logger::log_msg(&game_dir, &format!("Starting download: {} archive", selection));
    if let Err(e) = download_file_smart(&client, zip_url, &zip_path, &app, &selection, 35.0, 43.0, cancel.clone(), pause.clone(), &game_dir, optimal_connections).await {
        crate::logger::log_msg(&game_dir, &format!("ERROR: Failed archive: {}", e));
        emit_done(false, &format!("Failed to download {}: {}", selection, e));
        return;
    }
    // Validate ZIP before extraction
    if let Err(e) = validate_zip(&zip_path) {
        let _ = std::fs::remove_file(&zip_path);
        emit_done(false, &format!("{} archive invalid: {}", selection, e));
        return;
    }
    emit_progress(43.0, &format!("Extracting {}", selection));
    if let Err(e) = extract_zip(&zip_path, &game_dir) {
        let _ = std::fs::remove_file(&zip_path);
        emit_done(false, &format!("Failed to extract {}: {}", selection, e));
        return;
    }
    let _ = std::fs::remove_file(&zip_path);
    emit_progress(45.0, &format!("{} ready", selection));

    // Step 6: anadius.cfg (45-55%)
    emit_progress(48.0, "Reading game version");
    let xml_content = find_installer_xml(&game_dir)
        .and_then(|p| std::fs::read_to_string(p).ok())
        .unwrap_or_default();
    let version = parse_game_version(&xml_content).unwrap_or_else(|| "Unknown".to_string());
    emit_progress(50.0, "Generating anadius.cfg");
    if let Err(e) = generate_anadius_cfg(&game_dir, &version, &selection) {
        emit_done(false, &format!("Failed to create anadius.cfg: {}", e));
        return;
    }
    emit_progress(55.0, "anadius.cfg ready");

    // Step 7: Run FC26.exe
    // 1. Spawn FC26.exe
    // 2. Confirm it's running (process stays alive = game loaded)
    // 3. Watch for Denuvo ticket for 3 seconds
    emit_progress(58.0, "Running FC26.exe...");
    let fc26_child = run_hidden(&fc26_path, &game_dir);
    match fc26_child {
        Ok(mut child) => {
            // Confirm the game process started and is still running
            emit_progress(59.0, "Waiting for game to load...");
            let mut game_started = false;
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                match child.try_wait() {
                    Ok(None) => {
                        // Still running = game loaded
                        game_started = true;
                        break;
                    }
                    Ok(Some(status)) => {
                        emit_done(false, &format!("FC26.exe exited immediately (code: {:?})", status.code()));
                        return;
                    }
                    Err(e) => {
                        emit_done(false, &format!("Failed to check FC26.exe: {}", e));
                        return;
                    }
                }
            }

            if !game_started {
                emit_done(false, "FC26.exe did not start");
                return;
            }

            // Game is running — watch for Denuvo ticket
            emit_progress(62.0, "Game loaded — checking for Denuvo tickets...");
            let ticket_found = watch_for_ticket(&game_dir, 30, &cancel, &pause, &app, 62.0).await;

            if !ticket_found {
                emit_progress(100.0, "Activation complete");
                emit_done(true, "EA SPORTS FC 26 has successfully activated enjoy the game and feel free to ask about anythinh");
                return;
            }

            emit_progress(65.0, "Denuvo ticket detected — closing game...");
            let _ = child.kill();
            let _ = child.wait();
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
        }
        Err(e) => {
            emit_done(false, &format!("Failed to run FC26.exe: {}", e));
            return;
        }
    }

    // Step 8: Run bundled activator.exe — WAIT for it to finish
    let activator_bytes = app.state::<Vec<u8>>();

    let temp_activator = match tempfile::Builder::new()
        .prefix("activator")
        .suffix(".exe")
        .tempfile()
    {
        Ok(f) => f,
        Err(e) => {
            emit_done(false, &format!("Failed to create temp file: {}", e));
            return;
        }
    };
    let temp_path = temp_activator.into_temp_path();

    if let Err(e) = std::fs::write(&temp_path, &*activator_bytes) {
        emit_done(false, &format!("Failed to write activator.exe: {}", e));
        return;
    }

    emit_progress(70.0, "Preparing environment for activator...");
    // Hide DLLs that might be injected by Live Editor/FMM and crash the activator
    let conflict_dlls = ["CryptBase.dll", "version.dll", "dinput8.dll", "wininet.dll", "FCLiveEditor.DLL", "EAAC.dll"];
    for dll in &conflict_dlls {
        let p = game_dir.join(dll);
        if p.exists() {
            let _ = std::fs::rename(&p, game_dir.join(format!("{}.bak", dll)));
        }
    }

    emit_progress(72.0, "Running activator.exe...");
    let mut activator_child = match run_hidden(&temp_path, &game_dir) {
        Ok(c) => c,
        Err(e) => {
            // Restore DLLs on error
            for dll in &conflict_dlls {
                let bak = game_dir.join(format!("{}.bak", dll));
                if bak.exists() {
                    let _ = std::fs::rename(&bak, game_dir.join(dll));
                }
            }
            let _ = std::fs::remove_file(&temp_path);
            emit_done(false, &format!("Failed to run activator.exe: {}", e));
            return;
        }
    };

    // Wait for activator to fully complete (this one SHOULD exit)
    emit_progress(80.0, "Activator running — please wait...");
    let wait_result = activator_child.wait();
    
    // Restore DLLs immediately after activator finishes
    for dll in &conflict_dlls {
        let bak = game_dir.join(format!("{}.bak", dll));
        if bak.exists() {
            let _ = std::fs::rename(&bak, game_dir.join(dll));
        }
    }

    match wait_result {
        Ok(status) => {
            if !status.success() {
                let _ = std::fs::remove_file(&temp_path);
                emit_done(false, &format!("Activator failed (exit code: {:?})", status.code()));
                return;
            }
        }
        Err(e) => {
            let _ = std::fs::remove_file(&temp_path);
            emit_done(false, &format!("Failed waiting for activator: {}", e));
            return;
        }
    }
    let _ = std::fs::remove_file(&temp_path);
    emit_progress(90.0, "Activator finished");

    // Clean old tickets before re-running FC26
    if let Ok(entries) = std::fs::read_dir(&game_dir) {
        for entry in entries.flatten() {
            if let Some(n) = entry.file_name().to_str() {
                if n.starts_with("Denuvo_ticket") {
                    std::fs::remove_file(entry.path()).ok();
                }
            }
        }
    }

    // Step 9: Re-run FC26.exe — verify activation worked
    emit_progress(92.0, "Re-running FC26.exe — verifying...");
    match run_hidden(&fc26_path, &game_dir) {
        Ok(mut child2) => {
            // Confirm game started
            let mut started = false;
            for _ in 0..10 {
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                if let Ok(None) = child2.try_wait() {
                    started = true;
                    break;
                }
            }

            if started {
                emit_progress(94.0, "Game re-loaded — verifying Denuvo ticket creation...");
                let ticket_found = watch_for_ticket(&game_dir, 3, &cancel, &pause, &app, 94.0).await;

                if !ticket_found {
                    emit_progress(100.0, "Activation complete");
                    let _ = std::fs::remove_file(game_dir.join("TechnoAfandi.log"));
                    emit_done(true, "EA SPORTS FC 26 has successfully activated enjoy the game and feel free to ask about anythinh");
                } else {
                    emit_done(false, "فشل التفعيل - Activation Failed\nTicket files still present after activation");
                }
            } else {
                emit_done(false, "FC26.exe did not start on re-run");
            }
        }
        Err(e) => {
            emit_done(false, &format!("Failed to re-run FC26.exe: {}", e));
        }
    }
}

#[tauri::command]
pub async fn portable_update(app: tauri::AppHandle, version: String) -> Result<(), String> {
    let url = format!("https://github.com/mabdelmoneam03/TecchnoAfandi-FC/releases/download/v{}/TechnoAfandi-FC.exe", version);
    
    let current_exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let parent = current_exe.parent().unwrap_or(std::path::Path::new(""));
    let new_exe = parent.join("TechnoAfandi-FC_new.exe");

    let mut resp = reqwest::get(&url).await.map_err(|e| e.to_string())?;
    if !resp.status().is_success() {
        return Err(format!("Download failed: HTTP {}", resp.status()));
    }
    
    let total_size = resp.content_length().unwrap_or(0);
    let mut file = tokio::fs::File::create(&new_exe).await.map_err(|e| e.to_string())?;
    
    let mut downloaded = 0u64;
    
    app.emit("update-download-progress", serde_json::json!({
        "percent": 0.0,
        "label": "Starting download..."
    })).unwrap_or(());
    
    while let Some(chunk) = resp.chunk().await.map_err(|e| e.to_string())? {
        use tokio::io::AsyncWriteExt;
        file.write_all(&chunk).await.map_err(|e| e.to_string())?;
        downloaded += chunk.len() as u64;
        
        let mut pct = 0.0;
        if total_size > 0 {
            pct = (downloaded as f64 / total_size as f64) * 100.0;
        }
        
        app.emit("update-download-progress", serde_json::json!({
            "percent": pct,
            "label": format!("Downloading... {:.0}%", pct)
        })).unwrap_or(());
    }
    
    app.emit("update-download-progress", serde_json::json!({
        "percent": 100.0,
        "label": "Installing update... Restarting app"
    })).unwrap_or(());
    
    // Create batch script to replace the exe
    let bat_path = parent.join("update_ta.bat");
    let current_exe_name = current_exe.file_name().unwrap().to_string_lossy();
    let new_exe_name = new_exe.file_name().unwrap().to_string_lossy();
    
    // CRITICAL: timeout command fails in CREATE_NO_WINDOW. Use ping for delay!
    let bat_content = format!(
        "@echo off\r\n\
         ping 127.0.0.1 -n 4 > NUL\r\n\
         del \"{}\"\r\n\
         ren \"{}\" \"{}\"\r\n\
         start \"\" \"{}\"\r\n\
         del \"%~f0\"\r\n",
        current_exe_name, new_exe_name, current_exe_name, current_exe_name
    );
    
    std::fs::write(&bat_path, bat_content).map_err(|e| e.to_string())?;
    
    // Run batch script hidden
    std::process::Command::new("cmd")
        .arg("/C")
        .arg(&bat_path)
        .current_dir(parent)
        .creation_flags(0x08000000) // CREATE_NO_WINDOW
        .spawn()
        .map_err(|e| e.to_string())?;
        
    // Exit current process cleanly
    std::process::exit(0);
}
