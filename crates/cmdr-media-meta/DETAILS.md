# cmdr-media-meta details

The design card is GitHub issue #657. This crate is its engine half: reading. The app's Multi-Rename owns the cache, the
loader, the grammar, and every word a user sees (`apps/desktop/src-tauri/src/multi_rename/DETAILS.md`).

## Why its own crate

Parsing EXIF, tags, and two container formats is self-contained and testable without the app. As a `crates/` member it
compiles and tests alone, `index-crate-isolation` keeps `tauri` out of it, and the app can't reach into its parsers.

## The fields

- `exif`: `taken` (Date), `camera`, `make`, `model`, `lens` (Text), `iso`, `focal` (whole mm), `width`, `height`,
  `orientation` (Number).
- `tag`: `title`, `artist`, `albumartist`, `album`, `genre`, `comment` (Text), `track`, `disc`, `year` (Number).
- `FieldKind` decides the formats a placeholder takes: dates the mask's date grammar, numbers a digit count, text the
  range grammar.
- `MediaField::parse` takes the exact name only (case matters, as in every placeholder).
- `exif.taken` covers videos too: a phone's videos sit beside its photos.

## Which files are media

`is_media_name` goes by extension, in any case; a leading dot belongs to the name, as in the mask.

- Photos: jpg, jpeg, heic, heif, tif, tiff, png, webp, dng, cr2, cr3, nef, arw, orf, rw2, raf.
- Audio: mp3, m4a, m4b, flac, ogg, oga, opus, wav, aif, aiff, ape, wv, mpc, aac.
- Video: mp4, mov, m4v, 3gp, mkv, webm, mka.

**Decision**: CR3 and RAF are in the list though `kamadak-exif` reads neither (ISOBMFF and Fuji containers). **Why**:
leaving them out would make such a photo `NotMedia`, which the preview words as "no date taken", a quiet lie. In the
list, `read_media` answers `NotSupported` without fetching a byte, and the row says it can't read the file.

## Errors through `Read`

Parsers (`exif`, `lofty`) see only `Read + Seek`, so the block reader's typed refusals travel as `io::Error`:
`ReadError::into_io` wraps one in a private payload type, and `ReadError::from_io` downcasts it back. Any other
`io::Error` becomes `Io(text)`, whose text is for the log only. The round trip is typed, ❌ never a message match.
`lofty` wraps an `io::Error` in its own error, so `read.rs` looks through the whole source chain
(`ReadError::from_error`).

## The readers

