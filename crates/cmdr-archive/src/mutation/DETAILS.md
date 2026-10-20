# Zip mutation — details

Pull-tier docs for the zip write side. Must-know invariants live in `CLAUDE.md`. Read this before any non-trivial work
here: editing, planning, reorganizing, or advising.

The write side is `Volume`-free and manager-free like the [read core](../read/DETAILS.md). Full driver wiring (event
sink, pause gate, cancel intent via the `MutationHooks` seam, and the remote pull→edit→upload→swap flow):
`crates/cmdr-operations/src/archive_edit/DETAILS.md`.

## Temp+rename safe-overwrite (`mutator.rs`)

`ArchiveMutator::apply(archive_path, changeset, hooks)` applies a batched `Changeset` (`add` / `mkdir` / `delete` /
`rename`; a mkfile is an add of empty bytes) by building the FULL new archive into a same-directory sibling temp
(`foo.zip.cmdr-tmp-<uuid>`), then atomically renaming it over the original.

- **Temp+rename, not append-in-place.** `zip`'s `ZipWriter::new_append` overwrites the old central directory, so a
  cancel mid-edit corrupts the archive (verified: truncating before the new EOCD yields "Could not find EOCD"; the
  original does NOT survive). Building fresh into a temp and renaming is Cmdr's mandated safe-overwrite and the only
  shape where cancel is genuinely free (abandon the temp, no rollback ledger). The original is byte-for-byte untouched
  until the final rename; a cancel or crash at any earlier point leaves it fully readable.
