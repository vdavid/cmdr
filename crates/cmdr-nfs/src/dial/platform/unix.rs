//! Both Unixes: `gethostname`, `getuid`, and `getgid`.

pub(crate) fn machine_name() -> String {
    let mut buf = [0u8; 256];
    // SAFETY: `buf` is valid for `buf.len()` bytes for the whole call, and
    // `gethostname` writes at most that many.
    let status = unsafe { libc::gethostname(buf.as_mut_ptr().cast(), buf.len()) };
    if status != 0 {
        return String::new();
    }
    let len = buf.iter().position(|&b| b == 0).unwrap_or(buf.len());
    String::from_utf8_lossy(&buf[..len]).into_owned()
}

pub(crate) fn process_ids() -> (u32, u32) {
    // SAFETY: `getuid` and `getgid` take nothing, can't fail, and touch no
    // memory of ours.
    unsafe { (libc::getuid(), libc::getgid()) }
}
