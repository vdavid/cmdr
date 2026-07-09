# Compress feature plan

Status: ready to execute (survey verified against the live tree 2026-07-09 by three exploration passes; every path,
symbol, and line number below was confirmed on `main` at `a309164ca`). Worktree: `.claude/worktrees/compress-feature`,
branch `david/compress-feature` off local `main`.

## Goal

Add a **Compress** command that packs the file/folder under the cursor (or the selected files) into a NEW zip archive at
a target location, reusing the existing Transfer dialog as a THIRD mode alongside Copy and Move. It spans every rail
`file.copy` / F5 does: a native menu item, a command-palette entry, a customizable shortcut (default **⌥F5**), an MCP
tool, and the Transfer dialog. The default target folder is the OTHER pane's path; the default filename is derived from
the sources; the path field stays editable.

This mirrors `file.copy` end to end. `file.copy` is the closest template and is referenced throughout.

### Scope decisions (baked in; see Open questions for the reasoning and the ones still genuinely open)

- **Zip only.** tar/7z creation is future work.
- **Fixed compression.** Deflate at the `zip` crate's default level; no UI option in v1.
- **Local destination only in v1.** Compressing ONTO a remote (SMB/MTP) parent is deferred: the machinery composes (see
  M1 § remote), but the seed-through-Volume step plus SMB/MTP fixture testing is a separate cost. v1 refuses a remote
  destination with a typed error; remote-dest compress is future work.
- **Backend op type reuses `WriteOperationType::ArchiveEdit`.** Compress mechanically IS an archive edit (create a zip,
  copy sources into it), so it rides the existing `ArchiveEdit` op with no new enum variant. The "Compress" identity
  lives in the frontend (`TransferOperationType='compress'`, dialog title, command). See Open questions for the
  distinct-variant alternative.
- **Target-file conflict = overwrite/cancel**, via the dialog's existing dest-exists check, NOT the multi-file
  conflict-policy UI (which maps badly onto "create one new file"). See M3.

## Verified findings the plan relies on (do not re-derive)

All frontend paths are under `apps/desktop/src/`, backend under `apps/desktop/src-tauri/src/`.

