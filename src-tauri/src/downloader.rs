use reqwest::Client;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Arc;
use tokio::io::AsyncWriteExt;
use tauri::AppHandle;
use tauri::Emitter;
use serde::Serialize;

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

async fn download_chunk(
    client: Client,
    url: String,
    start_byte: u64,
    end_byte: u64,
    dest_path: PathBuf,
    cancel: Arc<AtomicBool>,
    pause: Arc<AtomicBool>,
    dl_counter: Arc<AtomicU64>,
) -> Result<(), String> {
    let mut retry_count = 0;
    
    loop {
        let current_size = tokio::fs::metadata(&dest_path).await.map(|m| m.len()).unwrap_or(0);
        let current_start = start_byte + current_size;
        
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

        let file_res = tokio::fs::OpenOptions::new().create(true).append(true).open(&dest_path).await;
        if file_res.is_err() {
            tokio::time::sleep(tokio::time::Duration::from_secs(2)).await;
            continue;
        }
        let mut file = file_res.unwrap();

        let mut success = true;
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
                },
                Err(_) => {
                    success = false;
                    break;
                }
            }
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
                let _ = tokio::fs::remove_file(dest).await;
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
    let part_temp = std::env::temp_dir();

    let total_downloaded = Arc::new(AtomicU64::new(0));
    let mut initial_total_downloaded: u64 = 0;
    
    let error_msg = Arc::new(tokio::sync::Mutex::new(None));
    let mut tasks = Vec::new();

    for i in 0..num_parts {
        let file_name = dest.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
        let part_path = part_temp.join(format!(".dl_part_{}_{}", file_name, i));
        
        let current_size = std::fs::metadata(&part_path).map(|m| m.len()).unwrap_or(0);
        initial_total_downloaded += current_size;
        
        let start_byte = i as u64 * chunk_size + current_size;
        let end_byte = if i == num_parts - 1 { total - 1 } else { (i as u64 + 1) * chunk_size - 1 };

        if start_byte > end_byte {
            total_downloaded.fetch_add(current_size, Ordering::Relaxed);
            continue;
        }

        total_downloaded.fetch_add(current_size, Ordering::Relaxed);
        let target_file = part_path.clone();

        let url_clone = url.to_string();
        let client_clone = client.clone();
        let cancel_clone = cancel.clone();
        let pause_clone = pause.clone();
        let dl_counter = total_downloaded.clone();
        let err_clone = error_msg.clone();
        
        let task = tokio::spawn(async move {
            if let Err(e) = download_chunk(client_clone, url_clone, start_byte, end_byte, target_file, cancel_clone, pause_clone, dl_counter).await {
                if e != "Stopped" {
                    let mut lock = err_clone.lock().await;
                    if lock.is_none() { *lock = Some(e); }
                }
            }
        });

        tasks.push((task, part_path));
    }

    let _ = app.emit("activation-progress", ProgressPayload {
        percent: progress_start,
        label: format!("{} · Resuming/Starting · {} connections", label, tasks.len()),
    });

    let start_time = Instant::now();

    loop {
        tokio::time::sleep(Duration::from_millis(300)).await;

        if pause.load(Ordering::Relaxed) {
            for (task, _) in &tasks { task.abort(); }
            while pause.load(Ordering::Relaxed) {
                if cancel.load(Ordering::Relaxed) {
                    for (_, part_path) in &tasks { let _ = tokio::fs::remove_file(part_path).await; }
                    let current_elapsed = start_time.elapsed().as_secs();
                    return Err(("Cancelled".to_string(), accumulated_time + current_elapsed));
                }
                tokio::time::sleep(Duration::from_millis(200)).await;
            }
            let current_elapsed = start_time.elapsed().as_secs();
            return Err(("Resumed".to_string(), accumulated_time + current_elapsed));
        }

        if cancel.load(Ordering::Relaxed) {
            for (task, part_path) in &tasks {
                task.abort();
                let _ = tokio::fs::remove_file(part_path).await;
            }
            let current_elapsed = start_time.elapsed().as_secs();
            return Err(("Cancelled".to_string(), accumulated_time + current_elapsed));
        }

        let err_lock = error_msg.lock().await.clone();
        if let Some(err) = err_lock {
            for (task, _) in &tasks {
                task.abort();
                // We purposefully do NOT delete part_path so it can resume on Retry
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
        for (t, _) in &tasks {
            if !t.is_finished() {
                all_done = false;
                break;
            }
        }
        
        if all_done { break; }
    }

    let current_elapsed = start_time.elapsed().as_secs();
    
    let _ = app.emit("activation-progress", ProgressPayload {
        percent: progress_end - 0.1,
        label: format!("{} · Merging...", label),
    });

    let mut output = std::fs::File::create(dest).map_err(|e| (e.to_string(), accumulated_time + current_elapsed))?;
    for i in 0..num_parts {
        let file_name = dest.file_name().and_then(|s| s.to_str()).unwrap_or("unknown");
        let part_path = part_temp.join(format!(".dl_part_{}_{}", file_name, i));
        if part_path.exists() {
            let mut part = std::fs::File::open(&part_path).map_err(|e| (e.to_string(), accumulated_time + current_elapsed))?;
            std::io::copy(&mut part, &mut output).map_err(|e| (e.to_string(), accumulated_time + current_elapsed))?;
            let _ = std::fs::remove_file(part_path);
        }
    }

    let _ = app.emit("activation-progress", ProgressPayload {
        percent: progress_end,
        label: format!("{} · Done ✓", label),
    });

    Ok(())
}
