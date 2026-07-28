use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tauri::AppHandle;
use tauri::Emitter;
use serde::{Serialize, Deserialize};

#[derive(Clone, Serialize)]
pub struct ProgressPayload {
    pub percent: f64,
    pub label: String,
}

pub fn format_time(secs: u64) -> String {
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

#[derive(Serialize, Deserialize, Clone, Default)]
struct DownloadState {
    parts_downloaded: Vec<u64>,
}

async fn save_state(state_path: &Path, state: &DownloadState) {
    if let Ok(json) = serde_json::to_string(state) {
        let _ = tokio::fs::write(state_path, json).await;
    }
}

async fn load_state(state_path: &Path, num_parts: usize) -> DownloadState {
    if let Ok(content) = tokio::fs::read_to_string(state_path).await {
        if let Ok(state) = serde_json::from_str::<DownloadState>(&content) {
            if state.parts_downloaded.len() == num_parts {
                return state;
            }
        }
    }
    DownloadState { parts_downloaded: vec![0; num_parts] }
}

async fn download_chunk(
    client: Client,
    url: String,
    part_index: usize,
    base_start_byte: u64,
    end_byte: u64,
    dest_path: PathBuf,
    state_path: PathBuf,
    state: Arc<tokio::sync::Mutex<DownloadState>>,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    dl_counter: Arc<AtomicU64>,
) -> Result<(), String> {
    let mut retry_count = 0;
    
    loop {
        let current_downloaded = {
            let s = state.lock().await;
            s.parts_downloaded[part_index]
        };
        let current_start = base_start_byte + current_downloaded;
        
        if current_start > end_byte {
            return Ok(()); // Done
        }

        let range = format!("bytes={}-{}", current_start, end_byte);
        let resp_res = client.get(&url).header("Range", range).send().await;
        
        let mut resp = match resp_res {
            Ok(r) if r.status().is_success() => r,
            Ok(r) => {
                retry_count += 1;
                if retry_count > 10 { return Err(format!("HTTP error: {}", r.status())); }
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            },
            Err(e) => {
                retry_count += 1;
                if retry_count > 10 { return Err(e.to_string()); }
                tokio::time::sleep(tokio::time::Duration::from_secs(3)).await;
                continue;
            }
        };

        let file_res = tokio::fs::OpenOptions::new().write(true).open(&dest_path).await;
        if file_res.is_err() {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            continue;
        }
        let mut file = file_res.unwrap();
        if let Err(e) = file.seek(std::io::SeekFrom::Start(current_start)).await {
            return Err(format!("Failed to seek: {}", e));
        }

        let mut success = true;
        let mut local_downloaded = current_downloaded;
        let mut last_save = std::time::Instant::now();

        while let Some(chunk_res) = resp.chunk().await.transpose() {
            if cancel.load(Ordering::Relaxed) || pause.load(Ordering::Relaxed) {
                return Err("Stopped".to_string());
            }
            match chunk_res {
                Ok(chunk) => {
                    if file.write_all(&chunk).await.is_err() {
                        success = false;
                        break;
                    }
                    dl_counter.fetch_add(chunk.len() as u64, Ordering::Relaxed);
                    local_downloaded += chunk.len() as u64;
                    
                    if last_save.elapsed().as_secs() >= 2 {
                        let mut s = state.lock().await;
                        s.parts_downloaded[part_index] = local_downloaded;
                        save_state(&state_path, &s).await;
                        last_save = std::time::Instant::now();
                    }
                },
                Err(_) => {
                    success = false;
                    break;
                }
            }
        }
        
        // Final save for this attempt
        {
            let mut s = state.lock().await;
            s.parts_downloaded[part_index] = local_downloaded;
            save_state(&state_path, &s).await;
        }

        if success {
            return Ok(());
        } else {
            retry_count += 1;
            if retry_count > 15 {
                return Err("Failed after 15 chunk retries".to_string());
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
        }
    }
}

pub async fn download_file_stream_reqwest(
    client: &Client,
    url: &str,
    dest: &Path,
    app: &AppHandle,
    label: &str,
    progress_start: f64,
    progress_end: f64,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    num_parts: usize,
    total_size: Option<u64>,
    accumulated_time: u64,
) -> Result<(), (String, u64)> {
    use std::time::{Duration, Instant};

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| (e.to_string(), accumulated_time))?;
    }

    let use_parallel = total_size.map(|s| s > 1 * 1024 * 1024).unwrap_or(false);

    if !use_parallel {
        // Just use single connection logic inline here
        let start_time = Instant::now();
        let mut resp = client.get(url).send().await.map_err(|e| (e.to_string(), accumulated_time))?;
        if !resp.status().is_success() {
            return Err((format!("HTTP error: {}", resp.status()), accumulated_time));
        }
        
        let file = tokio::fs::OpenOptions::new().create(true).write(true).truncate(true).open(dest).await.map_err(|e| (e.to_string(), accumulated_time))?;
        let mut writer = tokio::io::BufWriter::with_capacity(1024 * 1024, file);
        let mut total_downloaded: u64 = 0;
        
        while let Some(chunk) = resp.chunk().await.map_err(|e| (e.to_string(), accumulated_time + start_time.elapsed().as_secs()))? {
            if pause.load(Ordering::Relaxed) {
                while pause.load(Ordering::Relaxed) {
                    if cancel.load(Ordering::Relaxed) {
                        return Err(("Cancelled".to_string(), accumulated_time + start_time.elapsed().as_secs()));
                    }
                    tokio::time::sleep(Duration::from_millis(200)).await;
                }
                return Err(("Resumed".to_string(), accumulated_time + start_time.elapsed().as_secs()));
            }
            if cancel.load(Ordering::Relaxed) {
                let _ = tokio::fs::remove_file(dest).await;
                return Err(("Cancelled".to_string(), accumulated_time + start_time.elapsed().as_secs()));
            }
            
            writer.write_all(&chunk).await.map_err(|e| (e.to_string(), accumulated_time + start_time.elapsed().as_secs()))?;
            total_downloaded += chunk.len() as u64;
            
            let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
            let speed = (total_downloaded as f64 / 1_048_576.0) / elapsed;
            let mb = total_downloaded as f64 / 1_048_576.0;
            let total_elapsed = accumulated_time as f64 + elapsed;
            
            let (pct, text) = if let Some(total) = total_size {
                let frac = (total_downloaded as f64 / total as f64).min(1.0);
                let p = progress_start + (progress_end - progress_start) * frac;
                (p, format!("{} · {:.1}/{:.1} MB · {:.2} MB/s [{}]", label, mb, total as f64/1_048_576.0, speed, format_time(total_elapsed.round() as u64)))
            } else {
                (progress_start, format!("{} · {:.1} MB · {:.2} MB/s [{}]", label, mb, speed, format_time(total_elapsed.round() as u64)))
            };
            let _ = app.emit("activation-progress", ProgressPayload { percent: pct, label: text });
        }
        
        writer.flush().await.map_err(|e| (e.to_string(), accumulated_time + start_time.elapsed().as_secs()))?;

        let _ = app.emit("activation-progress", ProgressPayload {
            percent: progress_end,
            label: format!("{} · Done ✓", label),
        });
        
        return Ok(());
    }

    let total = total_size.unwrap();
    let chunk_size = total / num_parts as u64;
    let state_path = dest.with_extension("state");

    // Pre-allocate the file using set_len to avoid merging later
    let file = std::fs::OpenOptions::new()
        .write(true)
        .create(true)
        .open(dest)
        .map_err(|e| (format!("Failed to create file: {}", e), accumulated_time))?;
    
    file.set_len(total).map_err(|e| (format!("Failed to allocate file: {}", e), accumulated_time))?;

    let download_state = load_state(&state_path, num_parts).await;
    let state_arc = Arc::new(tokio::sync::Mutex::new(download_state.clone()));

    let mut initial_total_downloaded: u64 = download_state.parts_downloaded.iter().sum();
    let total_downloaded = Arc::new(AtomicU64::new(initial_total_downloaded));
    
    let error_msg = Arc::new(tokio::sync::Mutex::new(None));
    let mut tasks = Vec::new();

    for i in 0..num_parts {
        let base_start_byte = i as u64 * chunk_size;
        let end_byte = if i == num_parts - 1 { total - 1 } else { (i as u64 + 1) * chunk_size - 1 };

        let url_clone = url.to_string();
        let client_clone = client.clone();
        let cancel_clone = cancel.clone();
        let pause_clone = pause.clone();
        let dl_counter = total_downloaded.clone();
        let err_clone = error_msg.clone();
        let dest_clone = dest.to_path_buf();
        let state_path_clone = state_path.clone();
        let state_clone = state_arc.clone();
        
        let task = tokio::spawn(async move {
            if let Err(e) = download_chunk(client_clone, url_clone, i, base_start_byte, end_byte, dest_clone, state_path_clone, state_clone, cancel_clone, pause_clone, dl_counter).await {
                if e != "Stopped" {
                    let mut lock = err_clone.lock().await;
                    if lock.is_none() { *lock = Some(e); }
                }
            }
        });

        tasks.push(task);
    }

    let _ = app.emit("activation-progress", ProgressPayload {
        percent: progress_start,
        label: format!("{} · Resuming/Starting", label),
    });

    let start_time = Instant::now();

    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;

        if pause.load(Ordering::Relaxed) {
            for task in &tasks { task.abort(); }
            while pause.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    let _ = tokio::fs::remove_file(dest).await;
                    let _ = tokio::fs::remove_file(&state_path).await;
                    let current_elapsed = start_time.elapsed().as_secs();
                    return Err(("Cancelled".to_string(), accumulated_time + current_elapsed));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            let current_elapsed = start_time.elapsed().as_secs();
            return Err(("Resumed".to_string(), accumulated_time + current_elapsed));
        }

        if cancel.load(Ordering::Relaxed) {
            for task in &tasks {
                task.abort();
            }
            let _ = tokio::fs::remove_file(dest).await;
            let _ = tokio::fs::remove_file(&state_path).await;
            let current_elapsed = start_time.elapsed().as_secs();
            return Err(("Cancelled".to_string(), accumulated_time + current_elapsed));
        }

        let err_lock = error_msg.lock().await.clone();
        if let Some(err) = err_lock {
            for task in &tasks {
                task.abort();
            }
            let current_elapsed = start_time.elapsed().as_secs();
            return Err((format!("Retry|{}", err), accumulated_time + current_elapsed));
        }

        let current_dl = total_downloaded.load(Ordering::Relaxed);
        let frac = (current_dl as f64 / total as f64).min(1.0);
        let current_pct = progress_start + (progress_end - progress_start) * frac;
        let elapsed = start_time.elapsed().as_secs_f64().max(0.001);
        let total_elapsed = accumulated_time as f64 + elapsed;
        let session_downloaded = current_dl.saturating_sub(initial_total_downloaded);
        let speed = (session_downloaded as f64 / 1_048_576.0) / elapsed;
        let mb_dl = current_dl as f64 / 1_048_576.0;
        let mb_tot = total as f64 / 1_048_576.0;

        let _ = app.emit("activation-progress", ProgressPayload {
            percent: current_pct,
            label: format!("{} · {:.1}/{:.1} MB · {:.2} MB/s [{}]", label, mb_dl, mb_tot, speed, format_time(total_elapsed.round() as u64)),
        });

        let mut all_done = true;
        for t in &tasks {
            if !t.is_finished() {
                all_done = false;
                break;
            }
        }
        if all_done { 
            if pause.load(Ordering::Relaxed) { continue; }
            break; 
        }
    }

    let _ = tokio::fs::remove_file(&state_path).await;

    let _ = app.emit("activation-progress", ProgressPayload {
        percent: progress_end,
        label: format!("{} · Done ✓", label),
    });

    Ok(())
}
