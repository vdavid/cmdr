//! The kernel's VM map, folded by tag: the reader that spans BOTH allocators.
//!
//! Split out of the parent module because this is a different accountant with its
//! own ABI (`mach_vm_region_recurse` and its packed info struct) and its own
//! vocabulary of tags and region sizes. `process_memory`'s module docs hold the
//! argument for why it exists at all; everything here is how.

/// One distinct region size under a tag: how many regions are exactly this big,
/// and what they hold between them.
///
/// This is the field that names an anonymous block. A tag total says "643 MB of
/// `MALLOC_LARGE`"; the size groups say "71 regions of 9,437,184 bytes and 143
/// of 2,359,296", and a repeated exact size is a fingerprint of whatever sizes
/// it — a fixed buffer, a matrix, an arena step.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RegionSizeGroup {
    /// The size every region in this group has, in bytes.
    pub region_bytes: u64,
    /// How many regions are exactly this size.
    pub count: u32,
    /// Dirty bytes across the group.
    pub dirty_bytes: u64,
}

/// One VM tag's share of the process's address space.
///
/// The tag is what `vmmap` prints in its left column, so these rows compare
/// directly against a `vmmap -summary` of the same process — and, unlike the
/// malloc-zone APIs, this view sees mimalloc's arenas too (under the
/// `IOAccelerator` name; see the module docs).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TagUsage {
    /// The raw `user_tag` from the map entry.
    pub tag: u32,
    /// The tag's `vmmap`-style name, or `tag-<n>` when we don't carry one.
    pub name: String,
    /// Dirty bytes: pages this process wrote, so pages it pays for. The column
    /// to read.
    pub dirty_bytes: u64,
    /// Dirty pages since compressed or swapped out.
    pub swapped_bytes: u64,
    /// Resident bytes, clean pages (mapped files, shared text) included.
    pub resident_bytes: u64,
    /// Address space reserved, most of which is typically untouched.
    pub virtual_bytes: u64,
    /// How many map entries carry this tag.
    pub region_count: u32,
    /// The tag's distinct region sizes, biggest dirty total first, capped by the
    /// caller's `sizes_per_tag`.
    pub sizes: Vec<RegionSizeGroup>,
}

/// The process's whole VM map, folded by tag.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VmRegionMap {
    /// Every tag present, biggest dirty total first.
    pub tags: Vec<TagUsage>,
    /// Dirty bytes across every region.
    pub total_dirty_bytes: u64,
    /// How many map entries the walk saw.
    pub total_region_count: u32,
    /// Set when the walk hit [`MAX_REGIONS`] or [`MAX_STEPS`] and stopped early,
    /// so the totals are a floor rather than a total. A runaway is exactly when a
    /// diagnostic must not quietly under-report.
    pub truncated: bool,
}