- **Entry names: plan by what the reader shows, write unchanged names as stored.** A zip from a Japanese, Chinese, or
  Russian Windows tool stores Shift-JIS, GBK, or CP866 names without the UTF-8 flag. rc-zip (the reader) detects
  Shift-JIS; the `zip` crate reads every such name as CP437. So `apply` matches deletes and renames against rc-zip's
  name for each entry (`shown_names`, keyed by local header offset, which both crates agree on even behind an SFX stub
  and unlike the ordinal, which `zip` shifts when it collapses a duplicate name), falling back to `zip`'s `name()` for
  an entry rc-zip didn't list. Entries sharing an offset (a hostile zip can point two central records at one local
  header) pair in directory order (`ByHeaderOffset`); keyed by offset alone, both took the later name, so deleting one
  took its twin and deleting the other took nothing. An entry the edit doesn't rename goes through `raw_copy_file`,
  which writes its `name_raw()` under its own UTF-8 flag; a renamed one gets its new name (the user's text, or a renamed
  folder's prefix plus the shown rest) as flagged UTF-8 via `raw_copy_file_rename`. Upstream 9.0.1's `raw_copy_file`
  re-encoded the CP437 reading as flagged UTF-8, garbling every non-ASCII name on any edit (#514); the fix is the
  vendored patch in `vendor/zip/CMDR-PATCH.md`. Pinned by `mutator_raw_names_test.rs`.
- **Retained entries copy verbatim** via `raw_copy_file` / `raw_copy_file_rename` (no decompress/recompress), extra
  fields included. ❗ That's why `zip` carries its `unreserved` feature: without it, the writer refuses any entry with
  an extra-field ID it reserves but doesn't model, and `ditto` (Finder's Compress) writes one, Info-ZIP's `0x5855`, so
  every rename or delete inside a Finder-made zip would fail (verified on `zip` 9.0.1, `write.rs` `start_entry`,
  2026-10-12; pinned by `mutator_apple_double_test.rs`, which edits a `ditto` fixture). Only added files stream-compress
  (chunked, never whole-buffered — the add chunk is also the pause/cancel granularity mid-file). An added file carries
  its SOURCE's modification time into the entry (`add_entry_options`); a new folder and an in-memory file carry the
  write time.
- **Every new entry's time is written twice, by `with_entry_mtime`** (the one place, which the host's fresh-ZIP producer
  calls too, so the two writers can't drift apart again):
  - The header's MS-DOS date and time, decomposed in LOCAL time. The field carries no zone and every tool (`unzip -l`,
    Finder, Explorer) shows it as wall-clock time, so a UTC decompose reads hours off: a file modified at 09:44 in UTC+2
    listed as 07:44. 2-second granularity, 1980–2107; outside that it stays at the format's zero.
  - The Info-ZIP extended-timestamp extra field (`UT`, `0x5455`), the exact UTC second, in the local header and the
    central directory. Tools prefer it over the DOS field, and so does `rc-zip`, our reader, which is what keeps an
    mtime stable across write-then-reparse in any zone. Written when the time fits the field's signed 32 bits
    (1970–2038).
  - ❗ `SimpleFileOptions::default()` dates an entry 1980-01-01: `zip`'s "now" default needs its `time` feature, which
    this workspace doesn't enable (verified on `zip` 8.6.0, `DateTime::default_for_write`, 2026-09-30). ❌ Never hand
    `start_file` / `add_directory` bare default options.
  - The read side takes a DOS-only entry as the writer's wall clock in the reader's zone (`../read/DETAILS.md` § "Entry
    times"). Entries an older Cmdr wrote (UTC DOS, no `UT`) therefore list off by the reader's offset, an accepted
    shift.
- **Compression level applies to ADDED entries only.** `add_entry_options` sets the deflate level from
  `Changeset::compression_level` on each newly added entry; retained entries are raw-copied and keep their original
  compression untouched. `None` (the default) means the `zip` crate default, level 6 — so an unset level is byte-stable
  with pre-setting behavior. The level is set once per edit, sourced from the user's `behavior.archiveCompressionLevel`
  setting and threaded in via the operation config (see `crates/cmdr-operations/src/archive_edit/DETAILS.md`). The level
  is **clamped to 1..=9** inside `add_entry_options`: `zip`'s `get_compressor` returns
  `UnsupportedArchive("Unsupported compression level")` for an out-of-range Deflated level at the FIRST entry write (it
  does NOT clamp), which would fail the whole edit — so a bad config value or an MCP `set_setting` with a wild number is
  clamped, never propagated. Pinned by `add_entry_options_applies_and_clamps_the_compression_level` (unit) and the
  `compress_tests.rs` level cases (level 9 beats level 1; default None equals explicit 6; out-of-range clamps instead of
  failing).
- **Metadata preservation (the archive FILE, not the entries).** A rewrite yields a fresh inode, so the original
  `.zip`'s metadata goes through the shared helper (`crates/cmdr-fs/DETAILS.md` § "`metadata`: what a copy keeps, and
  when it goes on"): the temp's flush first (`cmdr_fs::durable::sync_file_data`), then xattrs, mode, and birth + mtime
  in one call on its fd, then the ACL and user flags on the archive after the swap. **The mtime is the EDIT time, not
  the original's** (birth stays the original's): the contents changed, so "Date Modified" must say so, the way a
  folder's does when you add to it, and a size + mtime backup check (rsync's default) mustn't skip a changed zip whose
  size happens to match. It's still set explicitly in that one call, so a share's close can't restamp it. Flush first
  and through that helper: on a Finder-mounted SMB share, std's `sync_all` (`F_FULLFSYNC`) answers `ENOTSUP`, which
  failed every edit there, and dates set before the flush lose the mtime to the server's close (verified against the
  Docker Samba guest share via `mount_smbfs`, macOS 27.0, 2026-10-12; no automated cell, since no lane mounts a share
  for this crate). xattrs include Finder tags, quarantine, and `com.apple.FinderInfo` VERBATIM (a faithful copy that
  keeps the custom-icon flag, so the `tags.rs` FinderInfo gotcha doesn't apply). Best-effort: metadata loss never fails
  a data-safe edit. `copyfile(COPYFILE_STAT)` never set birth on a non-clone copy, so this is what keeps it
  (`mutator_test.rs::archive_file_metadata`, which also pins the edit-time mtime).
- **Entries' xattrs ride AppleDouble sidecars, the shape Finder's Compress writes** (`__MACOSX/<parent>/._<name>`, codec
  in `../apple_double.rs`). An added local file's sidecar is read off the fd the add already holds
  (`local_file_sidecar`), a sourced mkdir's off its local folder (`Changeset::mkdir_sources`, `local_folder_sidecar`),
  and either is written right after its item, dated like it. ONLY when the source carries an xattr worth keeping
  (David's call, so an ordinary zip stays clean for Windows users): provenance, quarantine, the last-used date
  (`com.apple.lastuseddate#PS`, on most opened files), and an all-zero FinderInfo aren't, and cost one name-only
  `flistxattr`. A written sidecar carries the quarantine and the last-used date too. macOS only. The host's fresh-ZIP
  producer calls the same two functions.
- **Every delete and rename takes the item's sidecars along** (`plan.rs`: `with_sidecar_deletes` /
  `with_sidecar_renames`): its own and, for a folder, every one under `__MACOSX/<folder>/`. So F8, F2, and a move OUT of
  a zip leave no orphan. Every add and sourced mkdir also drops any sidecar already at its name: the new item writes its
  own or has none, and two entries at one name make the `zip` writer fail the whole edit. Sidecars never count toward
  `entries_changed`.
- **A Locked or `deny delete` archive can't be edited.** The swap replaces the original, which means removing it, and
  `uchg` refuses that (`EPERM`) as does the ACL (`EACCES`). Both guard the very file the edit would remove, so the edit
  refuses and the original stays byte-identical. The temp never wears the ACL, so its cleanup can remove it.
- **External replacement of the `.zip` between planning and the final rename is last-writer-wins.** The changeset is
  planned against the archive as parsed at plan time; if an outside process rewrites the same `.zip` before the
  mutator's atomic rename, that outside write is simply overwritten by our temp (and vice-versa — a rename that lands
  after ours wins). This is acceptable for the single-user local model Cmdr targets; there's no cross-process lock. It's
  stated here so a future multi-writer scenario revisits it deliberately rather than assuming a guard exists.
