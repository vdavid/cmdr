//! macOS: `sw_vers` for the build, `sysctl` for the model and cores, Foundation for the thermal
//! state, and `statfs` for the volume.

pub(crate) use super::unix::volume_space;

/// `sw_vers -buildVersion` (for example `25F80`).
pub(crate) fn os_build() -> Option<String> {
    let output = std::process::Command::new("sw_vers")
        .arg("-buildVersion")
        .output()
        .ok()?;
    let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
    (!s.is_empty()).then_some(s)
}

pub(crate) fn hardware_model() -> Option<String> {
    sysctl_string("hw.model")
}

pub(crate) fn cpu_physical() -> Option<u32> {
    sysctl_u32("hw.physicalcpu")
}

pub(crate) fn cpu_logical() -> Option<u32> {
    sysctl_u32("hw.logicalcpu")
}

pub(crate) fn thermal_state() -> Option<String> {
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

/// Reads a string `sysctl` (for example `hw.model`).
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

/// Reads an integer `sysctl` (for example `hw.physicalcpu`).
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