/// `vmmap`'s names for the tags a Cmdr process actually maps. Anything outside
/// this table renders as `tag-<n>`, which is still a usable fingerprint.
///
/// `100` is the one to know: mimalloc tags its arenas with it and macOS calls it
/// `IOAccelerator`, so those rows are the Rust heap (module docs). The name here
/// carries that inline, because whoever meets it in a diagnostic payload has no
/// module docs in front of them.
const TAG_NAMES: &[(u32, &str)] = &[
    (0, "unnamed"),
    (1, "MALLOC"),
    (2, "MALLOC_SMALL"),
    (3, "MALLOC_LARGE"),
    (4, "MALLOC_HUGE"),
    (6, "REALLOC"),
    (7, "MALLOC_TINY"),
    (8, "MALLOC_LARGE_REUSABLE"),
    (9, "MALLOC_LARGE_REUSED"),
    (11, "MALLOC_NANO"),
    (12, "MALLOC_MEDIUM"),
    (13, "MALLOC_PROB_GUARD"),
    (20, "MACH_MSG"),
    (21, "IOKit"),
    (22, "VM_RECLAIM"),
    (30, "Stack"),
    (31, "Guard"),
    (32, "Shared pmap"),
    (33, "__DATA/dylib"),
    (34, "ObjC dispatchers"),
    (35, "Unshared pmap"),
    (36, "libchannel"),
    (40, "AppKit"),
    (41, "Foundation"),
    (42, "CoreGraphics"),
    (43, "CoreServices"),
    (45, "CoreData"),
    (50, "ATS (font support)"),
    (51, "CoreAnimation"),
    (52, "CG image"),
    (54, "CoreGraphics data"),
    (55, "CoreGraphics shared"),
    (56, "CoreGraphics framebuffers"),
    (57, "CoreGraphics backing stores"),
    (60, "dyld"),
    (61, "dyld malloc"),
    (62, "SQLite page cache"),
    (63, "JavaScriptCore heap"),
    (64, "JS JIT executable"),
    (65, "JS JIT register file"),
    (68, "CoreImage"),
    (70, "ImageIO"),
    (73, "OS Alloc Once"),
    (74, "libdispatch"),
    (75, "Accelerate framework"),
    (76, "CoreUI"),
    (77, "CoreUI image file"),
    (82, "Swift runtime"),
    (83, "Swift metadata"),
    (88, "IOSurface"),
    (89, "libnetwork"),
    (90, "Audio"),
    (97, "QuickLook thumbnails"),
    (100, "IOAccelerator (= our Rust heap: mimalloc arenas)"),
    (103, "CoreUI cached image data"),
    (104, "ColorSync"),
    (107, "Compositor Services"),
];

/// The walk's iteration ceiling. A busy Cmdr maps a few thousand regions, so this
/// only trips on something pathological, and then it sets
/// [`VmRegionMap::truncated`] rather than spinning.
const MAX_REGIONS: u32 = 200_000;

/// Turns the walk will take at most, submap descents included.
///
/// [`MAX_REGIONS`] alone doesn't bound the loop: a descent advances `depth` but
/// deliberately NOT `address` or the region count, and `nesting_depth` is an
/// in/out parameter the kernel writes back, so a climbing `depth` is not proof
/// of progress. This counter is: every turn increments it, so the walk provably
/// terminates whatever the map does, and hitting it sets
/// [`VmRegionMap::truncated`] like the region ceiling does. Twice `MAX_REGIONS`
/// leaves room for far more descents than the shared region's two levels.
const MAX_STEPS: u32 = MAX_REGIONS * 2;

/// `vm_region_submap_info_64` from `<mach/vm_region.h>`, through its v2 fields.
///
/// The `count` we hand `mach_vm_region_recurse` is this struct's size in
/// `natural_t` (u32) words, which is how that ABI measures it; the kernel fills
/// the newest version the count covers and reports back how much it wrote. We
/// read only v0 fields, which every version has.
///
/// ⚠️ **`packed(4)` is load-bearing, not decoration.** The whole of
/// `<mach/vm_region.h>` sits inside `#pragma pack(push, 4)`, so `offset`'s `u64`
/// starts at byte 12 and every field after it is 4 bytes earlier than natural
/// alignment would put it. A plain `#[repr(C)]` reads `user_tag` from
/// `pages_resident`'s bytes and the walk comes back as plausible-looking
/// nonsense: tags above 255, region counts an order of magnitude short, and a
/// fresh 9 MiB allocation nowhere in the map (verified on macOS 26.5, 2026-08-21).
#[repr(C, packed(4))]
struct VmRegionSubmapInfo64 {
    protection: i32,
    max_protection: i32,
    inheritance: u32,
    offset: u64,
    user_tag: u32,
    pages_resident: u32,
    pages_shared_now_private: u32,
    pages_swapped_out: u32,
    pages_dirtied: u32,
    ref_count: u32,
    shadow_depth: u16,
    external_pager: u8,
    share_mode: u8,
    is_submap: i32,
    behavior: i32,
    object_id: u32,
    user_wired_count: u16,
    flags: u16,
    pages_reusable: u32,
    object_id_full: u64,
}