- **Encrypted entries: a kept one raw-copies and still decrypts with its password (#592).** An edit never needs an
  encrypted entry's plaintext: a kept entry, renamed or not, is a raw copy, and an overwrite deletes it. So the copy
  only has to keep what a reader decrypts with, and the vendored `zip` patch (`vendor/zip/CMDR-PATCH.md`) makes it:
  - ZipCrypto: the 12-byte encryption header sits in the stored bytes. The copy keeps bit 0 (encrypted) and bit 3 (data
    descriptor), and writes a descriptor after the bytes when bit 3 is set. Bit 3 is load-bearing: it picks the
    password's check byte, the DOS time's high byte under it (Info-ZIP's `zip -e` shape) and the CRC's without. Upstream
    9.0.1 dropped both bits, so the entry read back as ciphertext-as-plaintext, and keeping bit 0 alone would fail every
    Info-ZIP zip's password check. The CRC and DOS time ride the options as before.
  - WinZip AES (AE-1/AE-2): the salt, verifier, and MAC sit in the stored bytes, and `ZipFile::options()` already
    carries the mode and vendor version, from which the writer writes the `0x9901` extra field and method 99. The patch
    stops the copy also writing the stored field a second time.
  - Neither scheme binds the entry's name, so a rename is safe too.
  - Still refused (`EncryptedEntryRetained`, before any temp exists; `plan.rs` `carries_its_encryption`): PKWARE Strong
    Encryption (bit 6) and masked headers (bit 13), whose state the `zip` crate doesn't model, and a bit-3 ZipCrypto
    entry whose DOS time the crate couldn't parse (it keeps none to write back, so the check byte would break). Deleting
    such an entry works. The host maps the refusal to `WriteOperationError::ArchiveEncryptedEntry`.
- **The rewrite keeps the archive's own shape, not just its entries** (#542, pinned by `mutator_archive_shape_test.rs`):
  - The archive comment, set raw from `src.comment()`. A fresh `ZipWriter` writes none.
  - Every byte ahead of the first local header (a self-extractor stub, or whatever a tool prepended): `plan.rs`'s
    `leading_bytes_len` measures it, `copy_prefix` writes it to the temp first, and the writer starts at its end. The
    writer records offsets from the start of the FILE, so the result has absolute offsets, the shape `zip -A` gives a
    self-extractor, which `unzip`, rc-zip, and the `zip` crate all read.
  - A symlinked archive: `apply` canonicalizes the path first, so the temp is minted beside the real file and the rename
    replaces it. Renaming over the link swapped the link for a regular file holding the edit, and left the real archive
    unedited.
- **Decision: refuse an archive with a duplicate entry name (`DuplicateEntryName`).** The `zip` crate keys entries by
  raw name in an `IndexMap` and folds a duplicate into one slot (the later one's data), and the rewrite copies through
  it, so the earlier copy would silently vanish. `plan.rs`'s `refuse_a_folded_duplicate` refuses, before any temp
  exists, when the `zip` crate's own record count (`central_directory_len`, a vendored-patch addition) exceeds its entry
  count. Gotcha: comparing against rc-zip's record count instead missed every fold in a zip rc-zip can't parse (the
  planner then has no names), so the count must come from the `zip` crate itself. rc-zip's names only say WHICH name;
  without them the refusal carries `name: None` and the FE words it without one. Preserving both would need the `zip`
  crate's reader AND writer to stop keying by name, a structural patch (upstream `zip-rs/zip2` #841 asks for the reader
  half), so refusal is the answer until upstream moves. The host maps it to
  `WriteOperationError::ArchiveDuplicateEntryName`.
- **Decision: refuse an edit that would write an entry under a FILE entry (`EntryUnderFile`).** Every reader (Cmdr's
  index, `unzip`) drops an entry whose parent is a file, so `notes/a.txt` beside a file `notes` sits in the zip unseen
  and unextractable. The host's copy-into planner asks about that clash first; `plan.rs`'s
  `refuse_an_entry_under_a_file` is the backstop, run before any temp exists. It checks only what the edit WRITES (adds,
  mkdirs, renamed kept entries), both ways round (a new entry under a file, a new file over kept entries), so a zip that
  already holds such an entry stays editable. A kept entry is a folder by its trailing slash or its recorded mode (Unix
  `S_IFDIR` or the DOS directory bit), as the reader decides, so a slashless folder entry never reads as a file. The
  host maps it to `WriteOperationError::ArchiveEntryUnderFile`.
- **Leftover policy — no startup reaper.** A leftover `foo.zip.cmdr-tmp-*` is always an ABANDONED build (the original is
  intact), so it's harmless. `apply` reaps siblings of the target at the START of the next edit of that archive (before
  building its own fresh-uuid temp), which is sufficient; a cancel/error removes its own temp immediately via an RAII
  guard. It matches by `cmdr_fs::staging::temp_sibling_prefix`, the mint's own prefix, so a temp whose name was
  shortened to fit is matched too. This LOCAL reap deletes every matching sibling unconditionally (edits of one archive
  serialize on the parent lane, so a local leftover is always abandoned). A REMOTE archive edit leaves its leftover on
  the SHARE (a crash after upload, before swap), not on the local scratch copy this `apply` reaps; that leftover is
  reaped by an age-gated mirror of this reap on the REMOTE parent at the next edit of the same archive — see
  `crates/cmdr-operations/src/archive_edit/remote.rs` (`reap_remote_temps`) and
  `crates/cmdr-operations/src/archive_edit/DETAILS.md` § "Remote edit" for why the remote side needs the age gate.
- **Deletes/renames reshape the retained set.** A delete drops a file or a whole subtree (component-wise match, so `foo`
  never catches `foobar`); a rename rewrites a subtree prefix. Both are computed per original entry in one pass
  (`plan.rs`, `is_deleted` then `renamed_name`); deletes win over renames.
- **`Changeset::deletes_spare_hidden` (a move out) matches an item by where the reader's TREE shows it**, not by its raw
  name. `read::shown_entry_paths` lists every entry's header offset, with its tree path when the tree shows it: a file
  node's own entry, and an explicit folder entry at a folder node. An entry missing there (quarantined for `..` or
  depth, a file under a file, a file at a folder's path) never extracted, so it stays; one whose raw name differs from
  its tree path (`dir\a.txt`, `./dir/a.txt`) is matched by the tree path it extracted from. Sidecars aren't in the tree,
  so they still go by raw name with their item. If the tree can't be built the edit stops (`OpenOriginal`), since
  there's no telling what extracted. Without the flag (F8, an overwrite) a delete takes the raw subtree, hidden entries
  included.

## Testing

`mutator_test.rs` is the red-first TDD anchor for every data-safety property: round-trips
(add/delete/rename/mkdir/mkfile verified via our reader AND external `unzip -t`),
cancel-midway-leaves-the-original-intact-with-no-temp, a leftover temp reaped on the next edit, the merge invariant (a
delete keeps siblings' raw compressed bytes byte-identical), metadata (mode/mtime/xattr) survival,
pause-parks-mid-add-then-completes-on-resume. `mutator_encrypted_test.rs` pins encrypted entries for ZipCrypto (both
check-byte shapes, plus a real `zip -e` archive where the tool exists), AES-128, and AES-256: every stored fact
byte-identical, decrypting through the `zip` crate and `unzip -P`, and the refusals. `mutator_raw_names_test.rs` pins
the entry-name rules: a Shift-JIS name and an unflagged UTF-8 one keep their bytes and flags through an edit, and a
delete or rename finds a Shift-JIS entry by its shown name. `mutator_archive_shape_test.rs` pins the archive-level
shape: the comment, a self-extractor prefix, a symlinked archive, the duplicate-name refusal, two names sharing one
local header each deleting alone, and the entry-under-a-file refusal.
`crates/cmdr-operations/src/archive_edit/routing_tests.rs::ensure_zip_writable_allows_zip_and_refuses_read_only_formats`
pins the non-zip-refuses-typed matrix.
