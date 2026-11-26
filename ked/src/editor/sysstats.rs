//! System stats for the dashboard, collected on a background thread.

#[cfg(target_os = "macos")]
pub(super) fn collect_sys_stats() -> (f64, u64, u64, Option<u8>, Option<String>) {
    use std::process::Command;
    let mut cpu: f64 = 0.0;
    let mut mem_used: u64 = 0;
    let mut mem_total: u64 = 0;
    let mut batt_pct: Option<u8> = None;
    let mut batt_status: Option<String> = None;

    // CPU
    if let Ok(out) = Command::new("top")
        .args(["-l", "1", "-n", "0", "-s", "0"])
        .output()
    {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            if let Some(idx) = line.find("CPU usage") {
                if let Some(pct) = line[idx..].split_whitespace()
                    .find(|w| w.ends_with('%'))
                    .and_then(|w| w.trim_end_matches('%').parse::<f64>().ok())
                {
                    cpu = pct;
                }
                break;
            }
        }
    }
    // Memory
    if let Ok(out) = Command::new("vm_stat").output() {
        let s = String::from_utf8_lossy(&out.stdout);
        let mut page_size: u64 = 16384;
        let mut used: u64 = 0;
        for line in s.lines() {
            if line.contains("page size of") {
                page_size = line.split_whitespace().last()
                    .and_then(|n| n.parse().ok()).unwrap_or(16384);
            }
            if line.contains("Pages active") || line.contains("Pages wired") {
                if let Some(n) = line.split(':').last()
                    .and_then(|n| n.trim().trim_end_matches('.').parse::<u64>().ok())
                {
                    used += n;
                }
            }
        }
        mem_used = used * page_size;
        if let Ok(out) = Command::new("sysctl").args(["-n", "hw.memsize"]).output() {
            mem_total = String::from_utf8_lossy(&out.stdout)
                .trim().parse().unwrap_or(0);
        }
    }
    // Battery
    if let Ok(out) = Command::new("pmset").args(["-g", "batt"]).output() {
        let s = String::from_utf8_lossy(&out.stdout);
        for line in s.lines() {
            if let Some(pct) = line.split('%').next()
                .and_then(|p| p.split_whitespace().last())
                .and_then(|n| n.parse::<u8>().ok())
            {
                batt_pct = Some(pct);
                let status = if line.contains("discharging") { "discharging" }
                    else if line.contains("charging") { "charging" }
                    else if line.contains("AC") && pct == 100 { "full" }
                    else if line.contains("AC") { "on AC" }
                    else { "?" };
                batt_status = Some(status.to_string());
                break;
            }
        }
    }
    (cpu, mem_used, mem_total, batt_pct, batt_status)
}

#[cfg(target_os = "linux")]
pub(super) fn collect_sys_stats() -> (f64, u64, u64, Option<u8>, Option<String>) {
    use std::fs;
    let mut mem_used: u64 = 0;
    let mut mem_total: u64 = 0;
    let mut batt_pct: Option<u8> = None;
    let mut batt_status: Option<String> = None;

    // CPU from /proc/stat.  A single sample is meaningless (the
    // counters are cumulative since boot) — take two samples ~120 ms
    // apart and compute the percentage from the deltas.
    let read_cpu = || -> Option<(u64, u64)> {
        let s = fs::read_to_string("/proc/stat").ok()?;
        let line = s.lines().next()?;
        let parts: Vec<u64> = line.split_whitespace().skip(1)
            .filter_map(|v| v.parse().ok()).collect();
        if parts.len() < 4 {
            return None;
        }
        let total: u64 = parts.iter().sum();
        Some((total, parts[3]))
    };
    let cpu: f64 = match (read_cpu(), {
        std::thread::sleep(std::time::Duration::from_millis(120));
        read_cpu()
    }) {
        (Some((t0, i0)), Some((t1, i1))) if t1 > t0 => {
            100.0 * (1.0 - (i1.saturating_sub(i0)) as f64 / (t1 - t0) as f64)
        }
        _ => 0.0,
    };

    // Memory from /proc/meminfo
    if let Ok(s) = fs::read_to_string("/proc/meminfo") {
        for line in s.lines() {
            if line.starts_with("MemTotal:") {
                mem_total = line.split_whitespace().nth(1)
                    .and_then(|v| v.parse().ok()).unwrap_or(0) * 1024;
            }
            if line.starts_with("MemAvailable:") {
                let avail = line.split_whitespace().nth(1)
                    .and_then(|v| v.parse::<u64>().ok()).unwrap_or(0) * 1024;
                mem_used = mem_total.saturating_sub(avail);
            }
        }
    }

    // Battery from /sys/class/power_supply
    let batt_dirs = ["BAT0", "BAT1", "BATT"];
    for name in &batt_dirs {
        let base = format!("/sys/class/power_supply/{name}");
        if let Ok(pct) = fs::read_to_string(format!("{base}/capacity")) {
            batt_pct = pct.trim().parse().ok();
        }
        if let Ok(status) = fs::read_to_string(format!("{base}/status")) {
            batt_status = Some(status.trim().to_lowercase());
        }
        if batt_pct.is_some() { break; }
    }

    (cpu, mem_used, mem_total, batt_pct, batt_status)
}

#[cfg(not(any(target_os = "macos", target_os = "linux")))]
pub(super) fn collect_sys_stats() -> (f64, u64, u64, Option<u8>, Option<String>) {
    (0.0, 0, 0, None, None)
}
