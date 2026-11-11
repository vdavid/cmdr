//! macOS: signal handlers, plus the uncaught Objective-C exception's reason in a sidecar
//! (`objc_exception.rs`); the thread count from the Mach task.

use std::path::Path;

use super::super::objc_exception;
pub(crate) use super::unix::current_image_base;

/// Signal handlers, then the Objective-C exception handler.
pub(crate) fn install_native_crash_hooks(raw_crash_path: &Path, data_dir: &Path) {
    super::unix::install_signal_handlers(raw_crash_path);
    objc_exception::install(&data_dir.join(objc_exception::FILE_NAME));
}

/// Mach API: get thread list for the current task, return the count.
pub(crate) fn current_thread_count() -> usize {
    unsafe extern "C" {
        fn mach_task_self() -> libc::mach_port_t;
    }
    // SAFETY: `thread_list` and `thread_count` are live locals whose addresses are valid
    // out-params for `task_threads`. On `KERN_SUCCESS` the kernel hands back an allocated
    // array of `thread_count` ports in `thread_list`; we hand that exact array and its byte
    // size (`thread_count * size_of::<mach_port_t>()`) back to `vm_deallocate`, freeing it
    // exactly once. We deallocate only on `KERN_SUCCESS`, where the out-params are valid.
    unsafe {
        let mut thread_list: libc::mach_port_t = 0;
        let mut thread_count: u32 = 0;
        let kr = libc::task_threads(
            mach_task_self(),
            std::ptr::addr_of_mut!(thread_list) as *mut *mut libc::mach_port_t,
            &raw mut thread_count,
        );
        if kr == libc::KERN_SUCCESS {
            // Deallocate the thread list (we only needed the count)
            libc::vm_deallocate(
                mach_task_self(),
                thread_list as libc::vm_address_t,
                (thread_count as usize) * size_of::<libc::mach_port_t>(),
            );
            thread_count as usize
        } else {
            0
        }
    }
}
