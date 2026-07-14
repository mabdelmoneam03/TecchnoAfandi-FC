use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

pub fn log_msg(exe_dir: &Path, msg: &str) {
    let log_file = exe_dir.join("TechnoAfandi.log");
    
    // Simple custom timestamp to avoid pulling the massive chrono crate if it's not needed, but wait, chrono is standard for this. Let's just use SystemTime.
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs();
    
    let log_line = format!("[UNIX {}] {}\n", now, msg);
    
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_file) {
        let _ = file.write_all(log_line.as_bytes());
    }
}
