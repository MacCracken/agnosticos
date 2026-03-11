//! /proc filesystem helpers for reading process resource usage.

/// Read VmRSS (resident set size) from `/proc/{pid}/status` in bytes.
pub(super) fn read_proc_memory(pid: u32) -> u64 {
    let path = format!("/proc/{}/status", pid);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| {
            for line in contents.lines() {
                if let Some(val) = line.strip_prefix("VmRSS:") {
                    // Value is in kB, e.g. "   12345 kB"
                    let kb: u64 = val.split_whitespace().next()?.parse().ok()?;
                    return Some(kb * 1024);
                }
            }
            None
        })
        .unwrap_or(0)
}

/// Read CPU time (utime + stime) from `/proc/{pid}/stat` and convert to microseconds.
pub(super) fn read_proc_cpu_time_us(pid: u32) -> u64 {
    let path = format!("/proc/{}/stat", pid);
    std::fs::read_to_string(&path)
        .ok()
        .and_then(|contents| {
            // Fields in /proc/pid/stat are space-separated.
            // Field 14 = utime (user ticks), field 15 = stime (kernel ticks).
            // The comm field (2) can contain spaces/parens, so find the closing ')' first.
            let after_comm = contents.find(')')?.checked_add(2)?;
            let fields: Vec<&str> = contents[after_comm..].split_whitespace().collect();
            // After comm, fields are 0-indexed from field 3 of the original format
            // utime = field 14 → index 11, stime = field 15 → index 12
            let utime: u64 = fields.get(11)?.parse().ok()?;
            let stime: u64 = fields.get(12)?.parse().ok()?;
            let ticks = utime + stime;
            // Convert clock ticks to microseconds (typically 100 ticks/sec on Linux).
            // Cache the result since the clock tick rate never changes at runtime.
            static CLK_TCK: std::sync::OnceLock<u64> = std::sync::OnceLock::new();
            let ticks_per_sec =
                *CLK_TCK.get_or_init(|| (unsafe { libc::sysconf(libc::_SC_CLK_TCK) }) as u64);
            if ticks_per_sec > 0 {
                Some(ticks * 1_000_000 / ticks_per_sec)
            } else {
                Some(ticks * 10_000) // fallback: assume 100 Hz
            }
        })
        .unwrap_or(0)
}
