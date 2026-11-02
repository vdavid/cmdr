//! `image_extent` over hand-built Mach-O headers: no dyld, so Miri runs these and checks the
//! load-command walk's pointer work (`desktop-rust-miri`).

use super::*;

/// A 64-bit Mach-O header followed by its load commands, in `u64` words so it's 8-aligned the
/// way dyld maps one.
struct Image {
    commands: Vec<u8>,
    ncmds: u32,
    magic: u32,
}

impl Image {
    fn new() -> Self {
        Self {
            commands: Vec::new(),
            ncmds: 0,
            magic: 0xfeed_facf,
        }
    }

    fn segment(mut self, vmaddr: u64, vmsize: u64, prot: i32) -> Self {
        let cmdsize = size_of::<SegmentCommand64>() as u32;
        self.raw_command(LC_SEGMENT_64, cmdsize);
        let fields_start = self.commands.len();
        self.commands.extend_from_slice(&[0u8; 16]); // segname
        self.commands.extend_from_slice(&vmaddr.to_ne_bytes());
        self.commands.extend_from_slice(&vmsize.to_ne_bytes());
        self.commands.extend_from_slice(&0u64.to_ne_bytes()); // fileoff
        self.commands.extend_from_slice(&0u64.to_ne_bytes()); // filesize
        self.commands.extend_from_slice(&prot.to_ne_bytes()); // maxprot
        self.commands.extend_from_slice(&prot.to_ne_bytes()); // initprot
        self.commands.extend_from_slice(&0u32.to_ne_bytes()); // nsects
        self.commands.extend_from_slice(&0u32.to_ne_bytes()); // flags
        assert_eq!(
            self.commands.len() - fields_start + size_of::<LoadCommand>(),
            cmdsize as usize
        );
        self
    }

    /// A command this walk doesn't read (`LC_UUID`'s number), `cmdsize` bytes long.
    fn other(mut self, cmdsize: u32) -> Self {
        self.raw_command(0x1b, cmdsize);
        let body = (cmdsize as usize).saturating_sub(size_of::<LoadCommand>());
        self.commands.extend(std::iter::repeat_n(0u8, body));
        self
    }

    /// Only the `cmd` / `cmdsize` pair, so a test can lie about the size.
    fn raw_command(&mut self, cmd: u32, cmdsize: u32) {
        self.commands.extend_from_slice(&cmd.to_ne_bytes());
        self.commands.extend_from_slice(&cmdsize.to_ne_bytes());
        self.ncmds += 1;
    }

    fn words(&self, sizeofcmds: u32) -> Vec<u64> {
        let mut bytes = Vec::new();
        for field in [self.magic, 0x0100_000c, 0, 6, self.ncmds, sizeofcmds, 0, 0] {
            bytes.extend_from_slice(&field.to_ne_bytes());
        }
        bytes.extend_from_slice(&self.commands);
        bytes.resize(bytes.len().div_ceil(8) * 8, 0);
        bytes
            .as_chunks::<8>()
            .0
            .iter()
            .copied()
            .map(u64::from_ne_bytes)
            .collect()
    }

    fn extent(&self, slide: isize) -> Option<(u64, u64)> {
        self.extent_claiming(self.commands.len() as u32, slide)
    }

    fn extent_claiming(&self, sizeofcmds: u32, slide: isize) -> Option<(u64, u64)> {
        let words = self.words(sizeofcmds);
        // SAFETY: `words` holds a whole header and the load-command area it sizes (or less, which
        // is what the bounds checks under test refuse), and outlives the call.
        unsafe { image_extent(words.as_ptr().cast::<MachHeader64>(), slide) }
    }
}

const READ_EXECUTE: i32 = 5;

#[test]
fn the_extent_runs_from_the_lowest_segment_to_the_highest_ones_end_with_the_slide_applied() {
    let image = Image::new()
        .segment(0, 0x1_0000_0000, 0) // __PAGEZERO
        .segment(0x1_0000_0000, 0x4000, READ_EXECUTE)
        .other(24)
        .segment(0x1_0000_8000, 0x2000, READ_EXECUTE);
    assert_eq!(image.extent(0), Some((0x1_0000_0000, 0x1_0000_a000)));
    assert_eq!(image.extent(0x10_0000), Some((0x1_0010_0000, 0x1_0010_a000)));
}

#[test]
fn an_image_with_only_page_zero_has_no_extent() {
    let image = Image::new().segment(0, 0x1_0000_0000, 0);
    assert_eq!(image.extent(0), None);
}

#[test]
fn a_header_that_isnt_64_bit_mach_o_has_no_extent() {
    let mut image = Image::new().segment(0x1000, 0x1000, READ_EXECUTE);
    image.magic = 0xfeed_face;
    assert_eq!(image.extent(0), None);
}

#[test]
fn a_null_header_has_no_extent() {
    // SAFETY: null is the one pointer the function checks for before reading.
    assert_eq!(unsafe { image_extent(std::ptr::null(), 0) }, None);
}

/// The walk must stay inside the area `sizeofcmds` sizes, whatever `ncmds` says.
#[test]
fn a_command_running_past_the_command_area_has_no_extent() {
    let image = Image::new().segment(0x1000, 0x1000, READ_EXECUTE);
    let short = image.commands.len() as u32 - 8;
    assert_eq!(image.extent_claiming(short, 0), None);
}

#[test]
fn more_commands_than_the_area_holds_has_no_extent() {
    let mut image = Image::new().segment(0x1000, 0x1000, READ_EXECUTE);
    image.ncmds += 1;
    assert_eq!(image.extent(0), None);
}

/// A `cmdsize` below a load command's own size would walk in place forever.
#[test]
fn a_command_smaller_than_its_own_header_has_no_extent() {
    let mut image = Image::new();
    image.raw_command(0x1b, 4);
    image.commands.extend_from_slice(&[0u8; 4]);
    assert_eq!(image.extent(0), None);
}
