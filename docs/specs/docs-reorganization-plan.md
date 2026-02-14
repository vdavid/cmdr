# Docs reorganization plan

## Context

The `docs/` folder is largely write-only. 59 specs, 22 feature docs, and 7 notes contain valuable intent and design
decisions, but they're disconnected from the code, get stale, and agents don't read them. The goal is to reorganize docs
so they stay alive and agents discover them automatically.

Key insight from research: **Claude Code auto-discovers `CLAUDE.md` files** in subdirectories when reading files there.
No need to reference them from AGENTS.md. They load on-demand, keeping context lean.

## Strategy

**Colocated `CLAUDE.md` files** replace `docs/features/` — one per feature directory, auto-discovered by Claude Code.
**`docs/architecture.md`** replaces the central overview need — a thin index pointing agents to the right directories.
**Intent preservation**: before deleting any doc, subagents distill all intent/decisions/gotchas into the new CLAUDE.md
files. Conflicts between spec intent and current implementation get flagged for user resolution.

## Milestone 1: Extract intent from existing docs (subagents, parallel)

6 subagent clusters, each reading related docs + current code, producing a report file in `/tmp/cmdr-docs-reorg/`.

Each report contains:
- **Distilled intent**: design decisions, gotchas, non-obvious behavior not deducible from code alone
- **Conflicts**: where documented intent differs from current implementation (for user to resolve)
- **Draft CLAUDE.md content**: ready to write after conflict resolution

### Cluster assignments

**Cluster A — File loading, virtual scrolling, performance**
- Docs: `specs/load-files-super-fast-*`, `specs/move-file-cache-to-backend-*`, `specs/volume-fs-abstraction-*`,
  `specs/add-virtual-scrolling-*`, `specs/non-blocking-folder-loading*`, `specs/volume-streaming-redesign`,
  `specs/listing-operations-split-plan`, `features/file-loading`, `notes/dir-load-bench-findings`,
  `notes/debugging-startup-time`
- Code: `src/lib/file-explorer/views/`, `src-tauri/src/file_system/listing/`
- Target CLAUDE.md: `src/lib/file-explorer/views/CLAUDE.md`, `src-tauri/src/file_system/listing/CLAUDE.md`

**Cluster B — Drag and drop**
- Docs: `specs/drag-and-drop-target-*` (5 files), `specs/drag-image-detection-plan`,
  `specs/dynamic-drag-image-swap-plan`, `features/drag-and-drop`
- Code: `src/lib/file-explorer/drag/`, `src-tauri/src/drag_image_detection.rs`, `src-tauri/src/drag_image_swap.rs`
- Target CLAUDE.md: `src/lib/file-explorer/drag/CLAUDE.md`

**Cluster C — Write operations (copy, move, delete) + rename + new folder**
- Docs: `specs/write-operations*` (3), `specs/copy-dialog-plan`, `specs/add-move*` (2), `specs/move-feature-plan`,
  `specs/rename-*` (3), `features/write-actions`, `features/write-operations-tauri-api`, `features/new-folder`,
  `features/rename`
- Code: `src/lib/file-operations/`, `src/lib/file-explorer/rename/`, `src-tauri/src/file_system/write_operations/`,
  `src-tauri/src/commands/rename.rs`
- Target CLAUDE.md: `src/lib/file-operations/CLAUDE.md`, `src/lib/file-explorer/rename/CLAUDE.md`

**Cluster D — Settings, shortcuts, MCP server**
- Docs: `specs/settings*` (2), `specs/shortcut-settings*` (2), `specs/mcp-server-*` (2),
  `specs/agent-centric-mcp-plan`, `features/settings`, `features/mcp-server`
- Code: `src/lib/settings/`, `src/lib/shortcuts/`, `src-tauri/src/mcp/`
- Target CLAUDE.md: `src/lib/settings/CLAUDE.md`, `src/lib/shortcuts/CLAUDE.md`, `src-tauri/src/mcp/CLAUDE.md`

**Cluster E — Network/SMB + MTP**
- Docs: `features/network-smb/` (7 files), `specs/mdns-sd-migration-plan`, `specs/mtp*` (5), `features/llm`,
  `specs/llm*` (2)
- Code: `src/lib/file-explorer/network/`, `src-tauri/src/network/`, `src/lib/mtp/`, `src-tauri/src/mtp/`,
  `src/lib/ai/`, `src-tauri/src/ai/`
- Target CLAUDE.md: `src-tauri/src/network/CLAUDE.md`, `src/lib/mtp/CLAUDE.md` (or `src-tauri/src/mtp/CLAUDE.md`),
  `src/lib/ai/CLAUDE.md`

