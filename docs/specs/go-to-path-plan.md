# Go to path (⌘G) + "Go to latest download" rename

Plan for a keyboard-first "jump anywhere" action and a small naming/menu cleanup of its sibling action. This document
captures the **intention** behind each decision so the implementing agent can adapt details when reality pushes back, as
long as the intentions stay intact.

## Why

- Power users live by "type a path, hit Enter, I'm there." Cmdr is a keyboard-first orthodox file manager (see
  [design-principles.md](../design-principles.md)); a Go-to-path action is table stakes and currently missing.
- Two "jump" actions are conceptually siblings ("take me somewhere in the active pane"): the new Go-to-path and the
  existing reveal-latest-download. Today the latter is named "Reveal latest download" and is absent from the menu bar.
  Renaming it to "Go to latest download" and seating both under the **Go** menu makes the family legible and
  discoverable. This is a small, high-leverage consistency win.
- Clipboard is the natural source of "the path I want to go to" (you just copied it from a terminal, a chat, Finder's
  Get Info). ⌘G → Enter on a clipboard path, or ⌘G → digit on a recent, are the two muscle-memory flows we optimize.

## Scope / non-goals

- **Local filesystem only for v1.** Typed SMB/MTP paths are out of scope. Absolute and `~` paths always work; relative
  paths resolve against the active pane's current dir and assume a local-fs base (see "Relative paths on a non-local
  pane" gotcha). The architecture shouldn't preclude remote paths later, but none ship here.
- **No new MCP tool for Go-to-path.** The existing `nav_to_path(pane, path)` MCP tool already navigates a pane to a
  path; agents use that. We do not expose the dialog over MCP. (The only MCP change here is renaming the
  `reveal_latest_download` tool — see Milestone 0.)
- **Help → Search keywords are not achievable** for "jump"/"navigate". The macOS Help menu search field is the native
  NSMenu search (`setHelpMenu:`), which matches visible menu-item _titles_ only — there is no hidden-keyword metadata in
  NSMenu. We get those keywords into the **command palette** instead (Milestone 3). This is a deliberate accepted
  limitation, not an oversight.