unsafe extern "C" {
    /// Walk one entry of the task's VM map at or below `nesting_depth`, starting
    /// at `address`. Returns `KERN_INVALID_ADDRESS` past the last entry, which is
    /// what ends the walk.
    fn mach_vm_region_recurse(
        target_task: libc::mach_port_t,
        address: *mut u64,
        size: *mut u64,
        nesting_depth: *mut u32,
        info: *mut VmRegionSubmapInfo64,
        info_count: *mut u32,
    ) -> libc::kern_return_t;
}

/// Walk this process's VM map and fold it by tag: an in-process `vmmap -summary`,
/// plus a per-tag histogram of distinct region sizes.
///
/// **This is the reader that sees BOTH allocators.** [`super::query_mimalloc_heap`] and
/// [`super::query_system_malloc_zones`] each see exactly one, and neither can say what
/// SHAPE the bytes are in. The kernel's map can, because every allocator
/// ultimately takes its pages from it, which is what makes a repeated exact
/// region size the cheapest available fingerprint of an unattributed block.
///
/// `sizes_per_tag` caps the histogram per tag (biggest dirty totals win); 0 asks
/// for tag totals only.
///
/// **Snapshot-only, never per-tick.** It costs one `mach_vm_region_recurse`
/// syscall per map entry: low single-digit milliseconds for a few thousand
/// regions, against the single `task_info` call the cheap readers make.
///
/// `None` if the very first query fails.
pub fn query_vm_regions(sizes_per_tag: usize) -> Option<VmRegionMap> {
    // SAFETY: `_SC_PAGESIZE` is a valid `sysconf` name and the call has no side effects.
    let page_size = unsafe { libc::sysconf(libc::_SC_PAGESIZE) };
    let page_size = if page_size > 0 { page_size as u64 } else { 4096 };

    let mut by_tag: std::collections::HashMap<u32, TagAccumulator> = std::collections::HashMap::new();
    let mut address: u64 = 0;
    let mut depth: u32 = 0;
    let mut seen: u32 = 0;
    let mut steps: u32 = 0;

    while seen < MAX_REGIONS && steps < MAX_STEPS {
        steps += 1;
        let mut size: u64 = 0;
        let mut count = (size_of::<VmRegionSubmapInfo64>() / size_of::<u32>()) as u32;
        #[allow(deprecated, reason = "mach_task_self is deprecated in libc but works fine")]
        // SAFETY: every pointer is to an initialized local. `info` is a `#[repr(C, packed(4))]`
        // match of `vm_region_submap_info_64` — the packing is what makes the layout match, since
        // the header sits inside `#pragma pack(push, 4)` (see the struct's docs) — and `count` is
        // its size in `natural_t` words, so the kernel writes only within it (it fills the newest
        // version that count covers). `address`, `size`, and `depth` are the in/out cursor the
        // recurse ABI defines. We read `info` only after the call returned `KERN_SUCCESS`.
        let (kr, info) = unsafe {
            let mut info: VmRegionSubmapInfo64 = std::mem::zeroed();
            let kr = mach_vm_region_recurse(
                libc::mach_task_self(),
                &mut address,
                &mut size,
                &mut depth,
                &mut info,
                &mut count,
            );
            (kr, info)
        };
        // Anything but success means the walk ran off the end of the map.
        if kr != 0 {
            break;
        }
        if info.is_submap != 0 {
            // A submap (the shared region): descend into it rather than past it,
            // WITHOUT advancing the address or the region count. `steps` is what
            // keeps this branch from looping forever (see [`MAX_STEPS`]).
            depth += 1;
            continue;
        }

        seen += 1;
        let dirty = u64::from(info.pages_dirtied) * page_size;
        let entry = by_tag.entry(info.user_tag).or_default();
        entry.dirty += dirty;
        entry.swapped += u64::from(info.pages_swapped_out) * page_size;
        entry.resident += u64::from(info.pages_resident) * page_size;
        entry.virtual_bytes += size;
        entry.region_count += 1;
        if sizes_per_tag > 0 {
            let group = entry.sizes.entry(size).or_insert((0, 0));
            group.0 += 1;
            group.1 += dirty;
        }
        // Advance past this region. A zero-sized entry would otherwise stall the walk.
        address = address.saturating_add(size.max(page_size));
    }
    if seen == 0 {
        return None;
    }

    let mut tags: Vec<TagUsage> = by_tag
        .into_iter()
        .map(|(tag, acc)| {
            let mut sizes: Vec<RegionSizeGroup> = acc
                .sizes
                .into_iter()
                .map(|(region_bytes, (count, dirty_bytes))| RegionSizeGroup {
                    region_bytes,
                    count,
                    dirty_bytes,
                })
                .collect();
            sizes.sort_by(|a, b| {
                b.dirty_bytes
                    .cmp(&a.dirty_bytes)
                    .then(b.region_bytes.cmp(&a.region_bytes))
            });
            sizes.truncate(sizes_per_tag);
            TagUsage {
                tag,
                name: tag_name(tag),
                dirty_bytes: acc.dirty,
                swapped_bytes: acc.swapped,
                resident_bytes: acc.resident,
                virtual_bytes: acc.virtual_bytes,
                region_count: acc.region_count,
                sizes,
            }
        })
        .collect();
    tags.sort_by(|a, b| b.dirty_bytes.cmp(&a.dirty_bytes).then(a.tag.cmp(&b.tag)));

    Some(VmRegionMap {
        total_dirty_bytes: tags.iter().map(|t| t.dirty_bytes).sum(),
        total_region_count: seen,
        truncated: seen >= MAX_REGIONS || steps >= MAX_STEPS,
        tags,
    })
}