`read_media` dispatches by extension (`read.rs` `format_of`); `read_media_from` is the loader's entry: a `BlockReader`
over the source, the cover-art pre-pass, then `read_media`. A field the file doesn't have is `None`; a file whose
container is broken is `Io` (the row says it can't read it); a refusal comes back as itself from every reader.

- **Photos** (`photo.rs`): by content, not extension. Our own walks find the EXIF block and `kamadak-exif` parses it
  (`read_raw`, continuing past damaged entries).
  - JPEG: segments up to `SOS`; `APP1 Exif\0\0` is the block, `SOFn` the size. PNG: chunks up to the first `IDAT`
    (`IHDR` size, `eXIf` block). WebP: RIFF chunks (`VP8X` canvas, `EXIF`, with or without `Exif\0\0`).
  - TIFF and TIFF-based RAW (DNG, CR2, NEF, ARW, ORF, RW2): the first 256 KiB as the block; a value pointing past it is
    skipped. ORF (`IIRO`, `IISR`, `MMOR`) and RW2 (`IIU\0`) differ from TIFF only in their magic, patched to `42`.
  - HEIF/HEIC/AVIF: `kamadak-exif`'s `read_from_container`, whose ISOBMFF walk seeks.
  - Fields: `taken` from `DateTimeOriginal`, then `DateTimeDigitized`, then `DateTime`, the camera's wall time as
    written; blank or impossible dates are absent. `focal` rounds `FocalLength`; `orientation` only 1–8; zero ISO or
    size is absent. Size: the container's own (frame header, `IHDR`, `VP8X`) first, then `PixelXDimension`, then IFD0's
    `ImageWidth` unless `NewSubfileType` marks IFD0 a preview.
  - **Decision**: not `read_from_container` for JPEG, PNG, WebP, or TIFF. **Why**: it reads a JPEG's scan, a PNG's
    `IDAT`s, and a whole TIFF through to find the block (kamadak-exif 0.6.1 `jpeg.rs`, `png.rs`, `reader.rs`), so a
    photo without EXIF or any RAW would spend its whole 4 MiB budget and read as `TooLarge`.
- **Audio** (`read.rs`): `lofty`, type by content then extension, `read_properties(false)`, `read_cover_art(false)`. The
  primary tag answers first, other tags fill gaps (an ID3v1 beside an ID3v2). `year` is the tag date's year.
- **MP4 / MOV / M4V / 3GP** (`isobmff.rs`, then `lofty` for `ilst` tags): top-level box headers only, seeking past
  `mdat` (32- and 64-bit sizes, size 0 to the end, a `moof` ends the walk). `taken`: Apple's
  `com.apple.quicktime.creationdate` in `moov/meta` (`keys` + `ilst`) keeps the wall time as written, offset ignored;
  else `mvhd`'s creation time (seconds since 1904 UTC) on this machine's clock. `com.apple.quicktime.make` / `.model`
  fill `make` / `model`. A video `lofty` can't parse (a QuickTime file with no `ftyp`), or whose tags run past the byte
  budget (`TooLarge`), keeps its date; only `Cancelled` and `TimedOut` end the read. The other readers are single-pass:
  a refusal there has no earlier facts to keep (and keeping a half-walked MKV's `DateUTC` could hide the `DATE_RECORDED`
  its `Tags` would have given).
- **MKV / WebM / MKA** (`ebml.rs`): the EBML header, `Segment`, then its children in order up to the first `Cluster`;
  `SeekHead` targets (`Info`, `Tags`, further seek heads; at most 16 jumps) cover what sits past the clusters, which are
  never read. An unknown-size `Segment` runs to the end of the file; any other unknown size ends the walk.
  - `taken`: a `DATE_RECORDED` tag (wall time as written) first, else `Info`'s `DateUTC` (nanoseconds since 2001 UTC) on
    this machine's clock. `DateUTC` is often the muxing time (accepted for v1).
  - Tags: only those aimed at the whole file (a `Targets` with a track, edition, chapter, or attachment UID is skipped).
    `TITLE`, `ARTIST`, `GENRE`, `COMMENT` at the lowest level (30 track before 50 album; no `TargetTypeValue` is 50, as
    ffmpeg writes). `album` is `ALBUM`, else an album-level `TITLE` beside a track-level one; `albumartist` likewise.
    `track` is a track-level `PART_NUMBER`; `year` leads `DATE_RELEASED` or `DATE`.
- **Container clocks** (`clock.rs`): zero is "never set" and a year outside 1970–2199 is a wrong epoch or garbage, both
  absent. UTC → local goes through an injected `ToLocal` (`system_local` in production, a fixed offset in tests), so no
  test depends on the machine's zone.
- **Hostile input**: every walk caps its steps (4,096 boxes, elements, segments, or frames per level), sizes are checked
  and clamped to the parent, values over 1 MiB are skipped (`bytes.rs` `MAX_VALUE_BYTES`), and a value is allocated only
  as far as the file actually holds it. Tests cut fixtures at every length and feed hostile sizes.

## Cover art never travels (`holes.rs`)

`lofty` with `read_cover_art(false)` still reads cover art: an ID3v2 `APIC` frame is skipped by copying it into a sink,
and an MP4 `ilst` is read whole before its atoms are parsed (lofty 0.25.4 `id3::v2::frame::read`, `mp4::ilst::read`,
verified 2026-10-22). So `read_media_from` first maps them to holes:

- ID3v2.3 / 2.4 at the start of the file: each `APIC` frame's body, sizes read exactly as lofty reads them (plain in
  2.3, syncsafe in 2.4). ID3v2.2, a tag-wide unsynchronisation, or an extended header gets no holes: lofty and the walk
  could disagree on frame offsets, so lofty reads it all, within the budget.
