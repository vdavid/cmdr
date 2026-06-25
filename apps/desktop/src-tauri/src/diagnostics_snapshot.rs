//! A privacy-clean machine snapshot attached to diagnostic bundles (error reports and crash
//! reports), so triage knows the hardware, capacity, and Cmdr's own footprint without guessing.
//!
//! Two collection modes:
//! - [`SystemSnapshot::collect_full`] — used by error reports, gathered in a healthy running
//!   context, so it includes the live [`LiveSystemState`] (thermal, memory breakdown, RSS, uptime).
//! - [`SystemSnapshot::collect_stable`] — used by crash reports, which are assembled at the *next*
//!   launch. Only the stable machine-identity and capacity fields are meaningful then; live values
//!   would describe the freshly-restarted process, not the crash, so `live` is `None`.
//!
//! Privacy: every field is either coarse machine identity (model, CPU counts, OS build), an
//! aggregate number (RAM, disk, index sizes), or a coarse locale. No hostname, no usernames, no
//! paths, no volume names. The index breakdown is an unlabeled list of byte sizes. This is a
//! deliberate, reviewed widening of what the diagnostic bundles carry; keep it that way.

use std::path::Path;

use crate::system_memory::SystemMemoryInfo;

/// Stable machine identity and capacity, plus an optional live snapshot. See the module docs for
/// why `live` is absent on crash reports.
// No `Eq`: `live.uptime_secs` is an `f64`.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SystemSnapshot {
    /// macOS build number, for example `25F80` (pins the exact OS more tightly than the product
    /// version). `None` off macOS or if `sw_vers` is unavailable.
    pub os_build: Option<String>,
    /// Hardware model identifier, for example `Mac15,9`. `None` off macOS or on read failure.
    pub mac_model: Option<String>,
    /// Physical CPU core count (0 if unavailable).
    pub cpu_physical: u32,
    /// Logical CPU core count incl. SMT (0 if unavailable).
    pub cpu_logical: u32,
    /// Most-preferred UI language as a BCP-47 code, for example `en-US`. Coarse locale, no PII.
    pub preferred_language: Option<String>,
    /// Total physical RAM in bytes (stable across the crash → relaunch boundary).
    pub total_memory_bytes: u64,
    /// Free / total bytes of the volume holding the app data dir (where the index lives). `None`
    /// when `statfs` fails. A small `free` is the signal behind "indexing filled my disk" reports.
    pub data_volume_free_bytes: Option<u64>,
    pub data_volume_total_bytes: Option<u64>,
    /// Total on-disk size of all drive-index databases (each `index-*.db` plus its `-wal`/`-shm`).
    pub index_total_bytes: u64,
    /// Per-index-database sizes in bytes, sorted largest first. Unlabeled by design: it shows index
    /// skew without naming the user's drives.
    pub index_db_sizes: Vec<u64>,
    /// Per-moment state, present only for error reports (see module docs).
    pub live: Option<LiveSystemState>,
}

/// Live, per-moment machine state. Only meaningful when collected in a healthy running context, so
/// it rides error reports but not crash reports.
#[derive(Debug, Clone, PartialEq, serde::Serialize, serde::Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LiveSystemState {
    /// macOS thermal pressure: `nominal` / `fair` / `serious` / `critical` (`unknown` for an
    /// unrecognized raw value). `None` off macOS.
    pub thermal_state: Option<String>,
    /// Whole-machine memory breakdown (same data as the AI RAM gauge). Its `totalBytes` equals
    /// [`SystemSnapshot::total_memory_bytes`].
    pub memory: SystemMemoryInfo,
    /// Cmdr's own resident set size in bytes (0 if unavailable). The "is Cmdr the problem" signal a
    /// system-wide gauge can't give.
    pub process_rss_bytes: u64,
    /// Seconds since the process started. Distinguishes "died on launch" from "leaked over days".
    pub uptime_secs: f64,
}

impl SystemSnapshot {
    /// Full snapshot incl. live state. For error reports (healthy running context).
    pub(crate) fn collect_full(data_dir: &Path) -> Self {
        Self {
            live: Some(LiveSystemState {
                thermal_state: thermal_state(),
                memory: crate::system_memory::get_system_memory_info_inner(),
                process_rss_bytes: process_rss_bytes(),
                uptime_secs: crate::crash_reporter::uptime_secs(),
            }),
            ..Self::collect_stable(data_dir)
        }
    }

