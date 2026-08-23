//! Async-signal-safe capture of a native crash (SIGSEGV / SIGBUS / SIGABRT).
//!
//! Unix only, and a world apart from the panic hook next door: a signal handler may call
//! nothing that allocates, locks, or formats. So this side writes fixed-width binary
//! records to an fd opened at [`install`] time, and `crate::crash_reporter` turns them into
//! a `CrashReport` at the next launch (`symbolicate.rs`). The file format, the ASLR image
//! base, and why each call in the handler is safe are documented at each site and in
//! `DETAILS.md` § Image base.

use std::os::unix::io::RawFd;
use std::path::Path;
use std::sync::atomic::{AtomicI32, AtomicU64, Ordering};

/// Max stack frames to capture in the signal handler.
const MAX_FRAMES: usize = 256;

/// Pre-opened fd for writing raw crash data. Set at init, read in signal handler.
static RAW_FD: AtomicI32 = AtomicI32::new(-1);

// The raw crash file format (binary):
//   - 4 bytes: magic "CMCR"
//   - 4 bytes: version (u32 LE)
//   - 4 bytes: signal number (i32 LE)
//   - 4 bytes: frame count (u32 LE)
//   - 8 bytes: main-image load address (u64 LE; 0 when unknown)
//   - N * 8 bytes: instruction pointer addresses (u64 LE)
//   - 32 bytes: app version (zero-padded ASCII)
const MAGIC: &[u8; 4] = b"CMCR";
const VERSION: u32 = 2;
const APP_VERSION_FIELD_LEN: usize = 32;
/// Byte offset where frame addresses start: header (16) + image base (8).
const FRAMES_START: usize = 24;

/// Load address of the main executable, captured at init.
///
/// Resolved in [`install`] (normal context) rather than in the handler, so the
/// handler only has to do an atomic load, which IS async-signal-safe. The dyld
/// lookup itself isn't, so it must never move into the handler.
static IMAGE_BASE: AtomicU64 = AtomicU64::new(0);

unsafe extern "C" {
    /// macOS/glibc `backtrace()` from execinfo.h: async-signal-safe on macOS.
    fn backtrace(buffer: *mut *mut libc::c_void, size: libc::c_int) -> libc::c_int;
}

#[cfg(target_os = "macos")]
unsafe extern "C" {
    /// Mach header of the image at `image_index`. Index 0 is the main executable.
    fn _dyld_get_image_header(image_index: u32) -> *const libc::c_void;
}

/// The main executable's load address, or 0 if we can't determine it.
///
/// ASLR randomizes this per launch, so without it the captured instruction
/// pointers are meaningless off-machine. With it, `addr - base` is a stable
/// per-build offset: comparable across users (crash grouping) and resolvable
/// with `atos -o <binary> -l <base>`.
#[cfg(target_os = "macos")]
pub fn current_image_base() -> u64 {
    // SAFETY: `_dyld_get_image_header` takes an image index and returns a borrowed
    // pointer (or null) without transferring ownership. Index 0 is the main
    // executable and is always loaded. We only cast the pointer to an integer
    // address and never dereference it, so a null or stale value is harmless.
    unsafe { _dyld_get_image_header(0) as u64 }
}

/// Non-macOS Unix (the Linux E2E container) has no `_dyld_*`; report "unknown".
#[cfg(not(target_os = "macos"))]
pub fn current_image_base() -> u64 {
    0
}

pub fn install(raw_crash_path: &Path) {
    // Pre-open the fd for the raw crash file. O_WRONLY | O_CREAT | O_TRUNC
    // will be applied at write time by truncating to 0 first.
    let path_cstr = match std::ffi::CString::new(raw_crash_path.as_os_str().as_encoded_bytes()) {
        Ok(c) => c,
        Err(_) => {
            log::warn!("Crash reporter: invalid raw crash path, signal handlers not installed");
            return;
        }
    };

    // SAFETY: `path_cstr` is a valid, NUL-terminated C string (built above and held
    // alive across the call). `open` returns a fresh fd or a negative value on failure,
    // which we check below before storing it.
    let fd = unsafe {
        libc::open(
            path_cstr.as_ptr(),
            libc::O_WRONLY | libc::O_CREAT | libc::O_TRUNC,
            0o644,
        )
    };
    if fd < 0 {
        log::warn!("Crash reporter: couldn't open raw crash file, signal handlers not installed");
        return;
    }
    RAW_FD.store(fd, Ordering::SeqCst);

    // Resolve the ASLR base now, while we're still in normal context. The handler
    // can then just load the atomic (signal-safe); the dyld call itself is not.
    IMAGE_BASE.store(current_image_base(), Ordering::SeqCst);

    // Register signal handlers for SIGSEGV, SIGBUS, SIGABRT
    for sig in [libc::SIGSEGV, libc::SIGBUS, libc::SIGABRT] {
        // SAFETY: `action` is zeroed before use, so every field starts in a valid state;
        // `sigemptyset` then empties `sa_mask`. `sa_sigaction` points at `signal_handler`,
        // a valid `extern "C"` SA_SIGINFO handler with the matching signature. `&action` is
        // a live, fully-initialized `sigaction`, and the null `oldact` pointer is allowed.
        unsafe {
            let mut action: libc::sigaction = std::mem::zeroed();
            action.sa_sigaction = signal_handler as *const () as usize;
            action.sa_flags = libc::SA_SIGINFO | libc::SA_RESETHAND;
            libc::sigemptyset(&mut action.sa_mask);
            libc::sigaction(sig, &action, std::ptr::null_mut());
        }
    }
}

