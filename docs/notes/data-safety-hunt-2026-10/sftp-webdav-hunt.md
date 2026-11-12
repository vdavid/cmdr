# Data-safety hunt 2026-10: SFTP and WebDAV (`sftp-webdav`)

Hunter report. Scope: `crates/cmdr-sftp/`, `crates/cmdr-webdav/`, and their desktop glue. Read-only; nothing was built
or run. Ranked by severity, then confidence.

### sftp-webdav-1. Deleting a symlink to a folder on an SFTP server empties the folder it points to

- **Severity:** critical
- **Confidence:** confirmed (traced end to end; not run)
- **Where:** `crates/cmdr-operations/src/delete/walker.rs:350`, `scan_volume_recursive`;
  `crates/cmdr-operations/src/delete/walker.rs:649`, `delete_volume_files_with_progress` (cached-scan arm);
  `crates/cmdr-sftp/src/volume/scan.rs:20`, `ScanSource::scan_stat`; `crates/cmdr-sftp/src/volume/query.rs:86`,
  `get_metadata_impl`; `crates/cmdr-fs/src/volume/scan_walk.rs:92`, `scan_tree_keeping`
- **Excerpt:**

```rust
// crates/cmdr-operations/src/delete/walker.rs:350
    let is_dir = match is_dir_hint {
        Some(v) => v,
        None => volume
            .is_directory(path)
            .await
            .map_err(|e| map_volume_error(&path.display().to_string(), PathRole::Source, e))?,
    };

    if is_dir {
```

```rust
// crates/cmdr-sftp/src/volume/query.rs:86
    pub(super) async fn get_metadata_impl(&self, path: &Path) -> Result<FileEntry, VolumeError> {
        let remote = self.to_remote_path(path)?;
        let session = self.clone_session().await?;
        let meta = session
            .sftp()
            .fs()
            .metadata(&remote)
```