**Cluster F — Licensing, auto-updater, file viewer, and all remaining small features**
- Docs: `specs/license-activation-spec`, `specs/trial-persistence-spec`, `specs/auto-updater-*` (2),
  `features/licensing`, `features/automated-updates`, `features/file-viewer`, `features/file-selection*` (2),
  `features/command-palette`, `features/back-forward-navigation`, `features/disk-access`, `features/font-metrics`,
  `features/persistence-stores`, `features/selection-info`, `features/sorting`
- Also remaining specs: `specs/checker-script-improvements`, `specs/lib-reorganization-plan`,
  `specs/split-up-big-files-plans`, `specs/swap-panes-plan`, `specs/website-newsletter-plan`,
  `specs/word-wrap-toggle-plan`, `specs/vnc-to-e2e-linux-env`, `specs/mobile-responsive-fix-plan`,
  `specs/linux-and-windows-versions*` (2), `specs/file-selection`
- Also remaining notes: `notes/codebase-review*` (3), `notes/test-coverage`, `notes/dropbox-statuses-and-formats`
- Code: `src/lib/licensing/`, `src/lib/updates/`, `src/lib/file-viewer/`, `src-tauri/src/file_viewer/`,
  `src/lib/file-explorer/selection/`, `src/lib/file-explorer/navigation/`, `src/lib/command-palette/`,
  `src/lib/font-metrics/`
- Target CLAUDE.md: `src/lib/licensing/CLAUDE.md`, `src/lib/file-viewer/CLAUDE.md`,
  `src-tauri/src/file_viewer/CLAUDE.md`
- Small features (sorting, selection, navigation, etc.): intent goes into the parent `file-explorer/CLAUDE.md`
  or `architecture.md` if it's truly cross-cutting.

### What goes into each report

For each cluster, the subagent produces:
1. List of intent items: "Decision: X was chosen because Y" or "Gotcha: Z behaves this way because..."
2. List of conflicts: "Spec says A, but code does B" — with enough context for user to pick one
3. Draft CLAUDE.md content per target file (concise, ~30-80 lines each, following the pattern: purpose → architecture →
   key decisions → gotchas)

## Milestone 2: User resolves conflicts

I collect all conflict flags from the 6 reports and present them to the user as batch questions. Format:
"Feature X: spec says A, code does B. Which is the intended behavior?" The user resolves each.

## Milestone 3: Write colocated CLAUDE.md files

Using resolved conflicts + distilled intent, write all CLAUDE.md files. Estimated ~12-15 files across the codebase.

## Milestone 4: Create architecture.md

A single `docs/architecture.md` (~100-150 lines) serving as a map:
- Lists each major subsystem with a one-liner and a pointer to the relevant directory (where the CLAUDE.md lives)
- Covers: frontend (Svelte), backend (Rust), license server, website, infra
- Not a manual — just enough for an agent to know where to look

## Milestone 5: Structural cleanup

1. **Fix AGENTS.md**: Remove stale `artifacts/` references. Update the `docs/` file structure description to match new
   reality (no more `features/`, `specs/`, `notes/`). Add the doc-update process instruction.
2. **Move user docs**: `docs/user-docs/` → `apps/desktop/user-docs/`
3. **Add process rule**: Add `.claude/rules/docs-maintenance.md` with a `paths: ["**"]` scope, instructing agents to
   check for and update CLAUDE.md when modifying a feature directory. This is the process change that keeps docs alive.
4. **Delete old docs**: Remove `docs/features/`, `docs/specs/`, `docs/notes/`. Keep `docs/adr/`, `docs/guides/`,
   `docs/tooling/`, `docs/style-guide.md`, `docs/security.md`, `docs/architecture.md`.

## Milestone 6: Verify

- Run `./scripts/check.sh --check knip` to ensure no broken imports from moved/deleted files
- Grep for stale references to deleted docs paths
- Spot-check 2-3 CLAUDE.md files by reading the directory as an agent would, confirming they're discoverable and useful
- Verify AGENTS.md is accurate

## Subagent plan

| Phase | Parallelism | Work |
|-------|-------------|------|
| M1 | 6 subagents in parallel | Each reads docs + code, writes report to `/tmp/cmdr-docs-reorg/cluster-{A-F}.md` |
| M2 | Sequential (me + user) | Compile conflicts, present batch questions |
| M3 | 3-4 subagents in parallel | Each writes 3-5 CLAUDE.md files from resolved reports |
| M4 | 1 subagent | Writes `architecture.md` from all reports |
| M5 | Sequential (me) | AGENTS.md edit, move user-docs, add rule, delete old dirs |
| M6 | Sequential (me) | Verification checks |