/// Async-signal-safe signal handler. Only uses write() and _exit().
extern "C" fn signal_handler(sig: libc::c_int, _info: *mut libc::siginfo_t, _ctx: *mut libc::c_void) {
    let fd = RAW_FD.load(Ordering::SeqCst);
    if fd < 0 {
        // SAFETY: `_exit` is async-signal-safe and never returns; no allocation or
        // locking on this path. Reached only when no fd was opened, so there's nothing
        // to write.
        unsafe { libc::_exit(128 + sig) };
    }

    // Seek to beginning and truncate.
    // SAFETY: `fd` was checked non-negative above and is the pre-opened crash-file fd.
    // `lseek`/`ftruncate` are async-signal-safe; no allocation or locking on this path.
    unsafe {
        libc::lseek(fd, 0, libc::SEEK_SET);
        libc::ftruncate(fd, 0);
    }

    // Capture raw instruction pointer addresses
    let mut frames: [*mut libc::c_void; MAX_FRAMES] = [std::ptr::null_mut(); MAX_FRAMES];
    // SAFETY: `frames` is a stack array of exactly `MAX_FRAMES` valid slots, and we pass
    // that same count, so `backtrace` writes within bounds. `backtrace` is async-signal-safe
    // on macOS (Linux glibc is safe in practice); no allocation or locking on this path.
    let frame_count = unsafe { backtrace(frames.as_mut_ptr(), MAX_FRAMES as libc::c_int) };
    let frame_count = if frame_count < 0 { 0 } else { frame_count as u32 };

    // Write header: magic + version + signal + frame_count + image base
    write_bytes(fd, MAGIC);
    write_bytes(fd, &VERSION.to_le_bytes());
    write_bytes(fd, &sig.to_le_bytes());
    write_bytes(fd, &frame_count.to_le_bytes());
    // Plain atomic load: async-signal-safe (the dyld lookup happened at install).
    write_bytes(fd, &IMAGE_BASE.load(Ordering::Relaxed).to_le_bytes());

    // Write frame addresses as u64 LE
    for frame in frames.iter().take(frame_count as usize) {
        let addr = *frame as u64;
        write_bytes(fd, &addr.to_le_bytes());
    }

    // Write app version (zero-padded to fixed length)
    let version_bytes = env!("CARGO_PKG_VERSION").as_bytes();
    let mut version_buf = [0u8; APP_VERSION_FIELD_LEN];
    let copy_len = version_bytes.len().min(APP_VERSION_FIELD_LEN);
    version_buf[..copy_len].copy_from_slice(&version_bytes[..copy_len]);
    write_bytes(fd, &version_buf);

    // Close and re-raise to get the default behavior (core dump, etc.)
    // SAFETY: `fd` is the valid pre-opened crash-file fd (checked non-negative above);
    // `close` and `raise` are async-signal-safe, with no allocation or locking on this path.
    unsafe {
        libc::close(fd);
        libc::raise(sig);
    }
}

/// Async-signal-safe write helper.
fn write_bytes(fd: RawFd, buf: &[u8]) {
    let mut written = 0;
    while written < buf.len() {
        // SAFETY: `buf[written..]` is an in-bounds subslice (`written < buf.len()`), so the
        // pointer and length (`buf.len() - written`) describe valid initialized bytes.
        // `write` is async-signal-safe; no allocation or locking on this path.
        let n = unsafe { libc::write(fd, buf[written..].as_ptr().cast(), buf.len() - written) };
        if n <= 0 {
            break;
        }
        written += n as usize;
    }
}

/// Reads the raw crash file and returns (signal, frame_addresses, image_base, app_version).
/// `image_base` is 0 when the crashing build couldn't determine it.
/// Returns None if the file doesn't exist or is corrupt.
pub fn read_raw_crash(path: &Path) -> Option<(i32, Vec<u64>, u64, String)> {
    let data = std::fs::read(path).ok()?;

    // Minimum size: header(16) + image_base(8) + version_field(32)
    if data.len() < FRAMES_START + APP_VERSION_FIELD_LEN {
        log::info!("Crash reporter: raw crash file too small, discarding");
        let _ = std::fs::remove_file(path);
        return None;
    }

    if &data[0..4] != MAGIC {
        log::info!("Crash reporter: raw crash file bad magic, discarding");
        let _ = std::fs::remove_file(path);
        return None;
    }

    let version = u32::from_le_bytes(data[4..8].try_into().ok()?);
    if version != VERSION {
        log::info!("Crash reporter: raw crash file version mismatch ({version}), discarding");
        let _ = std::fs::remove_file(path);
        return None;
    }

    let signal = i32::from_le_bytes(data[8..12].try_into().ok()?);
    let frame_count = u32::from_le_bytes(data[12..16].try_into().ok()?) as usize;
    let image_base = u64::from_le_bytes(data[16..FRAMES_START].try_into().ok()?);

    let frames_end = FRAMES_START + frame_count * 8;
    let expected_len = frames_end + APP_VERSION_FIELD_LEN;
    if data.len() < expected_len {
        log::info!("Crash reporter: raw crash file truncated, discarding");
        let _ = std::fs::remove_file(path);
        return None;
    }

    let mut addresses = Vec::with_capacity(frame_count);
    for i in 0..frame_count {
        let offset = FRAMES_START + i * 8;
        let addr = u64::from_le_bytes(data[offset..offset + 8].try_into().ok()?);
        addresses.push(addr);
    }

    let version_slice = &data[frames_end..frames_end + APP_VERSION_FIELD_LEN];
    let app_version = std::str::from_utf8(version_slice)
        .ok()?
        .trim_end_matches('\0')
        .to_string();

    Some((signal, addresses, image_base, app_version))
}
