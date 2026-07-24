use std::fs::OpenOptions;
use std::io::Write;
use std::path::Path;

/// Convert a UNIX timestamp (seconds since epoch) to a human-readable UTC string.
/// Pure arithmetic — no external crate needed.
fn unix_to_readable(secs: u64) -> String {
    let s = secs % 60;
    let m = (secs / 60) % 60;
    let h = (secs / 3600) % 24;
    let mut days = secs / 86400;
    // Walk through years since 1970
    let mut year = 1970u64;
    loop {
        let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
        let days_in_year: u64 = if leap { 366 } else { 365 };
        if days < days_in_year { break; }
        days -= days_in_year;
        year += 1;
    }
    let leap = (year % 4 == 0 && year % 100 != 0) || (year % 400 == 0);
    let month_days: [u64; 12] = [31, if leap { 29 } else { 28 }, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31];
    let mut month = 1u64;
    for &md in &month_days {
        if days < md { break; }
        days -= md;
        month += 1;
    }
    format!("{}-{:02}-{:02} {:02}:{:02}:{:02} UTC", year, month, days + 1, h, m, s)
}

pub fn log_msg(exe_dir: &Path, msg: &str) {
    let log_file = exe_dir.join("TechnoAfandi.log");
    use std::time::{SystemTime, UNIX_EPOCH};
    let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_secs();
    let timestamp = unix_to_readable(now);
    let log_line = format!("[{}] {}\n", timestamp, msg);
    if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(&log_file) {
        let _ = file.write_all(log_line.as_bytes());
    }
}
