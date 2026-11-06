use std::fs;
use std::path::Path;

#[derive(Debug, Clone, PartialEq)]
pub enum MemorySource {
    ExplicitEnv,
    CgroupV2(u64),
    CgroupV1(u64),
    ProcMeminfo(u64),
    Sysconf(u64),
    DefaultFallback,
}

#[derive(Debug, Clone)]
pub struct MemorySetting {
    pub ceiling_mb: usize,
    pub source: MemorySource,
    pub ratio: f64,
}

impl MemorySetting {
    pub fn description(&self) -> String {
        match &self.source {
            MemorySource::ExplicitEnv => {
                format!("Explicitly configured via AEROKV_MAX_MEM_MB ({} MB)", self.ceiling_mb)
            }
            MemorySource::CgroupV2(total_bytes) => {
                let total_mb = total_bytes / (1024 * 1024);
                format!(
                    "Auto-detected container limit via cgroups v2 ({} MB allocated, using {:.0}%: {} MB)",
                    total_mb,
                    self.ratio * 100.0,
                    self.ceiling_mb
                )
            }
            MemorySource::CgroupV1(total_bytes) => {
                let total_mb = total_bytes / (1024 * 1024);
                format!(
                    "Auto-detected container limit via cgroups v1 ({} MB allocated, using {:.0}%: {} MB)",
                    total_mb,
                    self.ratio * 100.0,
                    self.ceiling_mb
                )
            }
            MemorySource::ProcMeminfo(total_bytes) => {
                let total_mb = total_bytes / (1024 * 1024);
                format!(
                    "Auto-detected system RAM via /proc/meminfo ({} MB total, using {:.0}%: {} MB)",
                    total_mb,
                    self.ratio * 100.0,
                    self.ceiling_mb
                )
            }
            MemorySource::Sysconf(total_bytes) => {
                let total_mb = total_bytes / (1024 * 1024);
                format!(
                    "Auto-detected host RAM via sysconf ({} MB total, using {:.0}%: {} MB)",
                    total_mb,
                    self.ratio * 100.0,
                    self.ceiling_mb
                )
            }
            MemorySource::DefaultFallback => {
                format!("Default fallback ceiling ({} MB)", self.ceiling_mb)
            }
        }
    }
}

pub fn parse_cgroup_v2_limit(content: &str) -> Option<u64> {
    let trimmed = content.trim();
    if trimmed.is_empty() || trimmed == "max" {
        return None;
    }
    trimmed.parse::<u64>().ok().filter(|&b| b > 0)
}

pub fn parse_cgroup_v1_limit(content: &str) -> Option<u64> {
    let trimmed = content.trim();
    if trimmed.is_empty() {
        return None;
    }
    // In cgroups v1, unconstrained limits are typically huge values (e.g. >= 2^60 or 0x7FFFFFFFFFFFF000)
    const CGROUP_V1_UNLIMITED_THRESHOLD: u64 = 1 << 60;
    trimmed
        .parse::<u64>()
        .ok()
        .filter(|&b| b > 0 && b < CGROUP_V1_UNLIMITED_THRESHOLD)
}

pub fn parse_meminfo(content: &str) -> Option<u64> {
    for line in content.lines() {
        if let Some(rest) = line.strip_prefix("MemTotal:") {
            let parts: Vec<&str> = rest.split_whitespace().collect();
            if let Some(first) = parts.first() {
                if let Ok(kb) = first.parse::<u64>() {
                    return Some(kb * 1024);
                }
            }
        }
    }
    None
}

fn detect_system_total_bytes() -> (Option<u64>, MemorySource) {
    // 1. Try Linux cgroup v2
    let cgroup_v2_path = Path::new("/sys/fs/cgroup/memory.max");
    if let Ok(content) = fs::read_to_string(cgroup_v2_path) {
        if let Some(bytes) = parse_cgroup_v2_limit(&content) {
            return (Some(bytes), MemorySource::CgroupV2(bytes));
        }
    }

    // 2. Try Linux cgroup v1
    let cgroup_v1_path = Path::new("/sys/fs/cgroup/memory/memory.limit_in_bytes");
    if let Ok(content) = fs::read_to_string(cgroup_v1_path) {
        if let Some(bytes) = parse_cgroup_v1_limit(&content) {
            return (Some(bytes), MemorySource::CgroupV1(bytes));
        }
    }

    // 3. Try /proc/meminfo (standard Linux)
    let meminfo_path = Path::new("/proc/meminfo");
    if let Ok(content) = fs::read_to_string(meminfo_path) {
        if let Some(bytes) = parse_meminfo(&content) {
            return (Some(bytes), MemorySource::ProcMeminfo(bytes));
        }
    }

    // 4. Fallback to POSIX sysconf (macOS, BSD, bare-metal Linux)
    #[cfg(unix)]
    unsafe {
        let pages = libc::sysconf(libc::_SC_PHYS_PAGES);
        let page_size = libc::sysconf(libc::_SC_PAGESIZE);
        if pages > 0 && page_size > 0 {
            let total_bytes = (pages as u64).saturating_mul(page_size as u64);
            if total_bytes > 0 {
                return (Some(total_bytes), MemorySource::Sysconf(total_bytes));
            }
        }
    }

    (None, MemorySource::DefaultFallback)
}

pub fn detect_memory_setting() -> MemorySetting {
    // Check explicit override
    if let Ok(val) = std::env::var("AEROKV_MAX_MEM_MB") {
        if let Ok(mb) = val.trim().parse::<usize>() {
            if mb > 0 {
                return MemorySetting {
                    ceiling_mb: mb,
                    source: MemorySource::ExplicitEnv,
                    ratio: 1.0,
                };
            }
        }
    }

    // Default container memory reservation ratio: 0.80 (80%), leaving 20% for OS, buffers, jemalloc
    let ratio: f64 = std::env::var("AEROKV_MEM_RATIO")
        .ok()
        .and_then(|r| r.trim().parse::<f64>().ok())
        .map(|r| r.clamp(0.1, 1.0))
        .unwrap_or(0.80);

    let (bytes_opt, source) = detect_system_total_bytes();
    match bytes_opt {
        Some(total_bytes) => {
            let calculated_mb = (((total_bytes as f64) * ratio) / (1024.0 * 1024.0)).round() as usize;
            MemorySetting {
                ceiling_mb: calculated_mb.max(16),
                source,
                ratio,
            }
        }
        None => MemorySetting {
            ceiling_mb: 1024,
            source: MemorySource::DefaultFallback,
            ratio,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cgroup_v2_parsing() {
        assert_eq!(parse_cgroup_v2_limit("max\n"), None);
        assert_eq!(parse_cgroup_v2_limit(""), None);
        assert_eq!(parse_cgroup_v2_limit("536870912\n"), Some(536870912)); // 512 MB
        assert_eq!(parse_cgroup_v2_limit("1073741824"), Some(1073741824)); // 1 GB
    }

    #[test]
    fn test_cgroup_v1_parsing() {
        assert_eq!(parse_cgroup_v1_limit(""), None);
        assert_eq!(parse_cgroup_v1_limit("9223372036854771712\n"), None); // Unlimited
        assert_eq!(parse_cgroup_v1_limit("18446744073709551615"), None); // Unlimited
        assert_eq!(parse_cgroup_v1_limit("536870912\n"), Some(536870912)); // 512 MB
    }

    #[test]
    fn test_meminfo_parsing() {
        let meminfo = r#"
MemTotal:       16314856 kB
MemFree:         8912440 kB
MemAvailable:   12398412 kB
"#;
        assert_eq!(parse_meminfo(meminfo), Some(16314856 * 1024));
    }
}

