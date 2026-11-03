use super::image_ranges::RangeTable;
use super::proc_maps::{LINE_CAPACITY, Lines, MapsLine, parse};
use super::verdict::{Region, page_to_recover};

const PAGE: u64 = 0x4000;

fn mapped_file() -> Region {
    Region {
        start: 0x1_0000_0000,
        end: 0x1_0080_0000,
        file_backed: true,
        writable: false,
        executable: false,
    }
}

// ── The verdict ──────────────────────────────────────────────────────

#[test]
fn a_read_only_file_page_outside_every_image_becomes_zeros_from_its_page_start() {
    assert_eq!(
        page_to_recover(0x1_0000_5123, PAGE, Some(mapped_file()), false),
        Some(0x1_0000_4000)
    );
}

#[test]
fn a_page_of_a_loaded_image_is_never_recovered() {
    assert_eq!(page_to_recover(0x1_0000_5123, PAGE, Some(mapped_file()), true), None);
}

#[test]
fn a_writable_mapping_is_never_recovered_since_a_write_there_would_vanish() {
    let region = Region {
        writable: true,
        ..mapped_file()
    };
    assert_eq!(page_to_recover(0x1_0000_5123, PAGE, Some(region), false), None);
}

#[test]
fn an_executable_mapping_is_never_recovered() {
    let region = Region {
        executable: true,
        ..mapped_file()
    };
    assert_eq!(page_to_recover(0x1_0000_5123, PAGE, Some(region), false), None);
}

#[test]
fn anonymous_memory_is_never_recovered() {
    let region = Region {
        file_backed: false,
        ..mapped_file()
    };
    assert_eq!(page_to_recover(0x1_0000_5123, PAGE, Some(region), false), None);
}

#[test]
fn an_address_no_region_covers_is_never_recovered() {
    // macOS answers the NEXT region for an unmapped address.
    assert_eq!(page_to_recover(0x0_ffff_0000, PAGE, Some(mapped_file()), false), None);
    assert_eq!(page_to_recover(0x1_0080_0000, PAGE, Some(mapped_file()), false), None);
    assert_eq!(page_to_recover(0x1_0000_5123, PAGE, None, false), None);
}

#[test]
fn an_unknown_page_size_recovers_nothing() {
    assert_eq!(page_to_recover(0x1_0000_5123, 0, Some(mapped_file()), false), None);
}

// ── The image table ──────────────────────────────────────────────────

#[test]
fn the_image_table_holds_what_was_added_until_it_is_removed() {
    let table: RangeTable<4> = RangeTable::new();
    table.add(0x1000, 0x2000);
    table.add(0x8000, 0x9000);
    assert!(table.contains(0x1000));
    assert!(table.contains(0x8fff));
    assert!(!table.contains(0x2000), "the end is exclusive");
    assert!(!table.contains(0x5000));

    table.remove(0x1000, 0x2000);
    assert!(!table.contains(0x1800));
    assert!(table.contains(0x8800), "removing one image leaves the others");
    assert!(table.is_complete());
}

#[test]
fn an_image_with_no_slot_left_makes_the_table_untrustworthy() {
    let table: RangeTable<1> = RangeTable::new();
    table.add(0x1000, 0x2000);
    assert!(table.is_complete());
    table.add(0x8000, 0x9000);
    assert!(!table.is_complete(), "an unrecorded image would read as fair game");
}

#[test]
fn an_image_whose_extent_cant_be_read_makes_the_table_untrustworthy() {
    let table: RangeTable<4> = RangeTable::new();
    table.give_up();
    assert!(!table.is_complete());
}

// ── `/proc/self/maps` ────────────────────────────────────────────────

#[test]
fn a_maps_line_reads_its_range_permissions_and_file() {
    let line = b"7f3a1c000000-7f3a1c800000 r--s 00000000 08:01 1234567    /mnt/usb/repo/.git/objects/pack/p.pack";
    assert_eq!(
        parse(line),
        Some(MapsLine {
            start: 0x7f3a_1c00_0000,
            end: 0x7f3a_1c80_0000,
            readable: true,
            writable: false,
            executable: false,
            device: (0x08 << 32) | 0x01,
            inode: 1_234_567,
        })
    );
}

#[test]
fn an_anonymous_maps_line_has_no_file() {
    let anonymous = parse(b"7ffd0000-7ffd2000 rw-p 00000000 00:00 0                          [stack]").unwrap();
    assert!(!anonymous.is_file_backed());
    let other = parse(b"1000-2000 r-xp 00000000 00:00 0").unwrap();
    assert!(!anonymous.same_file(&other), "anonymous memory is no file at all");
}