/// Per-tag fold state while the walk runs. `sizes` maps an exact region size to
/// `(count, dirty bytes)`.
// DEFAULT-OK: every field starts at zero, which is what a freshly-seen tag means.
#[derive(Default)]
struct TagAccumulator {
    dirty: u64,
    swapped: u64,
    resident: u64,
    virtual_bytes: u64,
    region_count: u32,
    sizes: std::collections::HashMap<u64, (u32, u64)>,
}

/// A tag's `vmmap`-style name, or `tag-<n>` for one we don't carry.
fn tag_name(tag: u32) -> String {
    TAG_NAMES
        .iter()
        .find(|(t, _)| *t == tag)
        .map_or_else(|| format!("tag-{tag}"), |(_, name)| (*name).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The `MALLOC_LARGE` user tag, from `<mach/vm_statistics.h>`.
    const TAG_MALLOC_LARGE: u32 = 3;
    /// The `IOAccelerator` user tag, which mimalloc claims for its arenas.
    const TAG_IOACCELERATOR: u32 = 100;

    fn tag(map: &VmRegionMap, tag: u32) -> Option<&TagUsage> {
        map.tags.iter().find(|t| t.tag == tag)
    }

    #[test]
    fn the_region_walk_covers_the_whole_address_space() {
        let map = query_vm_regions(8).expect("the VM map should be walkable in-process");
        assert!(map.total_region_count > 10, "a live process maps more than 10 regions");
        assert!(!map.truncated, "a test process is nowhere near the region ceiling");
        assert!(map.total_dirty_bytes > 0, "a live process has dirty pages");
        assert!(
            map.tags.windows(2).all(|w| w[0].dirty_bytes >= w[1].dirty_bytes),
            "tags come back biggest dirty total first"
        );
    }

    #[test]
    fn sizes_per_tag_zero_asks_for_tag_totals_only() {
        let map = query_vm_regions(0).expect("the VM map should be walkable in-process");
        assert!(
            map.tags.iter().all(|t| t.sizes.is_empty()),
            "a zero histogram cap yields no size groups"
        );
        assert!(map.total_dirty_bytes > 0, "the totals are still collected");
    }

    #[test]
    fn a_big_system_zone_block_becomes_a_malloc_large_region_of_exactly_its_size() {
        // The mechanism the 2026-08 memory attribution leans on: macOS routes any
        // allocation past its 127 KB large-zone threshold to its own VM region, sized
        // to the request. So a repeated exact region size in `MALLOC_LARGE` is the
        // fingerprint of whatever asked for that many bytes — which is how a block
        // no allocator API can name still gets named. ❌ Break this and the region
        // histogram stops being evidence.
        //
        // 9 MiB is not arbitrary: it's one of the two exact sizes the 2026-07-28 idle
        // profile reported for its unattributed block, and a Core ML scratch buffer
        // lands on it (`docs/notes/idle-malloc-large-clip-towers-2026-08-21.md`).
        const BLOCK: usize = 9 * 1024 * 1024;

        let before = query_vm_regions(64).expect("walkable");
        let before_dirty = tag(&before, TAG_MALLOC_LARGE).map_or(0, |t| t.dirty_bytes);

        // SAFETY: `malloc` returns an owned block of at least `BLOCK` bytes or null; we
        // null-check, write only within `BLOCK` bytes of it, and `free` it exactly once.
        // It goes through the SYSTEM allocator on purpose: mimalloc is the global
        // allocator in the shipped binary but not in this test harness, and this test is
        // about the system zone either way.
        let (after, block) = unsafe {
            let block = libc::malloc(BLOCK).cast::<u8>();
            assert!(!block.is_null(), "malloc should hand back a {BLOCK}-byte block");
            std::ptr::write_bytes(block, 1u8, BLOCK);
            (query_vm_regions(64).expect("walkable"), block)
        };
        // SAFETY: `block` came from `malloc` above and is freed exactly once, after the
        // measurement that needed it alive.
        unsafe { libc::free(block.cast()) };

        let large = tag(&after, TAG_MALLOC_LARGE).expect("a 9 MiB block puts something in MALLOC_LARGE");
        assert!(
            large.dirty_bytes >= before_dirty + (BLOCK as u64) / 2,
            "MALLOC_LARGE should grow by most of the block: {before_dirty} -> {}",
            large.dirty_bytes
        );
        assert!(
            large.sizes.iter().any(|s| s.region_bytes == BLOCK as u64),
            // allowed-pluralize-noun: `BLOCK` is a compile-time 9 MiB constant, never 1.
            "the block should show as a region of exactly {BLOCK} bytes, saw {:?}",
            large.sizes.iter().map(|s| s.region_bytes).collect::<Vec<_>>()
        );
    }

    #[test]
    fn the_rust_heap_shows_up_under_the_ioaccelerator_tag() {
        // The trap the module docs open with, asserted rather than only written down:
        // mimalloc tags its arenas with `VM_MEMORY_IOACCELERATOR`, so a Rust-heap
        // runaway reads as graphics memory in `vmmap` and in this walk alike.
        const CHUNK: usize = 256 * 1024 * 1024;

        let before = query_vm_regions(0).expect("walkable");
        let before_dirty = tag(&before, TAG_IOACCELERATOR).map_or(0, |t| t.dirty_bytes);

        // SAFETY: `mi_malloc` returns an owned block of at least `CHUNK` bytes or null; we
        // null-check, write only within `CHUNK` bytes of it, and `mi_free` it exactly once.
        let (after, block) = unsafe {
            let block = libmimalloc_sys::mi_malloc(CHUNK).cast::<u8>();
            assert!(!block.is_null(), "mi_malloc should hand back a {CHUNK}-byte block");
            std::ptr::write_bytes(block, 1u8, CHUNK);
            (query_vm_regions(0).expect("walkable"), block)
        };
        // SAFETY: `block` came from `mi_malloc` above and is freed exactly once.
        unsafe { libmimalloc_sys::mi_free(block.cast()) };

        let accel = tag(&after, TAG_IOACCELERATOR).expect("mimalloc arenas carry the IOAccelerator tag");
        assert!(
            accel.dirty_bytes >= before_dirty + (CHUNK as u64) / 2,
            "the mimalloc block should land under IOAccelerator: {before_dirty} -> {}",
            accel.dirty_bytes
        );
        assert!(
            accel.name.contains("Rust heap"),
            "the tag name has to carry the trap: {}",
            accel.name
        );
    }
}
