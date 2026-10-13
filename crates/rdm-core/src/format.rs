//! Human-facing formatting helpers shared by the UI and logs.

const UNITS: [&str; 6] = ["B", "KB", "MB", "GB", "TB", "PB"];

/// `1536` -> `"1.5 KB"`.
pub fn human_bytes(bytes: u64) -> String {
    if bytes == 0 {
        return "0 B".to_string();
    }
    let mut size = bytes as f64;
    let mut unit = 0;
    while size >= 1024.0 && unit < UNITS.len() - 1 {
        size /= 1024.0;
        unit += 1;
    }
    if unit == 0 {
        format!("{bytes} B")
    } else {
        format!("{size:.1} {}", UNITS[unit])
    }
}

/// `3725` -> `"1h 2m"`, `95` -> `"1m 35s"`, `12` -> `"12s"`.
pub fn human_duration(secs: u64) -> String {
    let (h, m, s) = (secs / 3600, (secs % 3600) / 60, secs % 60);
    if h > 0 {
        format!("{h}h {m}m")
    } else if m > 0 {
        format!("{m}m {s}s")
    } else {
        format!("{s}s")
    }
}

/// `1_048_576.0` -> `"1.0 MB/s"`.
pub fn human_speed(bytes_per_sec: f64) -> String {
    if bytes_per_sec < 0.5 {
        "0 B/s".to_string()
    } else {
        format!("{}/s", human_bytes(bytes_per_sec.round() as u64))
    }
}

/// Best-effort ETA string for a transfer, or `"--"` when it cannot be known.
pub fn human_eta(remaining: u64, speed: f64) -> String {
    if speed < 1.0 {
        return "--".to_string();
    }
    human_duration((remaining as f64 / speed).ceil() as u64)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bytes_scale() {
        assert_eq!(human_bytes(0), "0 B");
        assert_eq!(human_bytes(999), "999 B");
        assert_eq!(human_bytes(1024), "1.0 KB");
        assert_eq!(human_bytes(1536), "1.5 KB");
        assert_eq!(human_bytes(1024 * 1024), "1.0 MB");
    }

    #[test]
    fn duration_shapes() {
        assert_eq!(human_duration(12), "12s");
        assert_eq!(human_duration(95), "1m 35s");
        assert_eq!(human_duration(3725), "1h 2m");
    }

    #[test]
    fn speed_and_eta() {
        assert_eq!(human_speed(0.0), "0 B/s");
        assert_eq!(human_speed(1024.0), "1.0 KB/s");
        assert_eq!(human_eta(1024, 0.0), "--");
        assert_eq!(human_eta(2048, 1024.0), "2s");
    }
}