    /// Stable-fields-only snapshot (`live: None`). For crash reports, assembled at next launch.
    pub(crate) fn collect_stable(data_dir: &Path) -> Self {
        let sizes = index_db_sizes(data_dir);
        let (free, total) = volume_space(data_dir);
        Self {
            os_build: os_build(),
            mac_model: sysctl_string("hw.model"),
            cpu_physical: sysctl_u32("hw.physicalcpu").unwrap_or(0),
            cpu_logical: sysctl_u32("hw.logicalcpu").unwrap_or(0),
            preferred_language: crate::system_strings::preferred_language(),
            total_memory_bytes: crate::system_memory::get_system_memory_info_inner().total_bytes,
            data_volume_free_bytes: free,
            data_volume_total_bytes: total,
            index_total_bytes: sizes.iter().sum(),
            index_db_sizes: sizes,
            live: None,
        }
    }
}

/// macOS build number via `sw_vers -buildVersion` (for example `25F80`).
fn os_build() -> Option<String> {
    #[cfg(target_os = "macos")]
    {
        let output = std::process::Command::new("sw_vers")
            .arg("-buildVersion")
            .output()
            .ok()?;
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        (!s.is_empty()).then_some(s)
    }
    #[cfg(not(target_os = "macos"))]
    {
        None
    }
}

/// Cmdr's own resident set size in bytes, or 0 if it can't be read.
fn process_rss_bytes() -> u64 {
    use sysinfo::{ProcessRefreshKind, ProcessesToUpdate, System};

    let pid = match sysinfo::get_current_pid() {
        Ok(pid) => pid,
        Err(_) => return 0,
    };
    let mut sys = System::new();
    sys.refresh_processes_specifics(
        ProcessesToUpdate::Some(&[pid]),
        true,
        ProcessRefreshKind::nothing().with_memory(),
    );
    sys.process(pid).map(|p| p.memory()).unwrap_or(0)
}

/// Sum of each `index-*.db` plus its `-wal`/`-shm` siblings, one entry per database, sorted desc.
/// Reads only file *sizes* in the app data dir — never index contents, never paths leave the host.
fn index_db_sizes(data_dir: &Path) -> Vec<u64> {
    let mut sizes = Vec::new();
    let Ok(read_dir) = std::fs::read_dir(data_dir) else {
        return sizes;
    };
    for entry in read_dir.flatten() {
        let name = entry.file_name();
        let name = name.to_string_lossy();
        // Main db files only (`index-*.db`); the `-wal`/`-shm` siblings get folded into each db's total.
        if name.starts_with("index-") && name.ends_with(".db") {
            let db = entry.path();
            let total = file_len(&db) + file_len(&db.with_extension("db-wal")) + file_len(&db.with_extension("db-shm"));
            if total > 0 {
                sizes.push(total);
            }
        }
    }
    sizes.sort_unstable_by(|a, b| b.cmp(a));
    sizes
}

fn file_len(path: &Path) -> u64 {
    std::fs::metadata(path).map(|m| m.len()).unwrap_or(0)
}

/// Free (available-to-user) and total bytes of the volume holding `path`, via `statfs`.
/// `path` is the local app data dir, so this never hits a slow network mount.
fn volume_space(path: &Path) -> (Option<u64>, Option<u64>) {
    #[cfg(unix)]
    {
        use std::os::unix::ffi::OsStrExt;
        let Ok(c_path) = std::ffi::CString::new(path.as_os_str().as_bytes()) else {
            return (None, None);
        };
        // SAFETY: `libc::statfs` is a C struct of plain integers, so an all-zero bit pattern is a
        // valid (empty) value to initialize before the OS fills it in.
        let mut stat: libc::statfs = unsafe { std::mem::zeroed() };
        // SAFETY: `c_path` is a valid NUL-terminated path and `stat` is a live, correctly-sized
        // `statfs` the call writes into; we read its fields only after `rc == 0`.
        let rc = unsafe { libc::statfs(c_path.as_ptr(), &mut stat) };
        if rc != 0 {
            return (None, None);
        }
        let block_size = stat.f_bsize as u64;
        let total = (stat.f_blocks as u64).saturating_mul(block_size);
        let free = (stat.f_bavail as u64).saturating_mul(block_size);
        (Some(free), Some(total))
    }
    #[cfg(not(unix))]
    {
        let _ = path;
        (None, None)
    }
}

/// macOS thermal pressure label, or `None` off macOS.
#[cfg(target_os = "macos")]
fn thermal_state() -> Option<String> {
    use objc2_foundation::NSProcessInfo;

    let info = NSProcessInfo::processInfo();
    // `thermalState` is a thread-safe Foundation property (no AppKit / main-thread requirement).
    // The returned newtype wraps the raw `NSProcessInfoThermalState`: 0 nominal … 3 critical.
    let label = match info.thermalState().0 {
        0 => "nominal",
        1 => "fair",
        2 => "serious",
        3 => "critical",
        _ => "unknown",
    };
    Some(label.to_string())
}