1. **The Transfer dialog is one component parameterized by `operationType`.** `file-operations/transfer/TransferDialog.svelte`
   (976 lines). `TransferOperationType` is defined in `file-explorer/types.ts:441` as
   `'copy' | 'move' | 'delete' | 'trash' | 'archive_edit'` (NOT just copy/move — it's shared with the progress dialog).
   Compress adds `'compress'`. The dialog aliases the prop to mutable `activeOperationType` state; the segmented
   `.operation-toggle` (lines 481-494) is TWO hardcoded buttons that mutate `activeOperationType`. Helper modules in the
   same dir: `transfer-scan-state.svelte.ts` (deep byte scan), `transfer-conflict-check.svelte.ts` (top-level dest
   conflicts), `transfer-dialog-logic.ts` (`getPathValidationError`, `formatSpaceInfo`), `transfer-dialog-utils.ts`
   (`deriveTransferLabel`, `generateTitle`, `shouldShowHardlinkNote`, `toVolumeRelativePath`), and
   `transfer-dest-exists.svelte.ts` (`createTransferDestExistsCheck`). Copy/move-binary assumptions that a third mode
   must address: `confirmLabel` ternary (~235), `dialogTitle`=`generateTitle(...)` (~225), `isSameVolumeMove` (~180),
   `showHardlinkNote` (~226), `getPathValidationError(..., activeOperationType)` (~258).

2. **Openers** live in `file-explorer/pane/file-operation-commands.ts`: `openCopyDialog`/`openMoveDialog` (356-363) both
   call `openTransferDialog(op, ...)` (332-353), which guards the destination then calls `openUnifiedTransferDialog`
   (276-329). Sources: snapshot/selection/cursor via `buildTransferPropsFrom*` in `pane/transfer-operations.ts`. Default
   destination = the OTHER pane's path (`buildTransferContext`, 201-213: `destPath = access.getPanePath(other)`). Public
   wrappers are re-exported on `DualPaneExplorer.svelte` (openTransferDialog 774-780, openCopyDialog 783-785).

3. **Confirm routing.** `TransferDialog.onConfirm` is a 7-arg callback
   `(destination, volumeId, previewId, conflictResolution, operationType, scanInProgress, preKnownConflicts)` (56-67) →
   `DualPaneExplorer` `onTransferConfirm` (1364-1373) → `dialog-state.svelte.ts::handleTransferConfirm` (305) which
   builds `TransferProgressPropsData` and shows `TransferProgressDialog`. The actual copy/move dispatch is in
   `transfer/transfer-progress-state.svelte.ts::createTransferProgressState`: move → `moveBetweenVolumes(...)` or
   `moveFiles(...)`; copy → `copyBetweenVolumes(...)`. The typed IPC wrappers live in `tauri-commands/write-operations.ts`
   (`copyFiles`/`moveFiles` at 124/135, plus the cross-volume `copyBetweenVolumes`/`moveBetweenVolumes`). Signature:
   `copyFiles(sources: string[], destination: string, config?: WriteOperationConfig): Promise<WriteOperationStartResult>`.

4. **Shortcuts + palette + handler.** `commands/command-registry.ts` entry shape:
   `{ id, nameKey, scope, showInPalette, shortcuts }` — the field is **`shortcuts`** (array), `file.copy` → `['F5']`,
   `file.move` → `['F6']` (615-628). Shortcut strings are glyph-prefixed with no separators; `⌥F5` is the literal string
   `'⌥F5'` and is currently UNUSED (free). Command ids are a `COMMAND_IDS` `as const` tuple in `commands/command-ids.ts`
   (`'file.copy'` line 140). Handlers are a compile-error-enforced record in
   `routes/(main)/command-handlers/file-handlers.ts` (62-73: `'file.copy': ({explorerRef, dispatchArgs}) => void explorerRef?.openCopyDialog(...)`),
   keyed by `Exclude<CommandId, DispatchExemptId>` so a missing handler is a compile error.

5. **Menu.** `src-tauri/src/menu/mod.rs`: id constants (`FILE_COPY_ID = "file_copy"` line 109), forward map
   `menu_id_to_command` (arm at 358: `FILE_COPY_ID => Some(("file.copy", CommandScope::FileScoped))`), reverse map
   `command_id_to_menu_id` (arm at 441). Both are hand-synced exhaustive matches. `menu/macos.rs`:
   `MenuItem::with_id(app, FILE_COPY_ID, "Copy...", true, Some("F5"))` (87-88), inserted into the File `Submenu::with_items`
   (113-133). `menu/linux.rs`: same but `&`-mnemonic labels and NO F-key accelerators (`None::<&str>`, 45-46). Frontend
   `menuCommands` array in `shortcuts/shortcuts-store.ts` (228; `'file.copy'`/`'file.move'` at 255-256). Sync is guarded
   by `commands/rust-command-id-drift.test.ts` (block 2 asserts `new Set(menuCommands)` equals the parsed
   `command_id_to_menu_id` ids minus exclusions).

6. **MCP.** Single-source `mcp_tools!` registry in `src-tauri/src/mcp/tool_registry.rs` — the `"copy"` entry (286-305)
   declares `desc`, `schema` (autoConfirm + onConflict), `gate: TokenGate::IfAutoConfirm`, `run: app_params file_ops::execute_copy`.
   Adding an entry auto-registers the tool, its auth gate, and dispatch. Handler `execute_copy` in
   `mcp/executor/file_ops.rs:76`: pre-checks a target, parses/validates params, snapshots pane generation, emits
   `app.emit("mcp-copy", {autoConfirm, onConflict})`, then `wait_for_ack` — `AckSignal::GenerationAdvanced { from: pre_gen }`
   for auto-confirm, else `AckSignal::SoftDialogAppeared("transfer-confirmation")`. Frontend listener in
   `routes/(main)/mcp-listeners.ts` (410-415): `listenTauri('mcp-copy', ...)` → `dispatch(fileCopyCommand, {autoConfirm, onConflict})`.

7. **Backend — the real net-new work (verified, with corrections to the original survey):**
   - The archive-write routing lives in `commands/file_system/volume_copy.rs` (`copy_between_volumes` :60 /
     `move_between_volumes` :125), NOT in the legacy local-only `copy_files`/`move_files`. Routing keys on
     `dest_resolved.is_archive` from `get_volume_manager().resolve(...)`. A **non-existent** dest path resolves
     `is_archive = false` and takes the normal filesystem copy — never the archive driver (confirmed by
     `boundary.rs::confirm_archive_boundary`, which requires an existing FILE with valid zip magic; test
     `confirm_is_none_when_the_archive_component_does_not_exist` at `boundary.rs:325`).
   - The reusable, crate-public entry is **`route_archive_copy_into`** (`write_operations/archive_edit/copy_into.rs:62`,
     re-exported via `write_operations::route_archive_copy_into` → `file_system::route_archive_copy_into`). NOT the private
     `archive_copy_into_start`. Signature:
     ```rust
     pub(crate) async fn route_archive_copy_into(
         events: Arc<dyn OperationEventSink>,
         source_volume: Arc<dyn Volume>,
         source_paths: Vec<PathBuf>,   // relative to source volume root
         dest_full_path: PathBuf,      // e.g. /path/to/target.zip
         parent_volume_id: String,     // the DRIVE holding the .zip ("root" for local)
         conflict: ConflictResolution,
         progress_interval_ms: u64,
         is_move: bool,
     ) -> Result<WriteOperationStartResult, WriteOperationError>
     ```
     Crucially it splits the dest with the **string-only** `archive::archive_boundary_candidate` + an extension-only
     `ensure_zip_writable` guard — it does NOT re-confirm zip magic. So Compress can seed a valid empty zip at the target
     and call `route_archive_copy_into` directly, bypassing the `resolve`/`is_archive` gate; the valid-zip requirement is
     then enforced only inside the closure (`ArchiveIndex::parse` + the mutator's `ZipArchive::new`), which the seed
     satisfies. It already handles: local sources, remote (SMB/MTP) sources (pulled to scratch), a remote parent (via
     `pull_apply_upload_swap`), conflict policy, progress, and temp+rename durability.
   - The mutator (`file_system/volume/backends/archive/mutation/mutator.rs:183`) opens the archive with
     `ZipArchive::new(File::open(archive_path))` — a **0-byte file fails** here (`ZipError::InvalidArchive`). The write
     side uses the **`zip` crate v8.6** (temp+rename via `.cmdr-tmp-<uuid>` + `TempGuard`). A **minimal valid empty zip is
     a bare EOCD record, 22 bytes**: `PK\x05\x06` + 18 zero bytes. `bytes_start_with_zip_signature` (`boundary.rs:185`)
     accepts `PK\x05\x06`, so a seeded empty zip also passes any magic check.
   - **No existing "seed an empty zip at a target" code path exists** — this is the net-new backend surface. Everything
     downstream (cancel-safety via temp+rename / pull-upload-swap, delete-only-after-durable, WriteSettledGuard, lane
     admission) is inherited from `route_archive_copy_into`.

## Architecture at a glance

```
⌥F5 / palette / menu / MCP "compress"
        │
        ▼
file.compress command  ──►  explorerRef.openCompressDialog()
        │
        ▼
openTransferDialog('compress')  ──►  TransferDialog (compress mode: file-name path field, dest-exists check)
        │  onConfirm(destZipPath, volumeId, previewId, resolution, 'compress', ...)
        ▼
dialog-state.handleTransferConfirm  ──►  TransferProgressDialog  ──►  transfer-progress-state (compress branch)
        │  compressFiles(sourceVolumeId, sourcePaths, destVolumeId, destZipPath, config)
        ▼
[Rust] compress_files #[tauri::command]  ──►  compress_start(...)
        │  1. seed_empty_zip(destZipPath)   (22-byte EOCD, temp+rename; refuse remote dest in v1)
        │  2. route_archive_copy_into(...)  (existing machinery: scan, plan-in-closure, mutator, progress, durable rename)
        ▼
target.zip containing the sources
```

## Milestones

Sequential, single-agent, each ends green (`pnpm check --fast` minimum, plus the scoped lanes named per milestone) and
is committable. Backend logic is real red→green TDD.

### M1 — Backend: empty-zip seed + compress ops entry (Rust, TDD)

The riskiest, most net-new part. Build and test it in isolation before any wiring.

- **New module `write_operations/archive_edit/compress.rs`:**
  - `pub fn seed_empty_zip(path: &Path) -> Result<(), ...>` — writes the 22-byte bare-EOCD zip (`b"PK\x05\x06"` + `[0u8; 18]`)
    to a `.cmdr-tmp-<uuid>` sibling, fsyncs, atomically renames over `path`, fsyncs the parent dir. Mirror the mutator's
    temp+rename discipline (`mutator.rs:239-302`) so a crash never leaves a partial seed. (Alternatively
    `ZipWriter::new(f).finish()` from the `zip` crate produces the same bytes — pick whichever is cleaner; the literal
    22 bytes needs no crate call and is trivial to test.)
  - `pub async fn compress_start(events, source_volume, source_paths, dest_zip_full_path, parent_volume_id, conflict, progress_interval_ms) -> Result<WriteOperationStartResult, WriteOperationError>`
    — for a LOCAL parent: `seed_empty_zip(&dest_zip_full_path)`, then `route_archive_copy_into(events, source_volume, source_paths, dest_zip_full_path, parent_volume_id, conflict, progress_interval_ms, is_move=false)`.
    Reuses `WriteOperationType::ArchiveEdit` (no new variant). Re-export `compress_start` through
    `write_operations/mod.rs` and `file_system/mod.rs` (mirror the `route_archive_copy_into` re-exports at
    `write_operations/mod.rs:152` and `file_system/mod.rs:82`).
- **TDD (red first, see it fail for the right reason):**
  - `seed_empty_zip` unit test: after seeding a temp path, `zip::ZipArchive::new(File::open(path))` opens with `.len() == 0`,
    and `archive::boundary::bytes_start_with_zip_signature` accepts the first bytes. Write the assertion against a stub
    that writes 0 bytes first → RED (ZipArchive::new errors) → implement → green.
  - `compress_start` integration test (`tempfile` dir + a local `Volume`): compress two small files into `out.zip`, then
    reopen and assert both entries are present with correct contents. Prove the seed is load-bearing by temporarily
    skipping it → the in-closure `ZipArchive::new` must fail → restore → green (record in the commit body).
  - Cancel-safety test: inject a cancel between entries (via the existing `OperationIntent`/checkpoint seam the mutator
    honors) and assert the target is at worst the valid empty seed, never a torn file. If exercising real cancel is
    heavy here, defer the assertion to the E2E in M7 and note it.
- **Remote (documentation only in M1):** `compress_start` for a remote parent would need to seed THROUGH the parent
  `Volume` (because `pull_apply_upload_swap` pulls the existing remote `.zip` before editing — a local-only seed isn't
  visible). v1 does NOT implement this; the command layer (M2) refuses a remote destination. Leave a
  `// Remote dest: see compress-feature-plan.md § Open questions` marker where the local-only assumption lives.
- **Docs:** add a "Compress (seed + copy-into)" subsection to `write_operations/archive_edit/DETAILS.md` (or the nearest
  archive-edit `DETAILS.md`); one guardrail line to the colocated `CLAUDE.md` only if omitting the seed can silently
  break something (it can — "a Compress target MUST be seeded with a valid empty zip before `route_archive_copy_into`;
  a 0-byte file fails `ZipArchive::new`"). Watch the archive-area `CLAUDE.md` word budget (several sit at 595-599/600 —
  condense, don't append; see `docs/doc-system.md`).
- **Checks:** `pnpm check clippy rust-test -q` scoped, then `pnpm check --fast`.

### M2 — Backend: `compress_files` IPC command + bindings + FE wrapper

- **New `#[tauri::command] #[specta::specta] pub async fn compress_files(...)`** in
  `commands/file_system/volume_copy.rs` (model on `copy_between_volumes` :60). Args:
  `source_volume_id: String, source_paths: Vec<String>, dest_volume_id: String, dest_zip_path: String, config: Option<VolumeCopyConfig>`
  (match the `copy_between_volumes` arg/config shape — read it, don't guess). Body: resolve the source volume (reuse
  `resolve_source` :35) and the parent/dest volume; **if the dest volume is remote, return a typed error**
  (`WriteOperationError` variant — reuse an existing "unsupported" variant if one fits, otherwise add one; do NOT
  string-match, per `.claude/rules/no-string-matching.md`); build the `TauriEventSink`; call
  `compress_start(events, source_volume, source_paths, PathBuf::from(dest_zip_path), dest_volume_id, conflict, progress_interval_ms)`.
- **Register in BOTH IPC lists** (miss one and either bindings or the handler go stale): `ipc_collectors.rs` (near the
  `copy_between_volumes` block ~43) and `ipc.rs` (the second specta/handler list ~163).
- **Regenerate specta bindings** (the project's binding-gen step; the check lane will tell you if stale) so
  `commands.compressFiles` exists in `tauri-commands/`.
- **FE wrapper** `compressFiles(...)` in `tauri-commands/write-operations.ts`, mirroring the `copyBetweenVolumes`
  wrapper's signature and its `res.status === 'error'` → `throwIpcError` handling; returns `WriteOperationStartResult`.
- **TDD:** a Rust command-level test asserting a remote dest returns the typed refusal (matches!-style on the variant,
  not the message). The happy path is covered by M1's `compress_start` test + M7's E2E.
- **Checks:** `pnpm check clippy rust-test eslint-typecheck-ts -q` scoped, then `pnpm check --fast`.

### M3 — Frontend: Transfer dialog third mode + openers + confirm routing

The heaviest FE milestone. TransferDialog is pinned at **976 lines** in the file-length allowlist
(`scripts/check/checks/file-length-allowlist.json`), so **net growth must be zero or negative** — push new logic into
helpers and refactor the toggle rather than adding lines. Never bump the allowlist.

- **Type:** add `'compress'` to `TransferOperationType` (`file-explorer/types.ts:441`). Then audit EVERY consumer of the
  type for a `=== 'copy' ? … : …` / `=== 'move'` assumption and handle `'compress'`: in `TransferDialog.svelte`
  (`confirmLabel`, `isSameVolumeMove`, `showHardlinkNote`), and in `transfer-dialog-utils.ts` (`generateTitle`,
  `deriveTransferLabel`, `shouldShowHardlinkNote`) and `TransferProgressDialog` (progress title/label). `isSameVolumeMove`
  and the hardlink note are move-only, so `'compress'` reads false there — verify, don't assume.
- **Openers:** add `openCompressDialog(autoConfirm?, onConflict?)` → `openTransferDialog('compress', ...)` in
  `file-operation-commands.ts` (mirror 356-363), and re-export it on `DualPaneExplorer.svelte` (mirror 783-785).
- **The toggle (file-length neutral):** replace the two hardcoded `.operation-toggle` buttons (481-494) with an
  `{#each}` over `[{mode:'copy', key:'toggleCopy'}, {mode:'move', key:'toggleMove'}, {mode:'compress', key:'toggleCompress'}]`,
  preserving the exact class names (`toggle-option`, `class:active`) and click behavior. This adds the third mode while
  SHRINKING the markup. If any component/E2E test clicks the toggle by structure, keep the DOM shape compatible.
- **Compress-mode dialog behavior (the real semantic differences):**
  - The editable path field is a **file** path (`<destFolder>/<suggestedName>.zip`), not a folder. In copy/move it's the
    destination folder; in compress it defaults to a suggested zip filename inside the other pane's folder and stays
    editable.
  - **Suggested filename** (new pure helper, e.g. `transfer-compress-name.ts`, unit-tested): single source →
    `<basename>.zip`; multiple sources → `<source-directory-basename>.zip`, falling back to the FIRST selection's basename
    when the source directory is a volume root (empty basename). Cursor on an existing `.zip` gets no special treatment
    (still `<name>.zip`, targeting a new archive).
  - **Path validation:** extend `getPathValidationError` (`transfer-dialog-logic.ts`) with a compress branch — the path
    must end in `.zip`, the parent folder must exist, and (unlike copy/move) the leaf is a new file. Keep
    `toVolumeRelativePath` working for a file path.
  - **Conflict handling:** in compress mode, DO NOT run `transfer-conflict-check` (multi-file dest conflicts are
    meaningless for creating one new file). Instead use `createTransferDestExistsCheck` (`transfer-dest-exists.svelte.ts`)
    on the target zip path and surface a simple overwrite/cancel affordance (the user can also just edit the filename).
    The inner-conflict policy passed to the backend is fixed (a fresh empty zip has no existing entries; two sources in
    one folder can't collide) — pass a sensible constant.
  - **Scan preview:** keep `transfer-scan-state` (the source byte scan still gives size + progress).
- **Confirm routing:** in `transfer-progress-state.svelte.ts::createTransferProgressState`, add a `compress` branch that
  calls the M2 `compressFiles(sourceVolumeId, sourcePaths, destVolumeId, destZipPath, config)` wrapper. Simpler than
  copy's dual path — one command handles local and (future) remote sources.
- **English strings:** add the new `en` keys inline as you go (`toggleCompress`, the compress confirm label, the compress
  dialog title, any dest-exists/overwrite copy). Follow the style guide (sentence case, active voice, no "just/simple").
  Run `pnpm intl:keys`. Full translation is M6.
- **Tests:** unit-test `transfer-compress-name.ts` (all three filename cases) and the compress branch of
  `getPathValidationError`. A component/interaction test that switches the toggle to Compress and asserts the path field
  shows a `.zip` suggestion, if the existing dialog test harness supports it.
- **File-length:** run `pnpm check file-length` — TransferDialog should ratchet DOWN from 976 (the `{#each}` refactor
  shrinks it); the checker rewrites the allowlist. Confirm no new file crosses 800; if a helper would, split it, don't
  allowlist.
- **Checks:** `pnpm check eslint-typecheck-ts svelte vitest-desktop -q` scoped, then `pnpm check --fast`.

### M4 — Frontend: command id + registry + shortcut + palette + handler

- `commands/command-ids.ts`: add `'file.compress'` to the `COMMAND_IDS` tuple.
- `commands/command-registry.ts`: add the entry
  `{ id: 'file.compress', nameKey: 'commands.fileCompress.label', scope: 'Main window/File list', showInPalette: true, shortcuts: ['⌥F5'] }`
  (mirror 615-628). Add the `commands.fileCompress.label` `en` string.
- `CommandArgs['file.compress']` type (wherever `CommandArgs['file.copy']` is declared — carry `autoConfirm?`/`onConflict?`
  to match the copy args used by the handler and MCP).
- `routes/(main)/command-handlers/file-handlers.ts`: add the `'file.compress'` handler mirroring `'file.copy'` (62-67) →
  `void explorerRef?.openCompressDialog(args?.autoConfirm, args?.onConflict)`. (Omitting it is a compile error.)
- Update any palette/registry set assertions (`command-registry.test.ts`) to include the new palette-visible id.
- **Checks:** `pnpm check eslint-typecheck-ts vitest-desktop -q` scoped, then `pnpm check --fast`.

### M5 — Menu + MCP

- **Menu** (`file.compress` is a registered, accelerated File-menu item):
  - `src-tauri/src/menu/mod.rs`: add `pub const FILE_COMPRESS_ID: &str = "file_compress";` (near 109); add an arm to
    `menu_id_to_command` (`FILE_COMPRESS_ID => Some(("file.compress", CommandScope::FileScoped))`) and to
    `command_id_to_menu_id` (`"file.compress" => Some(FILE_COMPRESS_ID)`); extend the in-file id unit tests (~673, ~767).
  - `menu/macos.rs`: import `FILE_COMPRESS_ID`; build
    `MenuItem::with_id(app, FILE_COMPRESS_ID, "Compress...", true, Some("Alt+F5"))?` (verify the Tauri accelerator string
    for ⌥F5 against how existing ⌥ menu accelerators are written; `"Alt+F5"` is the Tauri convention). Insert into the
    File `Submenu::with_items` (113-133) next to Copy/Move. If you want the macOS SF-Symbol icon, the title must match the
    symbol map byte-for-byte (see `set_macos_menu_icons`) — otherwise leave it iconless.
  - `menu/linux.rs`: import the const; build `MenuItem::with_id(app, FILE_COMPRESS_ID, "&Compress...", true, None::<&str>)?`
    (GTK intercepts F-keys, so no accelerator — the shortcut dispatches via JS keydown). Insert into the File submenu
    (76-77).
  - `shortcuts/shortcuts-store.ts`: add `'file.compress'` to `menuCommands` (255-256).
  - `commands/rust-command-id-drift.test.ts` will now require the sync — it parses the Rust maps and asserts set-equality
    against `menuCommands`. Confirm it passes; update its expected-id lists only if it hard-codes them.
- **MCP:**
  - `src-tauri/src/mcp/tool_registry.rs`: add a `"compress" => { desc, schema, gate: TokenGate::IfAutoConfirm, run: app_params file_ops::execute_compress }`
    entry (mirror the `"copy"` block 286-305). Schema: same `autoConfirm` + `onConflict` shape (onConflict is near-moot
    for a fresh zip but keep the shape uniform, or drop it and document why). Description: "Compress selected files into a
    new zip in the other pane (opens confirmation dialog)."
  - `mcp/executor/file_ops.rs`: add `execute_compress` mirroring `execute_copy` (76) — pre-check a target, snapshot
    generation, `app.emit("mcp-compress", {autoConfirm, onConflict})`, then `wait_for_ack`. Compress opens the SAME
    transfer dialog, so reuse `AckSignal::SoftDialogAppeared("transfer-confirmation")` for the non-auto case and
    `AckSignal::GenerationAdvanced { from: pre_gen }` for auto-confirm.
  - `routes/(main)/mcp-listeners.ts`: add `const fileCompressCommand: CommandId = 'file.compress'` and a
    `listenTauri('mcp-compress', ...)` block mirroring `mcp-copy` (410-415) → `dispatch(fileCompressCommand, {autoConfirm, onConflict})`.
- **Checks:** `pnpm check clippy rust-test eslint-typecheck-ts vitest-desktop -q` scoped, then `pnpm check --fast`.

### M6 — i18n: translate all new strings to every locale

- Locales: **10 non-English** (`de`, `es`, `fr`, `hu`, `nl`, `pt`, `sv`, `vi`, `zh`) plus `en` — 11 dirs under
  `src/lib/intl/messages/`.
- Follow `docs/guides/i18n-translation.md` § "New feature → add strings and translate to ALL languages":
  1. Confirm every new `en` key has a `@key.description` that meets the bar (surface, trigger, placeholder meanings).
  2. `node apps/desktop/scripts/sync-locale-keys.js` to propagate keys as English skeletons with correct `sourceHash`.
  3. For each locale, read its `docs/i18n/<tag>/style.md`, **mine the reference pile** at the absolute main-clone path
     `~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/` (NOT the worktree — the pile isn't copied into worktrees; the
     worktree-relative path looks empty, which is the documented trap), and translate the new keys. The two-pane pair
     (Total Commander, Double Commander) is the lineage match for a "compress/pack" verb; check `<tag>/macOS/` for
     whether Apple localizes "Compress" (Finder's context-menu "Compress" is localized per-OS).
  4. Run `pnpm check desktop-i18n-parity desktop-i18n-icu desktop-i18n-plural desktop-i18n-stale desktop-i18n-coverage desktop-i18n-dont-translate`.
- New strings are few (toggle label, dialog title, confirm label, command label, maybe an overwrite prompt), so this is a
  bounded sweep. Record any `tentative` term in the per-language glossary; don't special-case Hungarian.
- **Checks:** the i18n lane above, then `pnpm check --fast`.

### M7 — E2E + docs sync + verify

- **Playwright E2E** (`apps/desktop/test/e2e-playwright/compress-basic.spec.ts`, modeled on `conflict-copy.spec.ts` and
  reusing `conflict-helpers.ts` where it fits — read `test/e2e-playwright/CLAUDE.md` first). One focused spec:
  - Set up fixtures (a couple of files/a folder in the left pane); focus the left pane, cursor on an item.
  - Trigger Compress via the command (⌥F5 or dispatch); assert the Transfer dialog opens in Compress mode with a `.zip`
    suggestion in the path field.
  - Confirm; assert a `<name>.zip` appears in the right pane and, by navigating INTO it (archive-as-folder browsing is
    shipped), that it contains the sources.
  - Cancel case: start a compress and cancel; assert NO partial `.zip` is left at the target (or at worst a valid empty
    seed — assert it's a valid archive, never torn). This is the data-safety assertion.
  - Keep it under the suite's per-test duration norm; if a mid-transfer cancel needs a payload big enough to catch, add a
    guardrail comment like the existing cancel-paste spec.
- **Docs sync** (colocated, per `.claude/rules/docs.md`): `transfer/` C+D (compress mode, the file-name path field, the
  dest-exists-vs-conflict decision), `write_operations/archive_edit` C+D (the seed + compress-start entry — the M1 doc),
  `commands/` C+D if the four-places note needs the compress example, and the MCP/menu C+D as touched. Single-source: the
  seed mechanism lives in ONE archive-edit `DETAILS.md`; everything else points to it.
- **Verify end to end:** drive the real app (per the `verify` skill / the docs on running) — compress a file, a folder,
  and a multi-selection; confirm the zip opens in Finder/Archive Utility and in Cmdr's own archive browser.
- **Full check:** `pnpm check` (plus `--include-slow` for the E2E lane). Acceptable pre-existing red: the `quick-xml`
  cargo-audit advisory (Renovate's to close).

## File-length allowlist risk spots

- **`TransferDialog.svelte` (pinned 976, over the 800 warn line).** The single biggest constraint. M3 must NET-SHRINK it
  via the `{#each}` toggle refactor and by putting compress logic in helpers (`transfer-compress-name.ts`,
  `transfer-dialog-logic.ts`, `transfer-dialog-utils.ts` — all well under 800). Run `pnpm check file-length` after M3 to
  ratchet the pin down. NEVER bump the allowlist for this file.
- **New helper files** (`transfer-compress-name.ts`, `compress.rs`) must stay under 800; if one would cross, split it.
- **Archive-area `CLAUDE.md` word budgets** (several at 595-599/600): M1's and M7's doc edits must condense, not append;
  a folder split beats another squeeze (`backends/archive/` is the standing split candidate). Warn-only — surface to
  David rather than silence.
- `transfer/volume_copy_tests.rs` is already an allowlisted growth warn (2461/2102) — don't pile compress tests into it;
  new backend tests live with `compress.rs` / the command module.

## Open questions (each with a recommendation)

1. **Distinct `WriteOperationType::Compress` vs reusing `ArchiveEdit`?** The plan reuses `ArchiveEdit` (compress
   mechanically IS create-zip + copy-into; minimal churn — a new variant touches every `match WriteOperationType` arm,
   specta bindings, analytics, and the FE catalog). The cost is analytics buckets compress with other archive edits.
   **Recommendation: reuse `ArchiveEdit` for v1.** If PostHog needs a distinct "compress" signal later, thread a boolean
   on the op descriptor rather than forking the enum. Flag for David only if he wants compress broken out in analytics
   from day one.
2. **Target-file conflict UX: dest-exists overwrite/cancel vs the full conflict-policy UI?** The multi-file policy
   (skip_all/overwrite_all/rename_all) is about files landing INTO a folder; compress creates ONE new file, so that UI is
   surprising. **Recommendation: reuse `createTransferDestExistsCheck` for a simple overwrite/cancel (plus the always-
   editable filename), and hide the conflict-policy control in compress mode.** Less code, less surprise.
3. **Remote destination in v1?** The machinery composes (seed through the parent `Volume`, then `route_archive_copy_into`
   handles the remote parent via `pull_apply_upload_swap`), but it adds a seed-through-Volume path plus SMB/MTP fixture
   testing. **Recommendation: v1 local-dest only with a typed refusal; remote-dest as a fast follow** (small code, the
   cost is fixtures/testing, not logic). If David wants remote-dest in v1, it's roughly +1 milestone (the seed-through-
   Volume path + a remote E2E).
4. **Multiple-selection default filename.** **Recommendation: `<source-directory-basename>.zip`** (predictable — all
   selected items share the source pane's folder), falling back to the first selection's basename when the source dir is
   a volume root. Single selection is always `<basename>.zip`. This matches the orthodox two-pane convention (Total
   Commander defaults the archive name to the parent folder).
5. **Does Cmdr localize native menu labels, or are they Rust string literals?** M5 verifies in `menu/macos.rs` — Copy is
   the literal `"Copy..."`, which suggests native labels are NOT run through the i18n catalog, so Compress follows suit
   with `"Compress..."` and M6 skips the menu label. **Recommendation: match whatever Copy/Move do today.** If native
   menu localization exists elsewhere, wire the key instead.

## Definition of done

Compress works from menu, palette, ⌥F5, and MCP; opens the Transfer dialog as a third mode with a suggested editable
`.zip` name; packs the cursor item or selection into a new local zip at the other pane's path; cancel leaves no torn
file; remote destination refuses with a typed error. Backend seed + compress-start are red→green tested; the command
refusal is tested; a focused E2E covers the happy path + cancel-safety. All new strings translated across 11 locales and
passing the i18n lane. TransferDialog net-shrinks (no allowlist bump). Colocated `CLAUDE.md`/`DETAILS.md` current and
single-sourced. Full `pnpm check` green (bar the sanctioned `quick-xml` red). Self-reviewed solid AND elegant.
