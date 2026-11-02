//! macOS: the region comes from `mach_vm_region_recurse`, and the images from dyld's callbacks.
//!
//! `mach_vm_region_recurse` still answers on a mapping whose vnode is dead, and reports it as
//! `external_pager` (verified on Darwin 27.0.0 arm64 with a RAM disk force-unmounted under an
//! 8 MiB mapping, 2026-10-19: 511 pages recovered, `si_code` 1, signal SIGBUS).

use std::sync::Once;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};

use super::image_ranges::RangeTable;
use super::verdict::{Region, page_to_recover};
use crate::process_memory::{VmRegionSubmapInfo64, mach_vm_region_recurse};

/// Room for every image a Cmdr process loads (several hundred, most of them in the shared
/// cache) with plenty to spare. Past it, recovery switches off rather than guess.
const IMAGE_CAPACITY: usize = 4096;

/// Every loaded image's `[lowest segment, highest segment end)`, slide applied.
static IMAGES: RangeTable<IMAGE_CAPACITY> = RangeTable::new();

static PAGE_SIZE: AtomicU64 = AtomicU64::new(0);

/// Set once dyld has handed over every image already loaded.
static READY: AtomicBool = AtomicBool::new(false);

// `<mach-o/loader.h>`, the three shapes the extent walk reads. Declared here because `libc`'s
// copies are deprecated in favor of a crate this one doesn't otherwise need.

const LC_SEGMENT_64: u32 = 0x19;

#[repr(C)]
struct MachHeader64 {
    magic: u32,
    cputype: i32,
    cpusubtype: i32,
    filetype: u32,
    ncmds: u32,
    sizeofcmds: u32,
    flags: u32,
    reserved: u32,
}

#[repr(C)]
struct LoadCommand {
    cmd: u32,
    cmdsize: u32,
}

#[repr(C)]
struct SegmentCommand64 {
    cmd: u32,
    cmdsize: u32,
    segname: [u8; 16],
    vmaddr: u64,
    vmsize: u64,
    fileoff: u64,
    filesize: u64,
    maxprot: i32,
    initprot: i32,
    nsects: u32,
    flags: u32,
}

unsafe extern "C" {
    /// Calls `callback` for every image already loaded, then for each one loaded later.
    fn _dyld_register_func_for_add_image(callback: extern "C" fn(*const MachHeader64, isize));
    /// Calls `callback` for each image unloaded from now on.
    fn _dyld_register_func_for_remove_image(callback: extern "C" fn(*const MachHeader64, isize));
    /// The task's own port. What `mach_task_self()` reads; a plain load, so signal-safe.
    static mach_task_self_: libc::mach_port_t;
}

pub(super) fn prepare() {
    static ONCE: Once = Once::new();
    ONCE.call_once(|| {
        // SAFETY: `_SC_PAGESIZE` is a valid `sysconf` name with no side effects.
        let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
        PAGE_SIZE.store(if page_size > 0 { page_size as u64 } else { 0 }, Ordering::SeqCst);
        // SAFETY: both callbacks are `extern "C"` with the signature dyld calls, can't unwind, and
        // live for the whole process, as a registration must (there is no unregister).
        unsafe {
            _dyld_register_func_for_add_image(on_image_added);
            _dyld_register_func_for_remove_image(on_image_removed);
        }
        READY.store(true, Ordering::SeqCst);
    });
}

extern "C" fn on_image_added(header: *const MachHeader64, slide: isize) {
    // SAFETY: dyld hands a loaded image's live header.
    match unsafe { image_extent(header, slide) } {
        Some((start, end)) => IMAGES.add(start, end),
        None => IMAGES.give_up(),
    }
}

extern "C" fn on_image_removed(header: *const MachHeader64, slide: isize) {
    // SAFETY: dyld hands the header of an image still mapped while it's being unloaded.
    if let Some((start, end)) = unsafe { image_extent(header, slide) } {
        IMAGES.remove(start, end);
    }
}

