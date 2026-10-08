# S3 hostile live cells, 2026-10-02

A campaign to break `cmdr-s3` against the seven real provider accounts, at the `S3Volume` / `cmdr_fs` `Volume` level the
app drives. The cells: `crates/cmdr-s3/src/volume/live_hostile_test.rs` (names, folders, sizes, scale, share links,
metadata) and `live_hostile_failure_test.rs` (cancels, crash recovery, races), sharing `live_hostile_support.rs`. Run
them with `apps/desktop/test/s3-servers/live.sh <providers> live_hostile[_<cell>]`. Each cell collects every miss per
provider and fails once at the end. The distilled, current facts live in `crates/cmdr-s3/DETAILS.md`; this note keeps
the evidence.

## Setup

- **Accounts**: the `live-env.sh` set (R2, Hetzner `nbg1`, GCS, Spaces `fra1`, AWS `eu-north-1`, B2 `eu-central-003`,
  Wasabi `eu-central-1`). Two sibling agents ran live suites on the same accounts and uplink at the same time, so
  throughput numbers are for orientation only.
- **Snapshot runs**: siblings edited the shared worktree mid-run (twice it didn't compile), so later runs went from a
  `git archive HEAD` snapshot with its own `CARGO_TARGET_DIR` (`target/s3-probe/snap-run.sh`, not committed).
- **Cost**: per provider about 1.3 GB uploaded and 1.1 GB read back (the ~1 GiB object), plus ~~1,300 small requests.
  Egress is billed on AWS (~~$0.10) and GCS (~$0.13); the rest charge none at this size. Wasabi's big object was 300
  MiB, so its 90-day minimum bills well under a cent. Every cell cleans its own `cmdr-live/<run>/` prefix and the sweep
  found no stale leftovers.

## Bugs found and fixed

1. **A Cancel after a PUT's last piece destroyed the file it was overwriting** (`1b5c755f9`). Live on R2: the cancel
   cell's "one PUT, at its last piece, over an original" case reported `Cancelled` while the original was gone. Cause:
   `put_streamed`'s 200 ms tick still honoured a Cancel after the last piece was released, dropped the request while the
   server was committing, and the cut-off cleanup then found our own `x-amz-meta-cmdr-write` token on the published
   object and deleted it, so neither the original nor the new bytes survived (the fake reproduced exactly that). R2
   answers slowly enough to hit it; the other six answered within the tick and reported `Ok`. Fix: once the last piece
   is released, the PUT waits for its answer and reports the file it finished; an empty body checks Cancel once, before
   the request. Pinned by `late_cancel_test.rs` over a fake S3 that commits and then answers slowly (red: object gone,
   `Cancelled`).
2. **A server copy whose source shrank mid-copy surfaced as a raw `InvalidRange`** (`90f619970`). Live on Spaces, which
   ignores the copy's ETag pin: a 16 MiB source replaced by 1 MiB between parts failed the next `UploadPartCopy` with
   `416 InvalidRange`, an opaque `IoError`. Nothing was published and the upload was aborted either way; now it's
   `SourceChanged` like the pin's 412. Unit-tested in `server_copy_test.rs`.
3. **An object with a 1,024-byte key couldn't be deleted, renamed, or tallied on B2** (`865680063`). Every "what's under
   this?" listing asks for `<key>/`, one byte past S3's ceiling; B2 refuses that prefix with `400 InvalidRequest` where
   everyone else answers an empty page. Now such a prefix is answered as empty without a request
   (`listing::can_hold_keys`). Pinned by `long_key_test.rs` over `fake_s3.rs`, which refuses an overlong prefix like B2;
   verified live on B2 afterwards.

## Outcome per cell

"All six" means R2, Hetzner, GCS, Spaces, AWS, and Wasabi; B2 is listed where it differs. Every cell passed on every
provider after the fixes; `live_hostile_sizes` on B2 passed on the 2026-10-03 rerun (§ "B2's daily cap").

- **`live_hostile_names_round_trip`**: 28 names (NFC and NFD `café`, emoji with a ZWJ sequence, Hebrew, Arabic, double
  spaces, leading space, trailing space, trailing dot, `...`, `100% sure`, a literal `%2e%2e` and `%20`, `+`, `#`, `?`,
  `&` `=` `;`, `\`, quotes, `<>|`, `~!*()$,@:`, `[]{}^` and a backtick, a tab, a line break, Japanese, `UPPER.TXT`
  beside `upper.txt`) plus a key of exactly 1,024 bytes, each through write, list, stat, read, rename, and delete. All
  passed everywhere, with two provider refusals: GCS refuses CR/LF in a key (raw PUT `400 InvalidObjectName`; through
  the volume a bodyless 400 from the no-overwrite HEAD, shown as `IoError "HTTP 400"`), and B2 refuses any control
  character, the tab too (`400 InvalidRequest`). Nothing lands in either case. R2 lists the NFD name as its NFC twin, as
  documented. A `..` segment resolves lexically inside the bucket (`RemoteRoot`), one climbing out of the bucket is
  refused, and a 1,025-byte key is refused by every server.
- **`live_hostile_folders`**: passed everywhere. A file beside its folder lists as `twin (file)`, reads as the file,
  renames alone, and the folder's delete refuses (`ENOTEMPTY`) without touching the file; a marker folder survives
  emptying; a folder under a file is `NotADirectory`; New File on a folder's name is `AlreadyExists`; a markerless
  20-level tree lists at both ends, tallies one file and 21 folders, is `CopyThenDelete` for the engine and
  `NotSupported` for `rename`, and vanishes with its last key.
- **`live_hostile_sizes`**: 0, 1, 5 MiB ± 1, 10 MiB ± 1, and 15 MiB + 1 (a one-byte last part) at a 5 MiB part floor,
  the 0 / floor + 1 / 2 × floor ones also of unknown length, each read back byte for byte and stat-sized; ranged reads
  across a part edge, from the last byte, and from the end (empty); a download released mid-way then read whole; then 1
  GiB (300 MiB on Wasabi, 200 MiB on B2) in production 64 MiB parts, generated and verified without holding it. Passed
  on all seven (B2 on 2026-10-03). Big-object rates (shared link, skewed): R2 20.5 up / 33.4 down MiB/s, Spaces 15.8 /
  46.3, AWS 25.7 / 48.1, GCS 15.4 / 23.2, Wasabi 22.9 / 48.8, Hetzner 4.6 / 31.1 (Hetzner's upload overlapped a
  sibling's big run), B2 22.7 / 32.4 (alone on the link).
- **`live_hostile_cancel_uploads`**: a Cancel before the first part, mid second part, and right before the completion of
  a 16 MiB multipart upload, and mid-body and at the last piece of a 2 MiB PUT, each to a free key and over an original.
  All cancelled cases left nothing published, the original byte for byte, no upload on the server, and no open ledger
  record, on all seven (after fix 1). The "at its last piece" case is too late by design: every provider now reports the
  finished file, read back whole.
- **`live_hostile_cancel_server_copies`**: a Cancel before the upload exists, before the first part, between parts,
  before the completion, and on a one-request copy. Passed on all seven: nothing published, the source intact, no upload
  left. GCS copies any size in one atomic `CopyObject`, so only the checkpoint before it can stop it; a later Cancel
  finishes the copy whole, as expected.
- **`live_hostile_crash_recovery`**: a child process (this test binary re-run) starts a paced 40 MiB upload under a
  state dir and is SIGKILLed after its record and a part landed; the server lists the upload (all seven, R2 included).
  The next connect with that state dir sweeps it: the upload is gone (a part sent to it is refused), nothing is
  published, the record is closed. An upload in flight in THIS process survives a second place's connect and an explicit
  sweep (0 aborted) and completes intact. Passed on all seven. FINDING (all seven): a second LIVE process sharing the
  record has its running upload aborted by the first one's sweep ("the server ended the upload before it completed"),
  because "in flight" is a per-process registry. Cmdr's `instance_lock.rs` allows one process per data dir, so the app
  can't reach this; documented in `DETAILS.md` § "Unfinished uploads".
- **`live_hostile_races`**: two `CreateNew`s on one key at once (one PUT, three rounds; in parts, one round): always
  exactly one winner whose bytes are stored, the loser `AlreadyExists`, on the header providers (R2, Hetzner, Spaces,
  AWS) every time. On check-then-write providers the documented blind window showed in one round of three: GCS, B2, and
  Wasabi each once reported BOTH writers `Ok`, the stored bytes being one writer's, so the other's write was silently
  replaced. A source replaced mid-copy is `SourceChanged` with nothing published on all seven (GCS: n/a, one atomic
  copy). Deleting a folder's listed files while another writer adds one keeps the new file; a file written while its
  folder is deleted survives; a rename onto a taken name is `AlreadyExists` with both kept, and forced replaces it.
- **`live_hostile_scale`**: 1,150 files plus five subfolders of ten. Passed on all seven: two pages, 1,150 file rows and
  five folder rows with the right sizes, `tally_subtree` 1,200 files and six folders, capped at 1,000 incomplete,
  `rename_work` `CopyThenDelete`, and `delete_files` of all 1,200 in two batches with no failure, the folder then
  `NotFound`. Seeding 1,200 PUTs at 16 in flight took 3 s (Spaces, Hetzner) to 34 s (B2).
- **`live_hostile_share_links`**: links to `hash# & plus+ 100% é 🦀 ?.txt`, `a b/c=d;e.txt`, and a Hebrew name fetch
  unsigned with the bytes intact everywhere. A three-second link works fresh and fails expired: 403 on most, ❗ 400 on
  GCS and 401 on B2. A deleted object's link answers 404 everywhere.
- **`live_hostile_metadata`**: passed everywhere. A stat shows the source's mtime (`x-amz-meta-mtime`) for an empty, a
  small, and a multipart object; a listing shows the upload time; a rename, a one-request copy, and a copy in parts keep
  the mtime; a copy of a dateless object takes the source's upload time; a dateless overwrite (one PUT, and in parts)
  drops the old mtime.

## Provider quirks seen

- **Refused names**: GCS, CR and LF (`InvalidObjectName`); B2, any control character (`InvalidRequest`).
- **An overlong listing prefix**: B2 `400 InvalidRequest`; everyone else an empty page.
- **An expired presigned link**: 403, except GCS 400 and B2 401.
- **A second abort of an aborted upload**: R2 and GCS answer a success, so a second abort proves nothing; probe with a
  part instead (`upload_gone`).
- **R2's `ListMultipartUploads` listed the killed child's upload** under its prefix, in every run of the crash cell.
  `DETAILS.md` § "Verified providers" says R2 lists nothing, not even a fresh upload; that may hold only for an unscoped
  listing.
- **A Cancel landing just after a PUT's body**: R2 answered the PUT later than the 200 ms progress tick, the others
  sooner; that difference is what exposed fix 1.

## B2's daily cap

Late in the campaign David's B2 account hit its free daily download cap (bandwidth and Class B transactions). From then
on every B2 read answered 403: a HEAD bodyless, a GET with `<Code>AccessDenied</Code>` (the message names the download
or Class B cap; per `live-providers` and `live-engine`, who read the body). LIST, PUT, and `DeleteObjects` kept working.
It resets around 00:00 UTC, or when the cap is raised on B2's Caps & Alerts page.

- **What the user sees in Cmdr**: `VolumeError::PermissionDenied` on any stat, read, or copy, and on a `CreateNew` write
  too, since check-then-write starts with a HEAD (that's how every `live_hostile_sizes` write failed). The UI words that
  as a permissions problem, which sends the user to their keys rather than their B2 caps. The code can't tell them
  apart: the HEAD has no body, and the GET's `AccessDenied` code is the same as a real refusal; only the message text
  differs, and classifying by message is off the table.
- **Rerun on 2026-10-03, within the caps**: `live.sh b2 live_hostile_sizes` passed every check (the size ladder, no
  upload left, the three ranged reads, the released download, and the big object). The ~1 GiB read-back alone would use
  B2's 1 GB daily download cap, so B2's big object is now 200 MiB (three 64 MiB parts and an 8 MiB tail; `big_size`,
  `c8ce363e4`). Cost, estimated from the cell's shape: about 80 Class B requests (a no-overwrite HEAD, a verifying HEAD,
  a stat, and a GET per size) and about 300 MiB downloaded (80 MiB of ladder read-backs, ~20 MiB of ranged and re-reads,
  200 MiB big). The whole B2 rerun's totals: `live-verification-2026-10.md` § "Outcome: B2 rerun within its free caps".

## Open questions for the lead

1. **B2's cap reads as "permission denied"** (§ "B2's daily cap"). Worth a sentence in the user-facing wording of an S3
   `PermissionDenied` ("or the provider's usage cap was reached")? Judgment call; not changed.
2. **A typed refusal for names a provider won't store?** GCS and B2 refusals surface as `IoError "HTTP 400"` /
   `"InvalidRequest (HTTP 400)"`. Mapping `InvalidObjectName` (and B2's control-character case) to
   `VolumeError::InvalidName` would let the UI say "this provider doesn't allow that name". Not done: `InvalidRequest`
   is B2's catch-all, so it can't be mapped by code alone, and GCS's refusal arrives as a bodyless 400 to the
   no-overwrite HEAD. Judgment call.
3. **The check-then-write blind window is not rare under a real race**: one round in three on GCS, B2, and Wasabi. It's
   the accepted residual risk (`DETAILS.md` § "No-overwrite writes"), and two Cmdr writers racing for one key is
   unusual, but if it matters, a post-write HEAD that also compares our write token (not just the ETag) wouldn't close
   it either; only versioning would.
4. **R2's multipart listing**: worth re-verifying the "lists nothing" claim (§ "Provider quirks seen") before relying on
   it in `abort_upload`'s R2 special case.

## Follow-up: the lead's decisions, and what landed

1. **B2's cap**: confirmed as B2's free daily Class B cap. `live_hostile_sizes` on B2 passed on the 2026-10-03 rerun (§
   "B2's daily cap").
2. **Typed refusal for names** (`745c7777e`, `1b325ae93`): `cmdr_fs` already had `VolumeError::InvalidName` (SMB's
   reserved names), so it's reused. The volume can't map B2 by code (`InvalidRequest` is its catch-all) or GCS's
   no-overwrite HEAD (bodyless 400), so the refusal happens up front: `ProviderProfile::refused_key_chars` (GCS CR/LF,
   B2 every control character) and `writes.rs::refuse_unstorable` answer `InvalidName` before any request on every write
   path (upload, New File, New Folder, a rename's destination, a server copy's destination). GCS's `InvalidObjectName`
   maps to `InvalidName` by code as a backstop. Verified live on GCS:
   `InvalidName("…: this provider doesn't store U+000A in a name")`, nothing sent (a raw PUT of the same key answers
   `400 InvalidObjectName`). B2's live recheck waits for the cap; `refused_name_test.rs` covers it against the fake.
   What the user sees: the copy dialog's existing invalid-name message ("{path} has a name the destination can't
   store.", no Retry), its suggestion now naming tabs and line breaks; Rename, New File, and New Folder say "“{name}”
   can't be stored here. Pick a different name." English only: `i18n-coverage` lists `errors.volume.invalidNameNamed`,
   and `i18n-stale` the changed suggestion, until the translator agent runs.
3. **Blind window**: accepted for B2 and Wasabi; GCS's native precondition is `live-providers`' to look at.
4. **R2's multipart listing**: `DETAILS.md` corrected; it lists an unfinished upload under a prefix. Nothing relied on
   the old claim: `abort_upload` and the sweep never special-cased R2, so their confirming listing simply works there.
5. **Lost answers** (`bb6e85267`, the lead's priority ask): a PUT, buffered PUT, or `CompleteMultipartUpload` whose
   server published before the connection dropped now reports the file. The cut-off cleanup removes only our object at a
   SHORT size. Still open: a server-side multipart copy whose completion answer is lost reports a failure though the
   copy landed (nothing is deleted).

## Follow-up 2: S3's permission wording, and the last lost-answer case

- **"Permission denied" names both causes on S3** (`16ee8a3b8`): a refusal on an S3 app path (its own `s3://` scheme,
  `server_of_path`, never a message) is `ListingErrorReason::ObjectStoreRefused` in a pane and
  `PermissionRefusal::ObjectStoreAccount` in a copy, move, or delete. The words: the access key may lack permission, or
  the provider may have paused the account (a usage cap, a billing issue); "Check the access key's permissions, or your
  provider's console for a usage cap or a billing notice." Other backends keep their wording. English only.
- **A server-side copy in parts whose completion answer is lost** (`9a3cd6b6b`): such a copy now carries its own write
  token in its creation metadata, so one HEAD proves the destination is ours and whole and the copy reports success.
  Size plus the ETag's part count alone couldn't prove that (an earlier identical copy matches both). Otherwise it still
  fails and aborts its upload, which is safe because a move keeps its source.
- **Checks**: clippy and the `cmdr-s3` tests are green, as are the touched app Rust tests and the FE suites;
  `bindings.ts` matches a regen. `pnpm check --fast` fails only `i18n-coverage` on the five new English keys
  (`errors.volume.invalidNameNamed`, `errors.listing.objectStoreRefused.*`,
  `errors.write.permissionDenied.suggestion.objectStoreAccount`), which wait for the translator agent.