#[test]
fn a_library_is_recognized_by_the_same_file_mapped_executable() {
    let data = parse(b"7f00-8f00 r--p 00000000 fd:00 42 /usr/lib/libc.so.6").unwrap();
    let text = parse(b"8f00-9f00 r-xp 00001000 fd:00 42 /usr/lib/libc.so.6").unwrap();
    let pack = parse(b"af00-bf00 r--s 00000000 fd:00 43 /repo/p.pack").unwrap();
    assert!(text.executable && data.same_file(&text));
    assert!(!pack.same_file(&text));
}

#[test]
fn a_truncated_maps_line_parses_to_nothing() {
    assert_eq!(parse(b"7f00-8f00 r--p"), None);
    assert_eq!(parse(b""), None);
    assert_eq!(parse(b"zz-8f00 r--p 0 fd:00 42"), None);
}

#[test]
fn lines_split_across_chunks_and_keep_only_the_head_of_a_long_line() {
    let long_path = "x".repeat(LINE_CAPACITY * 3);
    let text = format!("1000-2000 r--p 00000000 fd:00 7 /{long_path}\n3000-4000 r--s 00000000 fd:00 8 /b");
    let mut seen: Vec<MapsLine> = Vec::new();
    let mut lines = Lines::new();
    let mut collect = |line: &[u8]| {
        seen.push(parse(line).expect("each line's head parses"));
        true
    };
    for chunk in text.as_bytes().chunks(7) {
        assert!(lines.feed(chunk, &mut collect));
    }
    lines.finish(&mut collect);
    let inodes: Vec<u64> = seen.iter().map(|line| line.inode).collect();
    assert_eq!(inodes, [7, 8], "the unterminated last line still counts");
}

#[test]
fn lines_stop_as_soon_as_the_caller_found_what_it_wanted() {
    let mut lines = Lines::new();
    let mut count = 0;
    let kept_going = lines.feed(b"a\nb\nc\n", &mut |_| {
        count += 1;
        false
    });
    assert!(!kept_going);
    assert_eq!(count, 1);
}

// ── The live process ─────────────────────────────────────────────────

#[cfg(target_os = "macos")]
mod macos_process {
    use std::io::Write as _;
    use std::os::fd::AsRawFd as _;

    use super::super::{macos, prepare};

    #[test]
    #[cfg_attr(miri, ignore = "registers with dyld, which Miri can't cross")]
    fn this_binarys_code_and_libsystem_are_images_once_prepared() {
        prepare();
        let own_code = this_binarys_code_and_libsystem_are_images_once_prepared as *const () as u64;
        let libsystem = libc::write as *const () as u64;
        assert!(
            macos::is_in_image(own_code),
            "a code page of ours must never become zeros"
        );
        assert!(macos::is_in_image(libsystem), "nor one of the shared cache's");
        assert!(!macos::would_recover(own_code));
    }

    #[test]
    #[cfg_attr(miri, ignore = "registers with dyld, which Miri can't cross")]
    fn a_read_only_mapping_of_a_plain_file_is_one_recover_would_take() {
        prepare();
        let mut file = tempfile::tempfile().expect("a temp file");
        file.write_all(&vec![7u8; 64 * 1024]).expect("fill it");
        // SAFETY: a fresh read-only shared mapping of a file this test owns, unmapped below.
        let mapped = unsafe {
            libc::mmap(
                std::ptr::null_mut(),
                64 * 1024,
                libc::PROT_READ,
                libc::MAP_SHARED,
                file.as_raw_fd(),
                0,
            )
        };
        assert_ne!(mapped, libc::MAP_FAILED);
        let address = mapped as u64 + 0x1234;
        assert!(
            macos::would_recover(address),
            "a pack-file-shaped mapping is exactly what recovery is for"
        );
        // SAFETY: the mapping made above, with its own length.
        unsafe { libc::munmap(mapped, 64 * 1024) };
    }

    #[test]
    #[cfg_attr(miri, ignore = "registers with dyld, which Miri can't cross")]
    fn a_heap_address_is_not_one_recover_would_take() {
        prepare();
        let boxed = Box::new([0u8; 64]);
        assert!(!macos::would_recover(boxed.as_ptr() as u64));
    }
}
