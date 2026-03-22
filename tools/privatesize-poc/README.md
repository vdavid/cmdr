# privatesize-poc

Proof-of-concept tool that compares three file size metrics for a directory tree on macOS:

- **Logical** (`meta.len()`): File content size
- **Physical** (`st_blocks * 512`): Allocated disk blocks (what our index currently uses)
- **Private size** (`ATTR_CMNEXT_PRIVATESIZE`): Bytes that would be freed if the file were deleted — correctly accounts
  for APFS clone sharing

## Usage

```sh
cd tools/privatesize-poc
cargo build --release
./target/release/privatesize-poc ~/some/directory
```

## Why this exists

APFS clones share disk extents via copy-on-write. `st_blocks * 512` reports the full allocation per clone, overcounting
shared data. macOS provides `ATTR_CMNEXT_PRIVATESIZE` via `getattrlist()` which reports only the unique (non-shared)
bytes — the actual reclaimable space.

This PoC was used to validate the API works and measure the delta. See the conversation in which it was created for the
full investigation.

## Key finding: calling convention

`ATTR_CMNEXT_PRIVATESIZE` goes in the `forkattr` field of `attrlist` (not `commonattr`), and requires
`FSOPT_ATTR_CMN_EXTENDED` in the options. Getting this wrong returns garbage or errors with no helpful diagnostics.

```c
attrlist.forkattr = ATTR_CMNEXT_PRIVATESIZE;  // NOT commonattr!
options = FSOPT_NOFOLLOW | FSOPT_ATTR_CMN_EXTENDED;
```

## Status

Read-only PoC. Not integrated into Cmdr's indexer yet. A future step would be to replace `st_blocks * 512` with
privatesize in the scanner for clone-aware "on disk" sizes.
