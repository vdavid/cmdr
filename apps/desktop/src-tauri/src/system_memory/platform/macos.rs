//! macOS: the Mach VM statistics (`host_statistics64`), for the same non-overlapping categories
//! Activity Monitor shows. Falls back to `sysinfo` when the call fails.

use super::SystemMemoryInfo;

pub(crate) fn system_memory_info() -> SystemMemoryInfo {
    use std::mem;

    let (total_bytes, _) = super::super::sysinfo_memory();

    let page_size: u64;
    let (wired_pages, compressor_pages, internal_pages, purgeable_pages);

    // SAFETY: `vm_info` is zeroed before use, and `count` is set to the struct's size measured in
    // `integer_t` words (`size_of::<vm_statistics64>() / size_of::<integer_t>()`), which is the
    // layout `host_statistics64` with `HOST_VM_INFO64` expects, so it writes only within `vm_info`.
    // `sysconf` and `mach_host_self` take no pointers. We read `vm_info`'s fields only after the
    // call returns `KERN_SUCCESS`.
    unsafe {
        page_size = libc::sysconf(libc::_SC_PAGESIZE) as u64;

        #[allow(deprecated, reason = "libc says use mach2, but not worth a new dep for one call")]
        let host = libc::mach_host_self();
        let mut vm_info: libc::vm_statistics64 = mem::zeroed();
        let mut count = (size_of::<libc::vm_statistics64>() / size_of::<libc::integer_t>()) as u32;

        let ret = libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            &mut vm_info as *mut _ as *mut libc::integer_t,
            &mut count,
        );

        if ret != libc::KERN_SUCCESS {
            log::warn!("host_statistics64 returned {ret}, falling back to sysinfo");
            return super::super::sysinfo_breakdown();
        }

        wired_pages = vm_info.wire_count as u64;
        compressor_pages = vm_info.compressor_page_count as u64;
        // internal_page_count = anonymous pages owned by processes (what Activity Monitor calls "App
        // Memory"). Unlike active+inactive, this excludes file-backed cache that macOS freely
        // reclaims.
        internal_pages = vm_info.internal_page_count as u64;
        purgeable_pages = vm_info.purgeable_count as u64;
    }

    let wired_bytes = (wired_pages + compressor_pages) * page_size;
    let app_bytes = internal_pages.saturating_sub(purgeable_pages) * page_size;
    // Free = everything not wired or app (includes file cache, inactive, purgeable, speculative)
    let free_bytes = total_bytes.saturating_sub(wired_bytes + app_bytes);

    SystemMemoryInfo {
        total_bytes,
        wired_bytes,
        app_bytes,
        free_bytes,
    }
}