#[cfg(not(target_os = "macos"))]
fn thermal_state() -> Option<String> {
    None
}

/// Reads a string `sysctl` (for example `hw.model`). macOS only.
#[cfg(target_os = "macos")]
fn sysctl_string(name: &str) -> Option<String> {
    let c_name = std::ffi::CString::new(name).ok()?;
    let mut size: libc::size_t = 0;
    // SAFETY: `c_name` is a valid NUL-terminated string. Passing a null value pointer asks
    // `sysctlbyname` to report only the required buffer length, which it writes into `size`.
    let rc = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            std::ptr::null_mut(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || size == 0 {
        return None;
    }
    let mut buf = vec![0u8; size];
    // SAFETY: `buf` holds exactly `size` bytes as just reported by the sizing call; `sysctlbyname`
    // writes at most `size` bytes and updates `size` to the actual length.
    let rc = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 {
        return None;
    }
    // `size` includes the trailing NUL.
    let s = String::from_utf8_lossy(&buf[..size.saturating_sub(1)])
        .trim()
        .to_string();
    (!s.is_empty()).then_some(s)
}

/// Reads an integer `sysctl` (for example `hw.physicalcpu`). macOS only.
#[cfg(target_os = "macos")]
fn sysctl_u32(name: &str) -> Option<u32> {
    let c_name = std::ffi::CString::new(name).ok()?;
    let mut value: i32 = 0;
    let mut size = size_of::<i32>() as libc::size_t;
    // SAFETY: `c_name` is a valid NUL-terminated string and `value` is a live `i32` whose size
    // matches `size`; `sysctlbyname` writes a single int into `value`.
    let rc = unsafe {
        libc::sysctlbyname(
            c_name.as_ptr(),
            std::ptr::from_mut(&mut value).cast(),
            &mut size,
            std::ptr::null_mut(),
            0,
        )
    };
    if rc != 0 || value < 0 {
        return None;
    }
    Some(value as u32)
}

#[cfg(not(target_os = "macos"))]
fn sysctl_string(_name: &str) -> Option<String> {
    None
}

#[cfg(not(target_os = "macos"))]
fn sysctl_u32(_name: &str) -> Option<u32> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn index_db_sizes_sums_siblings_and_sorts_desc() {
        let dir = tempfile::tempdir().expect("tempdir");
        let p = dir.path();
        // Volume A: 1000-byte db + 500-byte wal + 24-byte shm = 1524.
        std::fs::write(p.join("index-aaa.db"), vec![0u8; 1000]).expect("write");
        std::fs::write(p.join("index-aaa.db-wal"), vec![0u8; 500]).expect("write");
        std::fs::write(p.join("index-aaa.db-shm"), vec![0u8; 24]).expect("write");
        // Volume B: 100-byte db only.
        std::fs::write(p.join("index-bbb.db"), vec![0u8; 100]).expect("write");
        // Non-index files are ignored.
        std::fs::write(p.join("settings.json"), vec![0u8; 9999]).expect("write");
        std::fs::write(p.join("logs.txt"), vec![0u8; 9999]).expect("write");

        let sizes = index_db_sizes(p);
        assert_eq!(
            sizes,
            vec![1524, 100],
            "sorted desc, siblings folded in, non-index ignored"
        );
        assert_eq!(sizes.iter().sum::<u64>(), 1624);
    }

    #[test]
    fn index_db_sizes_empty_when_no_index_dbs() {
        let dir = tempfile::tempdir().expect("tempdir");
        assert!(index_db_sizes(dir.path()).is_empty());
        // A nonexistent dir is handled, not panicked on.
        assert!(index_db_sizes(&dir.path().join("nope")).is_empty());
    }

    #[test]
    fn stable_snapshot_has_no_live_and_serializes_camel_case() {
        let dir = tempfile::tempdir().expect("tempdir");
        let snap = SystemSnapshot::collect_stable(dir.path());
        assert!(snap.live.is_none(), "stable snapshot omits live state");
        assert!(snap.total_memory_bytes > 0, "every machine has RAM");

        let json = serde_json::to_value(&snap).expect("serialize");
        assert!(json.get("totalMemoryBytes").is_some(), "camelCase field naming");
        assert!(json.get("indexDbSizes").is_some());
        assert_eq!(json.get("live"), Some(&serde_json::Value::Null));
    }

    #[test]
    fn full_snapshot_includes_live_state() {
        let dir = tempfile::tempdir().expect("tempdir");
        let snap = SystemSnapshot::collect_full(dir.path());
        let live = snap.live.as_ref().expect("full snapshot carries live state");
        // Live total must agree with the stable total (same machine, same instant).
        assert_eq!(live.memory.total_bytes, snap.total_memory_bytes);
    }
}