- **Rename the whole feature, internals included.** Internal identifiers track the UI name (see the "Name internals
  after the UI" principle this work adds to `AGENTS.md` § Technicals). So every "reveal" identifier in the download-jump
  feature becomes "go to latest": the command id, the Tauri IPC command + its binding, the MCP tool, the frontend
  file/function/toast/id names, the Rust types, and the settings keys. Pre-production means **no migration** — a dev
  `settings.json` holding the old `globalRevealShortcut` key just falls back to default (acceptable). The `downloads`
  module name itself **stays** — it's the downloads _watcher_; "go to latest download" is one action within it. See
  "Rename scope" under Milestone 0 for the full target list.

## Confirmed behavior (the contract)

### Go to path dialog

- Small modal: a single-line textbox, a list of up to **10** recent paths, a Cancel button, and a "Go to path"
  (default/primary) button. A live warning line sits below the textbox.
- The textbox is auto-focused. It is **prefilled from the clipboard only if** the clipboard holds a path that resolves
  to something that exists on disk (backend-checked). So ⌘G → Enter opens the clipboard path; ⌘G → digit jumps to a
  recent. If the clipboard isn't an existing path, the box opens empty.
- **Digit keys 1–9, 0 jump immediately to the corresponding recent** (1 = most recent, … 9 = ninth, **0 = tenth**), but
  **only while the textbox is empty.** Valid paths never start with a digit (they start with `/`, `~`, or `.`), so once
  any character is in the box, digits are ordinary input. A digit with no corresponding recent is a no-op.
- **Clicking a recent row jumps immediately.** Each row has a trailing `[x]` button (tooltip "Remove from list") that
  removes the entry and does **not** jump (`stopPropagation`).
- Accepts directories, files, `~` expansion, and relative paths (resolved against the active pane's current dir).
- The "Go to path" button is enabled whenever the box is non-empty (even for a non-existent path — we jump to the
  nearest existing ancestor). It is disabled only when the box is empty.
- Enter in the textbox = Go. Esc = Cancel.

### Navigation semantics (always in the currently **focused** pane)

The backend resolves the typed string to exactly one of three outcomes; the frontend acts on each:

1. **Existing directory** → navigate the focused pane into it. The cursor lands on the 0th row (`..`) via normal
   navigation; nothing special needed.
2. **Existing file** → navigate the focused pane to the file's **parent** and put the cursor on the file (reveal/select
   it). We do **not** open it. This reuses the exact helper the reveal-download feature uses.
3. **Non-existent path** → navigate to the **nearest existing ancestor** (e.g. `/tmp/nope/a.txt` → `/tmp`; worst case
   `/`), and fire an **INFO** toast: "Requested path `/tmp/nope/a.txt` doesn't exist, jumped to `/tmp`. Press `⌘[` to go
   back." The `⌘[` is rendered **dynamically** from the live effective shortcut for `nav.back`
   (`getEffectiveShortcuts('nav.back')[0]`), snapshotted at toast-creation, **never hardcoded**.

While typing, a debounced backend resolve drives a **live inline warning** below the box for the non-existent case:
"This path doesn't exist. The closest place to go is `/tmp`." (Directory/File outcomes show no warning.) The same single
resolve command serves both the live preview and the actual jump — one source of truth.

### Recents

- Populated **only by manual jumps in this dialog** — not by `nav_to_path` MCP calls, not by ordinary app-wide
  navigation. (Matches the search-history "record only on the explicit action" precedent.)
- Stores the **resolved target we actually jumped to**, not the raw input: a directory jump stores the dir; a file jump
  stores the file path; a nearest-ancestor jump stores the ancestor (e.g. `/tmp`), never the typo'd input. Deduped by
  resolved path, move-to-top, **cap 10** (a fixed const, not a user setting).
- Long paths are middle-truncated for display (via the existing pretext measurer in `lib/utils/shorten-middle.ts`) with
  the full path in a `title` tooltip.

### "Go to latest download" (rename of "Reveal latest download")

- Rename every occurrence of "Reveal latest download" → "Go to latest download" (user-facing strings) and rename all
  internal "reveal" identifiers to the "go to latest" vocabulary (see "Rename scope" in M0). Keep ⌘J and the global ⌃⌥⌘J
  hotkey.

### Go menu (both platforms)

New order: `Back`, `Forward`, `──`, `Parent folder`, `──`, `Go to path…`, `Go to latest download`.

- `Go to path…` carries the macOS ellipsis (it opens a dialog); `Go to latest download` has none (direct action).
- `Go to path…` gets ⌘G as a **native menu accelerator**, following the `search.open`/⌘F precedent (see "Menu
  double-dispatch" below). `Go to latest download` gets ⌘J the same way.

## Architecture decisions and the "why"

### Where the dialog lives: `+page.svelte`, not `DialogManager`

`DialogManager.svelte` hosts **pane-scoped file-operation** dialogs (transfer, delete, mkdir, mkfile). The Search and
Selection dialogs — the closest analogs to Go-to-path (window-scoped modals that read the focused pane and act on it) —
mount at `routes/(main)/+page.svelte` and dispatch through the `CommandDispatchDialogs` interface. Go-to-path follows
the Search/Selection pattern: one `showGoToPathDialog` boolean in `+page.svelte`, a `GoToPathDialog.svelte` mounted
there, and a `showGoToPathDialog(show)` method added to `CommandDispatchDialogs`. Rationale: it's a window-level modal,
not a pane file-op, and this keeps it beside its true siblings.

### Backend owns resolution (smart backend, thin frontend)

A single new async Tauri command does **all** the path reasoning so the frontend stays a thin presenter (per AGENTS.md
principle 3):

```
resolve_go_to_path(input: String, base_dir: String) -> GoToPathResolution
```

- Expands `~`, joins relative input against `base_dir` (the focused pane path), and **lexically** normalizes `.`/`..`
  (we do **not** `canonicalize()` — that requires the whole path to exist and would resolve symlinks, changing what we
  show the user and breaking nearest-ancestor logic).
- Classifies the result against the **local** filesystem and returns a tagged enum:

```
// rename_all_fields is REQUIRED: without it, struct-variant fields ship snake_case
// through tauri-specta and read `undefined` on the TS side. Enforced by the
// `ipc-enum-camelcase` check; `RevealError` in downloads/commands.rs is the precedent.
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
enum GoToPathResolution {
    Directory { path: String },                              // navigate into `path`
    File { parent_dir: String, file_name: String },          // navigate to parent, select file
    NearestAncestor { requested: String, ancestor_dir: String }, // navigate to ancestor, INFO toast
    Invalid { reason: String },                              // defensive: empty/unresolvable input
}
```

- **Async + `blocking_with_timeout`** because it touches the filesystem (`exists`/`metadata` can block on a hung mount;
  see architecture.md § Platform constraints). The frontend wraps the call in `withTimeout` for the debounced live
  preview so a slow mount never freezes typing.
- This one command serves three callers: the live inline warning (debounced as-you-type), the actual jump (Enter / row
  click / digit), and the clipboard-prefill decision (run it on the clipboard string; prefill only on `Directory` /
  `File`). One resolution path, no drift between preview and action.

`expand_tilde()` already exists in `commands/file_system/mod.rs` — reuse it.

**No capability entries needed.** `capabilities/*.json` gate only Tauri core/plugin commands; app-defined
`#[tauri::command]`s (`resolve_go_to_path`, the recents IPC) are reachable by default. Clipboard read uses the existing
app command `commands.readClipboardText` (`read_clipboard_text`), not the `clipboard-manager` plugin, so it needs no
permission either. Calling this out because "new Tauri command → add a capability" is a strong AGENTS.md reflex that
doesn't apply here.

### DRY: extract the navigate-and-select helper

`navigateToRevealedFile(explorer, parentDir, fileName)` currently lives **private** inside `lib/downloads/reveal.ts` (it
carefully handles `navigateToPath`'s sync-error-string vs Promise return, then awaits the listing before `moveCursor`).
Extract it to a shared module `lib/file-explorer/navigation/navigate-and-select.ts`, exporting:

- `navigateToFileInPane(explorer, pane, parentDir, fileName)` — the existing reveal logic, verbatim.
- `navigateToDirInPane(explorer, pane, dir)` — the directory case (navigate, no cursor move; sync-error handling
  preserved).

`reveal.ts` imports the extracted helper (small, behavior-preserving refactor). `go-to-path.ts` uses both. This is the
core code-reuse win: both "jump" features share one navigation primitive instead of each rolling their own.

### Recents store: a trimmed clone of `search/history.rs`

The backend recents store is a feature-local module modeled directly on `search/history.rs` (the proven pattern:
in-memory `Mutex<Store>`, `OnceLock` lazy load, atomic temp+rename via `config::durable_write_json`, schema-versioned
with corrupt-file quarantine, pure `add_to_store`/`remove_from_store`/`trim_to_cap` functions for unit testing without
an `AppHandle`). It is **simpler**: an entry is `{ id, timestamp, path }`, dedupe key is the resolved path string, cap
is a fixed `10`. No filters, no modes, no configurable max-count. File: `go-to-path-history.json` in the app data dir.

**Lock-poison compliance:** clone `search/history.rs`'s lock idiom verbatim —
`.lock().unwrap_or_else(|e| e.into_inner())` and `match … Err(poisoned) => poisoned.into_inner()`. A "simplification" to
`.lock().unwrap()` trips the `lock-poison` check. **Case-insensitivity (v1 limitation):** dedupe is a raw resolved-path
string compare, so on case-insensitive APFS `/Users/x/Foo` and `/Users/x/foo` are the same dir but would show as two
recents. Accept this for v1 (worst case: a duplicate-looking entry) and note it in the CLAUDE.md. Do **not** reach for
`canonicalize()` (symlink/nearest-ancestor reasons above) or the index DB's `platform_case` collation (that's
index-only).

> Do not over-abstract `search/history.rs` into a shared generic store as part of this work. The two stores share a
> _shape_, not a contract; a premature generic would couple search-history's tuning knobs to go-to-path's fixed cap.
> Clone the ~120 lines that matter and move on (elegance lives between duplication and overengineering — AGENTS.md).

### Menu double-dispatch — the real mechanism (both paths fire; idempotency saves us)

A command with a native menu accelerator **and** a `command-registry` shortcut fires **both** paths on macOS:
`shortcuts/CLAUDE.md` § "Modifier-key accelerators may fire twice (menu + JS)" documents that AppKit can leak the
modifier keydown to the webview **even after** the menu accelerator already fired, so `on_menu_event` emits
`execute-command` AND the JS `handleGlobalKeyDown` also dispatches. This was a real bug for the Quick Look toggle (it
fired twice). It is invisible for most commands because the action is **idempotent** — opening the search dialog twice
is a no-op (`+page.svelte:813`: `if (show && showSearchDialog) return // Already open`). So the design rule is:

- `Go to path…` (⌘G): the dialog-open MUST be guarded the same way — `if (show && showGoToPathDialog) return` in the
  `CommandDispatchDialogs.showGoToPathDialog` callback. With that guard, a double-fire opens the dialog exactly once.
- `Go to latest download` (⌘J): re-navigating to the same latest download twice is naturally idempotent, so no guard is
  needed. (Today ⌘J is JS-only; adding the menu item makes it fire both ways, but the net effect is one reveal.) Expect
  two `FE:user-action downloads.goToLatest` log lines on a single press — that's the documented, harmless pattern.
- Both `command-registry` `shortcuts` entries stay the source of truth for the **label** (synced onto the menu via
  `update_menu_item_accelerator` / `frontend_shortcut_to_accelerator`) and for customization. The global ⌃⌥⌘J hotkey is
  a separate `global-shortcut` plugin registration and is unaffected.
- **Focus gating is automatic, not a separate "set."** `commands/ui.rs` `set_menu_context` iterates `MenuState.items`
  and disables every item whose `menu_id_to_command(id)` scope is not `CommandScope::App`. So registering the two items
  in the `items` HashMap (via `register_item`) and mapping them to `CommandScope::FileScoped` in `menu_id_to_command` is
  **all** that's needed for them to grey out in the viewer/settings windows — no third wiring step.
- Implementer must still verify in testing: ⌘G opens the dialog exactly once, ⌘J reveals exactly once (log may show two
  lines; UI effect is single), neither fires from the file viewer / settings windows, and ⌃⌥⌘J still works.

### Command-palette keywords

Add an optional `keywords?: string[]` to the `Command` interface (`lib/commands/types.ts`). In `fuzzy-search.ts`, build
the haystack as `name + ' ' + (keywords ?? []).join(' ')` instead of `name` alone. **Highlight safety:** the returned
`matchedIndices` are positions into the visible `name`; when computing them, drop any index `>= name.length` so a match
that landed in the appended keyword text ranks/returns the command but never produces a bogus highlight past the visible
label. Give both `nav.goToPath` and `downloads.goToLatest` keywords `['jump', 'navigate', 'goto']` (and `'download'`
already lives in the latter's name).

## Critical files

**Backend (Rust)**

- `src-tauri/src/go_to_path/mod.rs` — **new.** Pure resolution logic (`resolve` over expand_tilde + lexical normalize +
  nearest-ancestor walk + dir/file classify) and its unit tests.
- `src-tauri/src/go_to_path/history.rs` — **new.** Recents store (clone-and-trim of `search/history.rs`).
- `src-tauri/src/go_to_path/CLAUDE.md` — **new.**
- `src-tauri/src/commands/go_to_path.rs` — **new.** Thin IPC: `resolve_go_to_path`, `get_recent_paths`,
  `add_recent_path`, `remove_recent_path`, `clear_recent_paths`. Register in the `tauri::generate_handler!` list and the
  specta builder; load history at startup beside `search::history::load_history`.
- `src-tauri/src/menu/mod.rs` — new IDs `GO_TO_PATH_ID`, `GO_LATEST_DOWNLOAD_ID`; entries in **both**
  `menu_id_to_command` (→ `nav.goToPath` / `downloads.goToLatest`, both `CommandScope::FileScoped`) and
  `command_id_to_menu_id` (the reverse map — needed so custom-shortcut → menu-accelerator sync works); update the
  dispatch test list.
- `src/lib/shortcuts/shortcuts-store.ts` — add `'nav.goToPath'` **and** `'downloads.goToLatest'` to the `menuCommands`
  array (the latter isn't there today — it had no menu item). This is the **fourth** of the "four places" a
  command-with-a-menu-item must be registered (see `lib/commands/CLAUDE.md` § the four-places gotcha; pinned by
  `shortcuts.test.ts`). Missing it = the accelerator label won't sync to custom rebinds.
- `src-tauri/src/menu/macos.rs` — build the two items, append to the Go submenu, update the hardcoded position comment +
  `register_item` indices (`back(0), forward(1), sep(2), parent(3), sep(4), go_to_path(5), go_latest_download(6)`), add
  SF Symbols to the `"Go"` map (`"Go to path\u{2026}"` → e.g. `arrow.right.to.line`; `"Go to latest download"` → e.g.
  `arrow.down.circle`) — titles must match exactly, ellipsis included.
- `src-tauri/src/menu/linux.rs` — mirror with unique `&` mnemonics in the Go submenu. `&Back`/`&Forward`/`&Parent`
  already claim B/F/P; open letters for the two new items include `t`, `l`, `h` (e.g. "Go &to path…", "Go to &latest
  download").
- `src-tauri/src/mcp/tools.rs` + `src-tauri/src/mcp/executor/mod.rs` + `src-tauri/src/mcp/executor/downloads.rs` —
  rename the `reveal_latest_download` MCP **tool** (name + description in `tools.rs`) → `go_to_latest_download`, AND
  update the dispatch match arm in `executor/mod.rs:172` (`"reveal_latest_download" => …`) to the new name — **without
  this, the renamed tool lists but fails to dispatch.** The executor fn (`execute_reveal_latest_download`) still calls
  the unchanged Tauri command internally; renaming the fn itself is optional internal churn and may be skipped.

**Frontend (Svelte/TS)**

- `src/lib/go-to-path/GoToPathDialog.svelte` — **new.** Modeled on `NewFileDialog.svelte` (ModalDialog + Button, onMount
  focus/select, debounced validation, Enter-to-confirm). Holds the textbox, live warning, recents list with digit
  chips + `[x]`, buttons.
- `src/lib/go-to-path/go-to-path.ts` — **new.** The handler: resolve → switch on outcome → call `navigateToDirInPane` /
  `navigateToFileInPane` / (ancestor) navigate + INFO toast; records the resolved target into recents on success.
- `src/lib/go-to-path/recent-paths-state.svelte.ts` — **new.** `$state` mirror of the backend recents (load on open,
  add/remove via IPC), modeled on `search/recent-searches-state.svelte.ts`.
- `src/lib/go-to-path/GoToPathAncestorToastContent.svelte` — **new.** INFO toast component; props `requested`, `landed`,
  `backShortcut` (snapshotted at creation, per the downloads snapshot rule). Code-formats the paths and the kbd.
- `src/lib/go-to-path/CLAUDE.md` — **new.**
- `src/lib/file-explorer/navigation/navigate-and-select.ts` — **new.** Extracted shared helper (see DRY decision).
- `src/lib/downloads/reveal.ts` — import the extracted helper instead of the private copy.
- `src/lib/commands/types.ts` — add `keywords?: string[]`.
- `src/lib/commands/command-registry.ts` — add the `nav.goToPath` entry (scope `Main window` — matching `search.open`,
  another dialog-opener; scope is **documentation-only** per `commands/CLAUDE.md`, it does NOT gate dispatch, so ⌘G
  works while a file pane is focused regardless), `shortcuts: ['⌘G']`, `keywords`); rename the `downloads.revealLatest`
  entry (id → `downloads.goToLatest`, `name` → "Go to latest download") and add its `keywords`. **Confirm ⌘G is
  otherwise unused** (it isn't a registry shortcut or menu accelerator today).
- `src/lib/commands/fuzzy-search.ts` — fold keywords into the haystack with highlight-index clamping.
- `src/routes/(main)/command-dispatch.ts` — add `case 'nav.goToPath'` → `ctx.dialogs.showGoToPathDialog(true)`.
- `src/routes/(main)/+page.svelte` — `showGoToPathDialog` state, mount `GoToPathDialog`, wire close, add to the "any
  modal open" guard, implement the `CommandDispatchDialogs.showGoToPathDialog` callback.
- `src/lib/settings/settings-registry.ts`, `src/lib/settings/sections/FileSystemWatchingSection.svelte`,
  `src/lib/downloads/GlobalShortcutRow.svelte` — rename the user-facing "Reveal latest download" label.

## Milestones

Each milestone is independently committable and leaves the app green. Sequential is fine (we're not in a hurry).

### M0 — Rename "Reveal latest download" → "Go to latest download" (UI + internals)

Self-contained, no new behavior: a full vocabulary rename plus the new naming principle. First, **add the principle** to
`AGENTS.md` § Technicals (new list item) — this is the rule that motivates the rest:

> **Name internals after the UI.** When a feature or action has a user-facing name, its internal identifiers — command
> ids, file/function/type names, settings keys, MCP tools — use the same vocabulary. If the UI says "Go to latest
> download", the code says `goToLatest`, not `revealLatest`. A UI "Go to…" backed by a `reveal_*` command forces every
> reader to keep a mental translation table, and the mismatch rots as the label drifts. Rename internals when you rename
> the UI.

**Rename scope** (grep `reveal`/`Reveal` under `lib/downloads/`, `src-tauri/src/downloads/`, `commands/`, `settings/`,
`mcp/`, `shortcuts/` and the command registry; rename all to the go-to-latest vocabulary). Representative targets — the
final names are the implementer's call as long as no "reveal" vocabulary survives:

- **Command id:** `downloads.revealLatest` → `downloads.goToLatest`. Ripples to: `command-registry.ts` entry (id +
  name), the `command-dispatch.ts` case, `menuCommands` in `shortcuts-store.ts`, every
  `getEffectiveShortcuts('downloads.revealLatest')` snapshot site (the downloads toasts), and
  `command_id_to_menu_id`/`menu_id_to_command` (added in M3).
- **Tauri IPC command:** `reveal_latest_download` (`downloads/commands.rs`) → `go_to_latest_download`; binding
  `revealLatestDownload` → `goToLatestDownload`. **`pnpm bindings:regen` is now required** (this IS a specta command).
- **MCP tool:** `reveal_latest_download` → `go_to_latest_download` in `mcp/tools.rs` (name + description) **and** the
  dispatch arm `mcp/executor/mod.rs:172`, the executor fn `execute_reveal_latest_download`, the test assertion
  `mcp/tools.rs:769`, and the `mcp/executor/CLAUDE.md` table row — or `cargo nextest` fails.
- **Frontend:** `lib/downloads/reveal.ts` → `go-to-latest.ts`; `revealLatestDownload`/`revealPath` →
  `goToLatestDownload`/`goToDownload`; `Reveal{Empty,Fda}ToastContent.svelte` + `reveal-ids.ts` + the `REVEAL_*` ids →
  download-themed names; `navigateToRevealedFile` disappears into the M2 shared helper (`navigateToFileInPane`).
- **Rust types:** `RevealedDownload` → e.g. `LatestDownload`; `RevealError` → `GoToLatestError`.
- **Settings keys:** `behavior.fileSystemWatching.globalRevealShortcut.*` → `…globalGoToLatestShortcut.*` across
  `settings-registry.ts`, `global-shortcut-setting.ts`, `GlobalShortcutRow.svelte`, the descriptions, and the **Rust
  startup/focus read** (the key is read from disk before any window loads — don't miss the Rust side).
- **User-facing strings:** command-palette name, settings label + `FileSystemWatchingSection.svelte`,
  `GlobalShortcutRow` label.
- **Docs:** `lib/downloads/CLAUDE.md`, `mcp/CLAUDE.md`, `mcp/executor/CLAUDE.md`, `docs/architecture.md` downloads row.

**Verify:** `pnpm bindings:regen` then `./scripts/check.sh` (catches stale bindings, the moved test assertions,
lock-poison, etc.); palette shows the new name; ⌘J still reveals; ⌃⌥⌘J still works.

> **Pin the user-facing string once.** The exact label chosen here ("Go to latest download") is reused verbatim as the
> M3 macOS menu-item **title**, and the SF-Symbol map (`macos.rs`) matches items **by exact title string**. A drift
> between the M0 rename and the M3 menu title silently breaks the icon. Treat the M0 string as canonical and byte-copy
> it into M3.

### M1 — Backend: resolution + recents store

`go_to_path/mod.rs` (resolve + tests), `go_to_path/history.rs` (store + tests), `commands/go_to_path.rs` (IPC),
handler/specta registration, startup load. `pnpm bindings:regen`. Rust tests green. No UI yet.

### M2 — Frontend: dialog + handler + wiring

Extract `navigate-and-select.ts` and repoint `reveal.ts` (verify reveal still works). Build `GoToPathDialog.svelte`,
`go-to-path.ts`, `recent-paths-state.svelte.ts`, the ancestor toast component. Add the `nav.goToPath` command entry +
dispatch case + `+page.svelte` hosting. Vitest for the handler and pure helpers. **Verify** via the running app
(`pnpm dev`): ⌘G opens (wire the registry shortcut first so it works before the menu lands), all three outcomes, digit
jump, clipboard prefill, recents add/remove.

### M3 — Go menu (both items) + palette keywords

One menu pass adds both `Go to path…` (⌘G) and `Go to latest download` (⌘J) to macOS + Linux, with IDs, mappings,
position rewrite, SF Symbols, focus gating. Add the `keywords` field + fold into fuzzy-search. **Verify:** menu items
present and enabled/disabled correctly; ⌘G/⌘J fire exactly once each from the menu; palette finds both via "jump" and
"navigate"; ⌃⌥⌘J still works.

### M4 — Tests round-out + docs + full checks

E2E (one Playwright spec), the new `CLAUDE.md` files, `docs/architecture.md` rows, manual smoke checklist. Run
`./scripts/check.sh --include-slow`.

## Testing

**Rust unit (M1).** `go_to_path/mod.rs` table-driven over a `tempfile::tempdir()`: existing dir → `Directory`; existing
file → `File { parent, name }`; `~/…` expansion; relative input joined to base_dir; `.`/`..` lexical normalization
including a `..` past a nonexistent middle segment; deep nonexistent → nearest ancestor (`/tmp/nope/a.txt` → `/tmp`);
`/totally-nonexistent` → `/`; empty input → `Invalid`. `go_to_path/history.rs`: dedupe + move-to-top, cap-10 eviction,
remove found/not-found, serialization round-trip, corrupt-file quarantine, missing-file default (mirror the
`search/history.rs` test suite, trimmed).

**Frontend unit (M2/M3, Vitest).**

- `go-to-path.ts` handler with mocked IPC: each outcome calls the right navigation helper; the `NearestAncestor` case
  builds the toast with `getEffectiveShortcuts('nav.back')[0]` (assert the dynamic binding is read, not hardcoded — mock
  it to a non-default like `⌘B` and assert the toast prop reflects it).
- A pure `digitToRecentIndex(inputValue, key, recentsCount)` helper: empty box + `'1'..'9'` → 0..8, `'0'` → 9,
  out-of-range → null, non-empty box → null, modifier held → null.
- A pure `shouldPrefillClipboard(resolution)` helper: `Directory`/`File` → true, else false.
- `recent-paths-state`: add/dedupe/move-to-top/cap/remove mirror behavior.
- `fuzzy-search`: querying "jump" and "navigate" returns both `nav.goToPath` and `downloads.goToLatest`; highlight
  indices never exceed the visible name length.

**E2E (M4, Playwright — keep to one spec, per testing.md).** ⌘G (or menu) opens the dialog, type an existing directory,
Enter, assert the focused pane navigated. Add a second assertion for the nonexistent → ancestor toast if cheap.
Clipboard prefill and OS-clipboard interplay are covered by unit + the manual smoke list (clipboard perms make it
brittle in E2E).

**Manual smoke checklist** goes in `lib/go-to-path/CLAUDE.md` (mirror the downloads checklist style): clipboard prefill
on/off, the three outcomes, digit jump, row click, `[x]` remove, the dynamic back-shortcut in the toast (rebind nav.back
and re-check), menu items + accelerators, palette keyword search.

## Docs to update

- **New:** `src/lib/go-to-path/CLAUDE.md`, `src-tauri/src/go_to_path/CLAUDE.md`.
- `docs/architecture.md`: add frontend `go-to-path/` and backend `go_to_path/` rows; refresh the `downloads/` and
  `menu/` rows for the rename + new Go-menu items.
- `apps/desktop/src/lib/downloads/CLAUDE.md`: rename user-facing labels to "Go to latest download"; note the new menu
  item and the MCP tool rename; keep internal-identifier notes intact.
- `src-tauri/src/menu/CLAUDE.md`: Go-menu structure, new IDs, SF Symbols, the double-dispatch note.
- `src-tauri/src/mcp/CLAUDE.md`: tool rename.
- Record a Decision/Why in `lib/go-to-path/CLAUDE.md` for the backend-resolution choice and the no-canonicalize choice.

## Parallelization

Sequential is the default and totally fine. The only genuinely-safe parallel split (no worktree, no shared files): M1
(backend, all under `go_to_path/` + a new `commands/` file + handler registration) and the M0 rename touch disjoint
files **except** `mcp/` — so do M0's MCP rename and M1's command registration as separate commits to keep diffs clean.
Within M2, the `navigate-and-select.ts` extraction must land before `go-to-path.ts` and the `reveal.ts` repoint. M3's
menu work depends on the `nav.goToPath` command entry existing (M2). Don't parallelize the menu pass with anything — the
Go-menu position rewrite is a single-owner edit.

## Trade-offs and gotchas

- **Relative paths on a non-local pane.** `base_dir` is the focused pane path; if that pane is on MTP/SMB, a relative
  input resolves against a non-local base and the local-fs existence walk will fall back to nearest-ancestor (often
  `/`). That's acceptable degraded behavior — absolute and `~` paths always work. Document it as a v1 limitation in the
  CLAUDE.md; don't engineer around it.
- **No `canonicalize()`.** We normalize lexically so the path we show and navigate matches what the user typed, symlinks
  aren't silently rewritten, and nearest-ancestor works when the full path doesn't exist. `metadata()` (which follows
  symlinks) classifies the _existing_ target as file/dir — a symlinked dir navigates into the symlink path, which the
  listing follows. Correct and intended.
- **`navigateToPath` sync-error string.** It returns `string | Promise<void>`; a string means navigation couldn't even
  start (e.g. a snapshot pane on a missing volume). The extracted helper preserves reveal.ts's report-and-bail on the
  string so `moveCursor` never races an empty cache. Keep that handling.
- **Digit/textbox conflict** is resolved by the empty-box guard, not by modifiers — confirmed with the user. The guard
  is unambiguous because no valid path starts with a digit. State this in a code comment at the keydown site.
- **Toast snapshot rule.** The back-shortcut shown in the ancestor toast is captured at toast-creation (a later rebind
  doesn't rewrite a visible toast), matching the downloads toast's snapshot-at-creation rule. The _next_ toast picks up
  the new binding.