- MP4 family: each `moov/udta/meta/ilst/covr/data` payload.
- FLAC `PICTURE` blocks need none: lofty seeks past them (`flac::block`). A Vorbis comment's embedded picture (Ogg,
  Opus) is read: it sits inside the comment packet.
- Tests (`holes_test.rs`): holes cover the picture exactly, lofty reads the tags around them, and a recording source
  under the real `BlockReader` proves a 3 MB cover is never fetched.

## The byte source

- `BlockReader` fetches 64 KiB blocks on demand and keeps the eight most recently used. A header walk is one or two
  round trips on a NAS, and a parser's back-and-forth between a header and a few boxes stays in the kept blocks.
- **Budget**: every fetched byte counts against `MAX_FETCHED_BYTES_PER_FILE` (4 MiB, 64 blocks); a fetch that would pass
  it is `TooLarge`. A kept block costs nothing to read again.
- **Holes** (`with_holes`, any order, overlaps merged) read as zeros and are never fetched: a block wholly inside one is
  never asked for, and a block that straddles one serves real bytes only up to the hole's start. Cover art is a hole, so
  an MP3 with a 3 MB cover costs its header blocks, not 3 MB.
- **Short answers** are asked again for the rest; an empty answer is the end of the file. A file that shrank since the
  listing ends where its bytes do, without an error.
- `VolumeByteSource::read_at` checks the cancel flag and the deadline before each round trip, so a closed sheet or a
  spent budget stops at the next block. The `read_range` runs detached under what's left of the deadline
  (`timeout_detached_within`): on a timeout the transfer finishes on its own, which keeps an MTP transaction whole.
  `VolumeError::NotSupported` maps to `ReadError::NotSupported`; any other volume error is logged there (marked, at
  debug) and becomes a pathless `Io`.
- **No fallback without positioned reads**: a volume whose `read_range` answers `NotSupported` makes the file
  `NotSupported` (unreadable). **Why**: every volume Multi-Rename opens on implements it (local `pread`, SMB, MTP
  `GetPartialObject64`, SFTP, ADB, S3 range GET, WebDAV, restic), and the sheet refuses archives, the `.git` portal, and
  restic snapshots before it opens, so a prefix-stream fallback would be dead code.
- Tests: `source_test.rs`, over a recording in-memory source, `InMemoryVolume`, and `cmdr_fs::testing::WedgedVolume`
  (never answers) for the timeout.

## Dependencies

- `kamadak-exif` 0.6.1 (as `exif`), the version the app already pins.
- `lofty` 0.25.4 (default features), MIT OR Apache-2.0, with `lofty_attr` 0.13.0 and `ogg_pager` 0.7.2 new to the graph;
  `cargo deny check` passes (verified 2026-10-22, crates.io and `cargo deny`; repo not archived, last push 2026-10-17).
- `serde` + `specta` (pinned `=2.0.0-rc.24`, as `cmdr-fs`): `MediaField` crosses IPC inside the app's row statuses.
- `tokio` (`rt`) for `Handle::block_on` in the byte source; `cmdr-fs` for `Volume`, `VolumeError`, and `Deadline`.

## Test fixtures

`test_fixtures.rs` builds every fixture in code (EXIF JPEGs via `kamadak-exif`'s writer, tagged audio via lofty, ISOBMFF
and EBML boxes byte by byte). The `testing` feature publishes it, the way `cmdr-archive` does, so the app's end-to-end
Multi-Rename test (`apps/desktop/src-tauri/src/multi_rename/media_real_test.rs`) writes real files the same way; only
dev-dependencies turn it on.

## Overlap with the agent's EXIF reader

`apps/desktop/src-tauri/src/tools/read/inspect/exif.rs` also reads EXIF, but shapes model-facing strings (`1/250 s`);
this crate answers typed values. They stay separate for now.