- **Mechanism:** The user selects a symlink `www -> /var/www` in an SFTP pane (the listing shows it as a link: readdir
  attrs are `lstat`'s). Delete opens a scan preview: SFTP has no watch coverage, so `scan_preview/volume.rs:497` goes to
  `scan_for_copy_batch_with_boundary` → `scan_walk::scan_trees` → `scan_tree_keeping`, whose top-of-walk
  `source.scan_stat(path)` is SFTP's `get_metadata_impl`, an `SSH_FXP_STAT` that FOLLOWS the link. `top.is_directory` is
  true, so `top_level_is_directory: true` and the walk lists the target. On confirm, the walker's cached-scan arm
  (`walker.rs:649`) recurses with `Some(true)`; without a preview the `None` arm calls `Volume::is_directory`, whose
  trait default (`cmdr-fs/src/volume/mod.rs:319`) is `get_metadata`, the same following stat. `list_directory("www")`
  opens the target, every child is queued as `www/<child>`, and the delete loop removes them through the link: the
  target's files, then its now-empty subfolders. Last, `delete_impl("www")` sends `SSH_FXP_REMOVE`, which removes the
  link itself. The target folder survives, empty.
- **Trigger:** Delete (F8) on any symlink to a directory on an SFTP server. Common on servers: `~/www`, `~/data`,
  `current -> releases/2026-10-01`.
- **Guard ruled out:** The `Volume::entry_kind` doc (`cmdr-fs/src/volume/mod.rs:326`) says to ask it, ❌ never
  `is_directory`, before recursively deleting; SFTP implements `entry_kind` with `lstat` (`query.rs:110`), but the
  delete walker never calls it, and `ScanSource::scan_stat` has no link-aware variant. The oracle branch in
  `scan_preview/volume.rs:371` checks `!entry.is_symlink`, but SFTP never reaches it (no watch coverage). Children are
  safe on OpenSSH only because its readdir uses `lstat`; a server that answers readdir with `stat` attrs would follow
  nested links too. SFTP has no trash, so nothing goes to a recoverable place. The preview's file count is the only hint
  the user gets.
- **Impact:** Permanent loss of everything under the link's target, outside the selected tree. Not recoverable.
- **Suggested fix:** Make the delete walker (and `scan_tree_keeping`'s top stat, or a delete-specific scan) ask
  `entry_kind` and treat `Symlink` as a leaf; give `ScanSource` an `lstat`-shaped stat for the top of a delete walk.
  Possibly a duplicate of a `hunt-delete` finding, since the hole sits in the walker; it's SFTP's following `stat` that
  makes it bite.

### sftp-webdav-2. On WebDAV, "Overwrite" in the rename conflict dialog deletes a whole folder tree

- **Severity:** high
- **Confidence:** confirmed (traced end to end; server behavior per RFC 4918 § 9.9.3, not run)
- **Where:** `crates/cmdr-webdav/src/volume/mutation.rs:149`, `rename_impl`;
  `apps/desktop/src/lib/file-explorer/pane/rename-flow.svelte.ts:697`, `handleConflictResolve`;
  `apps/desktop/src/lib/file-explorer/rename/RenameConflictDialog.svelte:30`;
  `crates/cmdr-operations/src/rename.rs:405`, `rename_target`
- **Excerpt:**

```rust
// crates/cmdr-webdav/src/volume/mutation.rs:149
    pub(super) async fn rename_impl(&self, from: &Path, to: &Path, force: bool) -> Result<(), VolumeError> {
        let remote_from = self.to_remote_path(from)?;
        let remote_to = self.to_remote_path(to)?;
        let client = self.clone_client().await?;
        let is_collection = self.stat(&client, &remote_from).await?.is_collection;
        debug!("WebdavVolume::rename: {remote_from} -> {remote_to} (force={force})");
        let request = client
            .request(method("MOVE"), client.url_for(&remote_from, is_collection))
            .header("Destination", client.url_for(&remote_to, is_collection).as_str())
            .header("Overwrite", if force { "T" } else { "F" })
            .timeout(MUTATION_BUDGET);
```

- **Mechanism:** F2 on `notes.txt` (or a folder), type `Photos`, which an existing FOLDER holds. The validity check
  (`rename/validity.rs:309`) reports a conflict with `is_directory: true` and size 0 (`mapping.rs:16` gives a collection
  no size). The dialog titled "A file with this name already exists" shows the folder as a 0-byte "file"; WebDAV has no
  trash, so `enterResolution` is `overwrite-delete` and Enter fires it. `performRename(…, force: true)` →
  `rename_target` accepts any taken name when forced (no type check) → `rename_impl` sends `MOVE` with `Overwrite: T`.
  RFC 4918 § 9.9.3 obliges the server to `DELETE` the destination with `Depth: infinity` first; Apache `mod_dav` and
  sabre/dav both do (this crate's `DETAILS.md` § "Which side a test lives on": "`MOVE` overwrites by default, `DELETE`
  on a collection is recursive").
- **Trigger:** A rename whose new name matches an existing folder, then Enter (or "Overwrite and delete old file") in
  the conflict dialog.
- **Guard ruled out:** Local refuses this shape in the kernel (`rename(2)` gives `EISDIR` / `ENOTEMPTY`), and SFTP's
  forced rename only clears an EMPTY directory (`remove_dir`, `cmdr-sftp/src/volume/mutation.rs:337`). WebDAV has no
  equivalent: nothing checks the destination's type, and the dialog never says "folder" or how much is in it. The
  transfer engine's own type-crossing consent (`conflict::resolution_for_clash`) isn't on the rename path.
- **Impact:** The destination folder and everything under it, permanently on Apache `mod_dav`; Nextcloud's trash bin may
  hold it if the app is enabled.
- **Suggested fix:** In `rename_impl`, refuse a forced MOVE onto a destination that is a collection (one `Depth: 0`
  PROPFIND, after the user already confirmed) unless the source is an empty-folder replacement; and have the rename
  conflict path refuse type-crossing or word the dialog for a folder.

### sftp-webdav-3. Deleting a WebDAV folder can take a file that arrived a moment earlier with it, unreported

- **Severity:** medium
- **Confidence:** plausible (the window is traced; the race was not reproduced)
- **Where:** `crates/cmdr-webdav/src/volume/mutation.rs:121`, `delete_impl`
- **Excerpt:**

```rust
// crates/cmdr-webdav/src/volume/mutation.rs:121
    pub(super) async fn delete_impl(&self, path: &Path) -> Result<(), VolumeError> {
        let remote = self.to_remote_path(path)?;
        let client = self.clone_client().await?;
        let listing = self.propfind(&client, &remote, false, Depth::One).await?;
        // ❗ Judged by COUNT, ❌ never by which entry comes first: RFC 4918
        // promises no order, and a child read as "the resource" would send a
        // recursive DELETE at a folder with things in it. A `Depth: 1` on a
        // file or an empty collection answers exactly one response.
        if listing.len() > 1 {
            return Err(VolumeError::IoError {
                message: format!(
                    "{remote} still holds {}",
                    pluralize_with((listing.len() - 1) as u64, "entry", "entries")
                ),
                raw_os_error: Some(ENOTEMPTY),
            });
        }
        let is_collection = listing.first().is_some_and(|p| p.is_collection);
        let request = client
            .request(Method::DELETE, client.url_for(&remote, is_collection))
```

- **Mechanism:** The trait's `delete` is "one node"; the protocol's `DELETE` on a collection is recursive. The guard is
  a check-then-act: `PROPFIND Depth: 1` proves the folder empty, then a separate `DELETE` removes whatever is there by
  then. The delete walker empties each folder deepest-first and then calls this per folder, and a same-volume move's
  source sweep relies on the same refusal to keep a skipped child. Anything written into the folder between the two
  requests (one round trip, 20–150 ms on Nextcloud) is deleted with it, and the delete reports success.
- **Trigger:** Deleting or moving a folder on a Nextcloud that the Nextcloud desktop client, a phone's auto-upload, or a
  second user is writing into at the same time.
- **Guard ruled out:** No lock (deliberately, `DETAILS.md` § "Not supported"), no `If` precondition on the `DELETE`, and
  the count can't see the future. SFTP and local are immune because their rmdir refuses a non-empty folder atomically.
- **Impact:** A file the user never saw is permanently deleted (recoverable only from Nextcloud's trash, if enabled).
- **Suggested fix:** Send the `DELETE` with `If: (["<etag>"])` taken from the PROPFIND's self entry (Nextcloud's
  collection ETag changes when a child changes, so the server refuses atomically with 412); keep the count as the
  fallback for servers that give collections no ETag.

### sftp-webdav-4. A host key revoked in `~/.ssh/known_hosts` the standard way still connects, or prompts as new

- **Severity:** medium
- **Confidence:** confirmed (code traced; the `@revoked *` line shape is the one `sshd(8)` documents)
- **Where:** `crates/cmdr-sftp/src/known_hosts.rs:185`, `matches_pattern`; `crates/cmdr-sftp/src/known_hosts.rs:124`,
  `lookup`; `crates/cmdr-sftp/src/trust.rs:75`, `decide`
- **Excerpt:**

```rust
// crates/cmdr-sftp/src/known_hosts.rs:185
fn matches_pattern(address: &str, pattern: &str) -> bool {
    if let Some(hashed) = pattern.strip_prefix("|1|") {
        return matches_hashed(address, hashed);
    }
    // ❌ No glob expansion. OpenSSH allows `*` and `?` in a pattern, and a
    // fallback reader that got the expansion subtly wrong would either trust a
    // host it shouldn't or alarm on one it should. An exact match is always safe:
    // the worst it costs is one extra approval.
    if pattern.contains('*') || pattern.contains('?') {
        return false;
    }
    address == pattern
}
```

- **Mechanism:** A revocation is written `@revoked * ssh-ed25519 AAAA…` (the `sshd(8)` example; a revoked key is bad on
  every host). `lookup` only considers entries whose patterns match the address, and `*` never matches here, so the
  revoked line is invisible. `decide` then falls through: a key Cmdr's own store approved earlier reads `Trusted` and
  connects silently; a first contact reads `Unknown` and gets the one-click approval the module says `Revoked` must
  never reach.
- **Trigger:** The user (or their admin's config management) revokes a compromised host key with a wildcard line, then
  an attacker holding that key intercepts the connection.
- **Guard ruled out:** "An exact match is always safe" is true for positive entries and false for `@revoked`, where a
  missed match is the unsafe direction. No test covers a wildcard revocation (`known_hosts_test.rs` uses exact hosts).
- **Impact:** Passwords and file contents go to a man in the middle using a key the user explicitly revoked.
- **Suggested fix:** Match `@revoked` entries by blob alone, whatever their host patterns, since revocation names a key,
  not a host; keep the no-glob rule for plain entries.

### sftp-webdav-5. A mistyped password in the sign-in sheet replaces the working one in the Keychain

- **Severity:** low
- **Confidence:** confirmed
- **Where:** `crates/cmdr-webdav/src/volume/reconnect.rs:138`, `do_reconnect_with_credentials`;
  `crates/cmdr-sftp/src/volume/reconnect.rs:165`, same name
- **Excerpt:**

```rust
// crates/cmdr-webdav/src/volume/reconnect.rs:138
    pub(super) async fn do_reconnect_with_credentials(
        &self,
        username: String,
        password: String,
    ) -> Result<(), VolumeError> {
        if username != self.params().username {
            return Err(VolumeError::NotSupported);
        }
        self.refresh_remembered_secret(&username, &password).await;
        self.auth_attempt_spent.store(false, Ordering::Relaxed);
        self.rebuild(Some(password))
            .await
            .map_err(|stalled| self.report(stalled))
    }
```

- **Mechanism:** The typed secret is written over the remembered one BEFORE the probe or dial checks it. A typo lands in
  the store, the dial is refused, and the saved secret that worked is gone. SFTP does the same on the password,
  keyboard-interactive, and passphrase rungs.
- **Trigger:** A volume lands in `NeedsCredentials` while the stored secret is still right (a server that briefly
  answered 401: LDAP down, a temporary lockout), and the user mistypes at the sheet.
- **Guard ruled out:** "Refreshes, never seeds" limits it to remembered entries, which are exactly the ones worth
  keeping.
- **Impact:** A saved credential is lost; recoverable only if the user knows the password.
- **Suggested fix:** Refresh the store only after the dial succeeds with the typed secret.

### sftp-webdav-6. An SFTP upload of a file that shrank mid-copy lands truncated and reports success

- **Severity:** low
- **Confidence:** confirmed (code); impact reasoning plausible
- **Where:** `crates/cmdr-sftp/src/volume/writes.rs:254`, `upload`; `crates/cmdr-sftp/src/volume/writes.rs:313`, `pump`
- **Excerpt:**

```rust
// crates/cmdr-sftp/src/volume/writes.rs:254
        match self
            .pump(&mut stream, writer.clone(), &remote, length, depth, on_progress)
            .await
        {
            Ok(written) => {
                // ❗ The close is part of the write, not tidying after it.
                if let Err(e) = writer.close().await {
                    self.remove_partial(&session, &remote).await;
                    return Err(e);
                }
                let report = keep_source_attrs(&session, &remote, stream.modified_at(), landed_mode).await;
```

- **Mechanism:** `StreamLength::Known(n)` is defined as "the stream must contain exactly this many bytes"
  (`cmdr-fs/src/volume/types.rs:92`), but `pump` returns whatever the source yielded and nothing compares it with `n`.
  The engine doesn't either (`stream_pipe.rs:439` takes `written.bytes` as is). A source that ends early (a local file
  truncated or rewritten during the copy; the SFTP read side itself "reports what exists", `streams.rs:386`) lands a
  short file under the user's name. WebDAV (`writes.rs:458`) and S3 refuse the same input.
- **Trigger:** Copying a file that another app is rewriting (a log, a database, a document being saved) to SFTP.
- **Guard ruled out:** A MOVE is protected by the source-stamp check before its source delete; a COPY has nothing.
- **Impact:** A torn copy reported as complete; the source still exists.
- **Suggested fix:** Fail the write with the WebDAV-style size mismatch when `length` is `Known(n)` and `written != n`.

### sftp-webdav-7. A WebDAV download with no `Content-Length` declares its length as zero

- **Severity:** low
- **Confidence:** plausible (the server shape wasn't observed on the fixtures)
- **Where:** `crates/cmdr-webdav/src/volume/streams.rs:126`, `open_read_stream_impl`
- **Excerpt:**

```rust
// crates/cmdr-webdav/src/volume/streams.rs:126
        let (total, skip) = match response.status() {
            StatusCode::PARTIAL_CONTENT => (
                content_range_total(&response).unwrap_or(response.content_length().unwrap_or(0) + offset),
                0,
            ),
            _ => (response.content_length().unwrap_or(0), offset),
        };
```

- **Mechanism:** A `200` with chunked transfer encoding (or HTTP/2 with no length header) has no `content_length()`, so
  the stream answers `StreamLength::Known(0)`, a promise of exactly zero bytes, and `open_read_stream_with_hint`'s
  PROPFIND size is ignored (the trait default drops the hint). `stream_pipe.rs:229` hands that to the destination. A
  WebDAV destination then always fails the copy (size mismatch); an MTP destination declares a 0-byte object to the
  phone in `SendObjectInfo` and streams the real bytes after it; progress and free-space checks see zero. SMB is safe
  (`fits_one_compound_write` refuses size 0, so it stages).
- **Trigger:** Copying off a WebDAV server that streams downloads without a length (for example `rclone serve webdav`
  over a backend with unknown sizes).
- **Guard ruled out:** `fits_one_compound_write`'s `size > 0` kills the SMB variant; nothing covers the MTP one.
- **Impact:** Failed copies, wrong progress; on MTP a possibly wrong-size object, depending on `mtp-rs`.
- **Suggested fix:** Answer `StreamLength::Unknown` (or the PROPFIND size the caller hinted) when a 200 carries no
  length, rather than inventing `Known(0)`.

### sftp-webdav-8. A WebDAV server saved as `http://` sends the password in clear text on every request

- **Severity:** low
- **Confidence:** confirmed
- **Where:** `apps/desktop/src-tauri/src/network/webdav_volume_wiring.rs:299`, `credential_service`;
  `apps/desktop/src-tauri/src/commands/servers.rs:787`, `webdav_params`; `crates/cmdr-webdav/src/transport.rs:217`,
  `request`
- **Excerpt:**

```rust
// crates/cmdr-webdav/src/transport.rs:217
    pub(crate) fn request(&self, method: Method, url: Url) -> RequestBuilder {
        self.http
            .request(method, url)
            .basic_auth(&self.username, Some(&self.password))
    }
```

- **Mechanism:** Both `http` and `https` are accepted, and Basic auth rides every request, so on `http://` the password
  crosses the network in base64 on each listing and each upload. Nothing in the sign-in UI (`lib/servers/`) warns.
- **Trigger:** Adding a NAS by `http://testnas.local:5005`, a common default on Synology and QNAP.
- **Guard ruled out:** None found: no warning, no refusal, no HTTPS-upgrade attempt.
- **Impact:** The account password is readable to anyone on the network path (shared Wi-Fi), which gives them the data.
- **Suggested fix:** Warn in the sign-in sheet for `http://` (or require an explicit opt-in) before the first secret is
  sent.

## Near misses

- WebDAV `CreateOrReplace` DELETEs its target on any failure: from every production caller the target is the engine's
  `.cmdr-tmp-*` (`write_is_single_shot` false; `native_drag/fulfillment.rs` writes locally).
- WebDAV `copy_within` sends `Overwrite: T`: the destination is always the caller's staging temp.
- SFTP forced rename without `posix-rename` deletes then renames: callers are a user-confirmed overwrite, the archive
  swap (new bytes stay under the temp and the delete-then-rename fallback retries), and `commit_atomic_replace`, which
  `supports_atomic_replace_rename` (false here) keeps away.
- SFTP forceless rename: the claim-then-`posix-rename` dance is sound; a directory source falls back to an `MKDIR`
  claim, and every failure removes the placeholder.
- SFTP forced rename onto a non-empty directory: `remove_dir` refuses it, so nothing is lost.
- Listed names with `/`, `..`, or NUL: `RemoteRoot::to_app_path` drops off-root results and the transfer engine joins
  names only through `ChildName`. A malicious server could alias an in-root sibling, but it holds the data anyway.
- SMB single-shot (unstaged) write on a WebDAV source reporting `Known(0)`: killed by `fits_one_compound_write`'s
  `size > 0`.
- Nextcloud chunked upload: target untouched until the assembling MOVE; every failure DELETEs the uploads collection and
  only the temp target.
- Over-long and short PUT sources: both refused by the read-ahead `fetched` count (`writes.rs:437`).
- Host-key pinning: `algorithms_to_pin` + `approve_host_key` re-probing the presented key hold; the probe never
  authenticates.
- URL userinfo (`https://user:pass@…`): the frontend address parser keeps only the account (`address-parser.ts:201`), so
  no password reaches the saved-server JSON or the probe's debug log.
- `reap_stale_transfer_temps` is age-gated on mtime, and SFTP/WebDAV stamp the SOURCE's old date onto a temp before its
  landing rename: the window (SETSTAT or PUT end to landing MOVE) is one round trip, and a reaped temp fails the landing
  loudly with the source intact.
- WebDAV `create_file` relies on `If-None-Match: *`, which some servers may ignore; only the New File command uses it,
  after its own conflict check.

## Covered

Read in full:

- `crates/cmdr-sftp/CLAUDE.md`, `crates/cmdr-sftp/DETAILS.md` (§§ write window through renaming, error policy)
- `crates/cmdr-sftp/src/volume/mutation.rs`, `writes.rs`, `streams.rs`, `copy.rs`, `query.rs`, `mapping.rs`, `paths.rs`,
  `scan.rs`, `volume_impl.rs`
- `crates/cmdr-sftp/src/trust.rs`, `crates/cmdr-sftp/src/known_hosts.rs`
- `crates/cmdr-webdav/CLAUDE.md`, `crates/cmdr-webdav/DETAILS.md`
- `crates/cmdr-webdav/src/volume/mutation.rs`, `writes.rs`, `chunked_upload.rs`, `copy.rs`, `paths.rs`, `query.rs`,
  `mod.rs`, `streams.rs`, `scan.rs`
- `crates/cmdr-webdav/src/chunking.rs`, `params.rs`, `urls.rs`
- `apps/desktop/src-tauri/src/network/webdav_volume_wiring.rs`, `apps/desktop/src-tauri/src/network/sftp_host_keys.rs`

Read in part (the data-touching paths):

- `crates/cmdr-sftp/src/transport.rs` (lines 270–340, 660–1036: the handshake, trust handler, auth ladder, pin)
- `crates/cmdr-sftp/src/volume/reconnect.rs` (130–260), `crates/cmdr-webdav/src/volume/reconnect.rs` (120–300)
- `crates/cmdr-webdav/src/transport.rs` (100–406)
- Out of slice, followed for mechanisms: `cmdr-operations/src/delete/walker.rs`, `scan_preview/volume.rs`,
  `cmdr-fs/src/volume/scan_walk.rs`, `remote_paths.rs`, `child_name.rs`, `cmdr-operations/src/rename.rs`,
  `rename/validity.rs`, `transfer/staged_write.rs`, `transfer/volume/stream_pipe.rs`, `transfer/volume/cleanup.rs`,
  `archive_edit/remote.rs`, the rename conflict dialog and flow, `cmdr-smb/src/volume/streams.rs` (compound path),
  `cmdr-mtp/src/volume/volume_impl.rs` (`write_from_stream`).

## Not covered

- `crates/cmdr-sftp/src/auth.rs`, `errors.rs`, `extensions.rs`, `params.rs`, `volume/state.rs`, `volume/mod.rs` (only
  `approve_host_key`), `volume/testing.rs`, the rest of `transport.rs` (dial, engine lifecycle).
- `crates/cmdr-webdav/src/errors.rs`, `propfind.rs`, `volume/mapping.rs` (one line), `volume/state.rs`,
  `volume/volume_impl.rs` (grepped only), `volume/testing.rs`, `transport.rs` lines 1–100.
- `apps/desktop/src-tauri/src/network/sftp_volume_wiring.rs`, `sftp_known_servers.rs`, `webdav_known_servers.rs`,
  `live_server_edit.rs`, `one_shot_credentials.rs`, `commands/servers.rs` beyond `webdav_params`.