/// The image's `[start, end)`: from its lowest segment to its highest segment's end, slide
/// applied, `__PAGEZERO` (which maps nothing) left out. `None` for a header this can't walk.
///
/// # Safety
///
/// `header` must point at a loaded image's mach header, followed by its load commands.
unsafe fn image_extent(header: *const MachHeader64, slide: isize) -> Option<(u64, u64)> {
    const MH_MAGIC_64: u32 = 0xfeed_facf;
    if header.is_null() {
        return None;
    }
    // SAFETY: the caller promises a live header; a 64-bit one is the only kind macOS loads here,
    // and the magic check below refuses anything else before a 64-bit field is used.
    let fixed = unsafe { &*header };
    if fixed.magic != MH_MAGIC_64 {
        return None;
    }
    // Offsets from `header` itself, never from `fixed`: a reference covers only the 32 bytes of
    // its type, so a pointer derived from it can't reach the commands after them (Miri caught
    // that as undefined behavior, `macos_test.rs`).
    let commands_end = size_of::<MachHeader64>() + fixed.sizeofcmds as usize;
    let mut command = size_of::<MachHeader64>();
    let (mut low, mut high) = (u64::MAX, 0u64);
    for _ in 0..fixed.ncmds {
        if command + size_of::<LoadCommand>() > commands_end {
            return None;
        }
        // SAFETY: `command` lies inside the load-command area the header sizes, checked above.
        let load = unsafe { &*header.byte_add(command).cast::<LoadCommand>() };
        if (load.cmdsize as usize) < size_of::<LoadCommand>() || command + load.cmdsize as usize > commands_end {
            return None;
        }
        if load.cmd == LC_SEGMENT_64 && load.cmdsize as usize >= size_of::<SegmentCommand64>() {
            // SAFETY: an `LC_SEGMENT_64` command of at least this size is a `segment_command_64`.
            let segment = unsafe { &*header.byte_add(command).cast::<SegmentCommand64>() };
            let is_page_zero = segment.vmaddr == 0 && segment.initprot == 0 && segment.maxprot == 0;
            if !is_page_zero && segment.vmsize > 0 {
                let start = segment.vmaddr.wrapping_add_signed(slide as i64);
                low = low.min(start);
                high = high.max(start.saturating_add(segment.vmsize));
            }
        }
        command += load.cmdsize as usize;
    }
    (low < high).then_some((low, high))
}

/// What the VM map says about the region at or after `address`.
fn region_at(address: u64) -> Option<Region> {
    let mut start = address;
    let mut size: u64 = 0;
    let mut depth: u32 = 0;
    let mut count = (size_of::<VmRegionSubmapInfo64>() / size_of::<u32>()) as u32;
    // SAFETY: every pointer is to an initialized local; `count` is `info`'s size in `natural_t`
    // words, so the kernel writes only within it. `info` is read only after `KERN_SUCCESS`. A
    // Mach trap allocates nothing and takes no user-space lock.
    let (kr, info) = unsafe {
        let mut info: VmRegionSubmapInfo64 = std::mem::zeroed();
        let kr = mach_vm_region_recurse(
            mach_task_self_,
            &mut start,
            &mut size,
            &mut depth,
            &mut info,
            &mut count,
        );
        (kr, info)
    };
    if kr != 0 || info.is_submap != 0 {
        // A submap is the shared region, which holds only the dyld shared cache's images.
        return None;
    }
    let protection = info.protection;
    Some(Region {
        start,
        end: start.saturating_add(size),
        file_backed: info.external_pager != 0,
        writable: protection & libc::VM_PROT_WRITE != 0,
        executable: protection & libc::VM_PROT_EXECUTE != 0,
    })
}

/// Async-signal-safe: atomic loads, one Mach trap, one `mmap`.
pub(super) fn recover(info: &libc::siginfo_t) -> bool {
    if !READY.load(Ordering::SeqCst) || !IMAGES.is_complete() {
        return false;
    }
    let address = info.si_addr as u64;
    let page_size = PAGE_SIZE.load(Ordering::SeqCst);
    let Some(page) = page_to_recover(address, page_size, region_at(address), IMAGES.contains(address)) else {
        return false;
    };
    super::map_zeros_over(page, page_size)
}

/// Whether `address` lies in a loaded image, for the tests that prove the table is filled.
#[cfg(test)]
pub(super) fn is_in_image(address: u64) -> bool {
    IMAGES.contains(address)
}

/// What [`recover`] would decide for `address`, without a fault, for the tests.
#[cfg(test)]
pub(super) fn would_recover(address: u64) -> bool {
    page_to_recover(
        address,
        PAGE_SIZE.load(Ordering::SeqCst),
        region_at(address),
        IMAGES.contains(address),
    )
    .is_some()
}

#[cfg(test)]
#[path = "macos_test.rs"]
mod tests;
