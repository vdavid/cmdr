# What's new popup

Plan for a post-update "What's new" dialog: after Cmdr updates, show the changelog slice between the version the user
last saw and the version now running, in a good-looking soft dialog. This document captures the **intention** behind
each decision so the implementing agent can adapt details when reality pushes back, as long as the intentions stay
intact.

## Why

- The updater is deliberately silent (download in background, "Restart to update" toast, done). That's great UX for the
  mechanics and terrible UX for the story: users get new features every few days and never hear about them. A changelog
  popup closes that loop and shows the project's pace, which is itself a beta selling point.
- The CHANGELOG is already written for this. The release flow (`.claude/commands/release.md` § Audience) pins the
  contract: the prose lead plus the Added / Changed / Fixed / Security sections are user-facing material, rendered with
  commit links stripped and Non-app dropped. Every release section now opens with a 1–2 sentence plain-prose lead. The
  popup is the second renderer of that contract (GitHub release notes being the first).
- Some users genuinely don't care. They get a one-click, guilt-free opt-out ("Not interested in changelogs"),
  re-enableable under Settings > Updates & privacy.

## Scope / non-goals

- **No network.** The popup renders the changelog embedded in the running binary. No fetching from getcmdr.com, no
  "newer notes than my version" cases, works offline by construction.
- **No images, no per-release marketing.** v1 renders the existing markdown (lead + bulleted sections). If a release
  ever deserves a hero screenshot, that's a future iteration.
- **No Linux/Windows forks needed.** The dialog is a plain soft modal; nothing platform-specific beyond the Help menu
  registration that already forks per platform.
- **No backlog replay on re-enable.** When the feature is off, `lastSeenVersion` still advances silently on every
  launch. Re-enabling shows future updates only, never a months-deep dump.

## Confirmed behavior (the contract)

### When the popup shows

On main-window startup, after settings load:

1. **First run ever** (no `whatsNew.lastSeenVersion` stored): silently set it to the current version and never show the
   popup. Fresh installs get onboarding, not a changelog. This also makes every E2E run popup-free by default (fresh
   instance data dir → first-run rule).
2. **Version unchanged or downgraded**: do nothing (on downgrade, rewrite `lastSeenVersion` to current so a later
   re-upgrade behaves sanely).
3. **Version increased**: update `lastSeenVersion` to current, and if `whatsNew.showOnUpdate` is on, open the dialog
   showing every released section `lastSeen < v <= current`, newest first.

Sequencing: never over onboarding. Gate on the same mechanism the update toast uses (`onboardingShowing` + re-attempt
when onboarding closes, see `updater.svelte.ts` `shouldShowUpdateToast`). If another startup modal (crash report prompt,
expiration modal) is already up, the popup waits for the next launch rather than queueing — `lastSeen` in that case is
NOT advanced past the skipped versions until the popup (or the silent paths 1/2) actually ran. Missing one launch is
fine; double-showing or stacking modals is not.

### The dialog

- A **soft modal** (`ModalDialog`, draggable, with `onclose`), `dialogId: 'whats-new'` registered in
  `SOFT_DIALOG_REGISTRY`. Sized roughly like the Search dialog: comfortable reading width (~560 px), body scrolls,
  max-height capped to the window.
- Title: **"What's new in Cmdr"**. Body, per skipped release, newest first:
  - A version heading: `0.25.0 · 2026-06-11` (version prominent, date subtle).
  - The prose lead as an intro paragraph.
  - The Added / Changed / Fixed / Security sections that exist, as small subheadings with bulleted entries. Non-app
    never appears. Commit-link groups are already stripped by the parser (see below); remaining inline markdown (bold,
    `code`, quotes, ⌘-symbols) renders via the existing `snarkdown` path.
- Footer: **"Not interested in changelogs"** as a subtle text button (LinkButton style, `--color-text-secondary`)
  bottom-left; **"Close"** as the primary button bottom-right. Esc closes. No other actions.
- "Not interested in changelogs" sets `whatsNew.showOnUpdate = false`, closes the dialog, and fires a default-level
  toast: "Got it, no more update notes. Re-enable them anytime in Settings > Updates & privacy." (Copy is a draft; David
  reviews all user-facing strings per the humans-to-humans principle.)
- Many skipped versions: render them all, scrollable, no cap. In practice a user skips a handful; the scroll handles the
  pathological case without extra UI.

### Settings

- New visible setting in `['Updates & privacy']`, directly under `updates.autoCheck`:
  - id `whatsNew.showOnUpdate`, type `boolean`, default `true`, component `switch`.
  - Label: "Show what's new after updates". Description: "After Cmdr updates itself, show a quick summary of what
    changed." Keywords: `['changelog', 'release notes', "what's new", 'update notes']`.
- New **hidden** registry entry `whatsNew.lastSeenVersion` (type string, default `''`), following the
  `onboarding.upgradeNudgeShown` hidden-entry precedent. Hidden entries live in the same settings store, sync across
  windows, and don't render in the Settings UI.

### Help menu + command palette

- New command `help.whatsNew`, name "What's new", palette-visible, no shortcut. Opens the same dialog showing the
  **current version's** section only (a manual reopen, not a diff — the diff is by definition already seen). Works
  regardless of the `showOnUpdate` setting; the setting governs only the automatic popup.
- macOS Help menu order: **What's new**, Send feedback…, Send error report…. Linux mirrors with a unique `&` mnemonic.
- This is a command-with-menu-item, so all four places apply (`command-ids.ts` + `command-registry.ts`,
  `command-dispatch.ts` case, `menu/mod.rs` id + both mappings + `macos.rs`/`linux.rs` items, `menuCommands` in
  `shortcuts-store.ts` — the last one matters even with no default shortcut, so a future custom binding syncs to the
  menu). Scope `CommandScope::App` (like `app.about`): usable from any window via the menu; the dialog itself mounts in
  the main window.

## Architecture decisions and the "why"

### Content source: `include_str!` the CHANGELOG into the binary

The backend embeds the repo-root changelog at compile time:

```rust
const CHANGELOG_MD: &str = include_str!("../../../../CHANGELOG.md"); // path from the module's location; verify
```

Why this beats the alternatives:

- **Atomic with the version by construction.** The binary that reports version X embeds the changelog as of building X.
  No build-pipeline step, no `bundle.resources` mapping reaching outside `src-tauri/`, no generated-and-committed JSON
  with a freshness check (the `bindings-fresh` pattern exists but earns its complexity for cross-language types; a
  markdown file doesn't).
- **No file I/O at runtime** → the IPC command needs no `blocking_with_timeout` and can't hang on anything.
- ~200 KB of UTF-8 in the binary is noise next to the bundled llama-server.
- Dev-mode staleness is acceptable: editing CHANGELOG.md doesn't trigger a Tauri rebuild (the watcher only covers
  `src-tauri/`), so a dev session shows the changelog as of the last cargo build. Cargo tracks `include_str!` inputs, so
  the next build picks it up automatically. Document this in the module CLAUDE.md.

### Backend owns parsing (smart backend, thin frontend)

One Rust module parses the embedded markdown into a typed model, lazily once (`OnceLock`), with the
user-facing-rendering contract applied at parse time:

- Recognize `## [x.y.z] - YYYY-MM-DD` section headings; skip `## [Unreleased]` and everything below the
  `### Development history` details block.
- Per release: capture the **lead** (prose paragraphs between the heading and the first `###`), and the Added / Changed
  / Fixed / Security sections. **Drop Non-app entirely.** Tolerate absent sections and unknown section names (drop those
  too, log a debug line).
- Per entry: join continuation lines, then strip the trailing commit-link group — the
  ` ([8hexchars](https://github.com/vdavid/cmdr/commit/…), …)` parenthetical. Convert any other markdown link to its
  plain text (the webview shouldn't grow in-dialog navigation, and external-open plumbing isn't worth it for v1). Keep
  bold/italic/code/quotes verbatim for the frontend renderer.
- Version comparison via the `semver` crate (already in the dependency tree through the updater; verify, else add it —
  license-check via `cargo deny` like any dep).

```rust
// rename_all_fields is REQUIRED (ipc-enum-camelcase check; see go-to-path-plan.md for the precedent)
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
struct WhatsNewRelease {
    version: String,          // "0.25.0"
    date: String,             // "2026-06-11" (display string, no parsing needed)
    lead: Option<String>,     // markdown
    sections: Vec<WhatsNewSection>, // { title: "Added" | "Changed" | "Fixed" | "Security", entries: Vec<String> }
}
```

IPC: `get_whats_new(since_version: Option<String>) -> Vec<WhatsNewRelease>`. `Some(v)` returns `v < release <= current`
(the startup diff); `None` returns just the current version's release (the Help-menu reopen). Thin pass-through command
per the thin-IPC principle; the parsing and slicing logic lives in a `whats_new/` module with plain unit-testable
functions. `pnpm bindings:regen` required.

### Trigger logic is a pure function

Frontend keeps a tiny `shouldShowWhatsNew({ lastSeen, current, enabled, onboardingShowing, otherStartupModalOpen })`
pure helper plus a thin effect that calls it on startup and on onboarding close (mirroring the update-toast re-attempt).
All state via the existing `getSetting`/`setSetting`; no new persistence mechanism.

### Rendering

The dialog reuses `renderErrorMarkdown()`'s `snarkdown` path for entry/lead markdown (`{@html}` is safe here: the
content is our own committed changelog, not user input — same trust level as `FriendlyError`'s `md!` output). Styling
follows the design system: tokens only, sentence case, dark/light both verified, `prefers-reduced-motion` respected (no
entrance animation beyond the standard ModalDialog one).

## Critical files

**Backend (Rust)**

- `src-tauri/src/whats_new/mod.rs` — **new.** `include_str!`, parser (heading/lead/section/entry extraction,
  link-stripping, Non-app drop), `releases_between(since, current)` slicing, `OnceLock` cache, unit tests.
- `src-tauri/src/whats_new/CLAUDE.md` — **new.** Parser contract, dev-staleness note, the "changelog is the source of
  truth; fix formatting there, not here" guardrail.
- `src-tauri/src/commands/whats_new.rs` — **new.** Thin `get_whats_new` IPC; register in `generate_handler!` + specta
  builder.
- `src-tauri/src/menu/mod.rs` — `HELP_WHATS_NEW_ID`, both `menu_id_to_command` (→ `help.whatsNew`, `CommandScope::App`)
  and `command_id_to_menu_id` mappings; dispatch test list.
- `src-tauri/src/menu/macos.rs` — Help menu item "What's new" above Send feedback; SF Symbol if one fits (e.g.
  `sparkles`); exact-title-match gotcha applies.
- `src-tauri/src/menu/linux.rs` — mirrored item with unique mnemonic.

**Frontend (Svelte/TS)**

- `src/lib/whats-new/WhatsNewDialog.svelte` — **new.** ModalDialog host: version blocks, section rendering, footer
  buttons, opt-out toast.
- `src/lib/whats-new/whats-new.ts` — **new.** `shouldShowWhatsNew` pure helper + the startup/onboarding-close trigger
  - `lastSeenVersion` bookkeeping.
- `src/lib/whats-new/CLAUDE.md` — **new.** Show-once semantics, the no-backlog-on-re-enable decision, manual smoke
  checklist.
- `src/lib/ui/dialog-registry.ts` — add `'whats-new'`.
- `src/lib/settings/settings-registry.ts` — the visible `whatsNew.showOnUpdate` entry + hidden
  `whatsNew.lastSeenVersion`.
- `src/lib/commands/command-ids.ts` + `command-registry.ts` — `help.whatsNew` entry.
- `src/routes/(main)/command-dispatch.ts` + `+page.svelte` — dispatch case, `showWhatsNewDialog` state, `{#if}` mount,
  inclusion in the any-modal-open guard, idempotent-open guard (`if (show && showWhatsNewDialog) return` — the menu
  double-dispatch gotcha from go-to-path-plan.md).
- `src/lib/shortcuts/shortcuts-store.ts` — `help.whatsNew` in `menuCommands`.

## Milestones

### M1 — Backend: parser + IPC

`whats_new/mod.rs` with full unit tests against a fixture changelog string (NOT the live file, so tests don't churn with
every release) plus one smoke test over the real embedded changelog (parses, ≥ 20 releases, current version present —
the real-file test is the drift alarm if the changelog format ever changes). `commands/whats_new.rs`, registration,
`pnpm bindings:regen`. No UI.

### M2 — Frontend: dialog + trigger + setting

Dialog component, trigger wiring, both settings entries, dialog-registry entry. Vitest for `shouldShowWhatsNew` and the
lastSeen bookkeeping (first-run, upgrade, downgrade, disabled paths). Verify in `pnpm dev` by setting
`whatsNew.lastSeenVersion` to an old version in the dev `settings.json` and relaunching.

### M3 — Menu + palette + E2E + docs

The four-places menu/command pass, one Playwright spec, CLAUDE.md files, `docs/architecture.md` rows, full `pnpm check`.

## Testing

- **Rust unit (M1), fixture-based:** heading recognition incl. Unreleased skip and dev-history cutoff; lead extraction
  (present, absent, multi-paragraph); Non-app dropped; unknown section dropped; commit-link group stripped while keeping
  inline code/bold; non-commit links flattened to text; `releases_between` slicing incl. since=current (empty), since
  older than oldest (all), since missing/garbage (treat as show-current-only and log); semver ordering with double-digit
  components (0.9.0 < 0.10.0).
- **Vitest (M2):** `shouldShowWhatsNew` truth table; first-run silent stamp; downgrade rewrite; disabled-still-stamps;
  opt-out button writes the setting and fires the toast (mocked IPC).
- **Component a11y test** for the dialog (tier-3 suite; the `a11y-coverage` check will demand it).
- **Playwright (M3), one spec:** launch with pre-seeded settings (`isOnboarded: true`,
  `whatsNew.lastSeenVersion: '0.1.0'`), assert the dialog appears with at least one version block, click "Not interested
  in changelogs", assert it closes and the setting flipped; relaunch-shows-nothing can be asserted cheaply via the
  stamped `lastSeenVersion`. All other E2E specs stay popup-free automatically via the first-run rule — verify one
  unrelated spec still passes before calling M3 done.

## Docs to update

- **New:** `src/lib/whats-new/CLAUDE.md`, `src-tauri/src/whats_new/CLAUDE.md`.
- `docs/architecture.md`: frontend `whats-new/` and backend `whats_new/` rows; Help-menu note in the `menu/` row.
- `src-tauri/src/menu/CLAUDE.md` (or its DETAILS.md): the new Help item.
- `.claude/commands/release.md`: one line noting the changelog now ships in-app, so the self-edit pass is also a UI copy
  review.

## Trade-offs and gotchas

- **The changelog is now UI copy.** Whatever lands in Added/Changed/Fixed/Security renders inside the app verbatim. The
  release flow's audience contract and self-edit pass already account for this; the parser must NOT grow "fix up bad
  entries" logic. Garbage in the popup gets fixed in CHANGELOG.md.
- **Parser resilience over strictness.** A malformed future section must never panic or block startup: skip what doesn't
  parse, log at debug, show what does. The smoke test over the real file is the canary.
- **`include_str!` path fragility.** The relative path from the module to the repo root breaks silently-at-compile-time
  if files move — which is the good failure mode (compile error, not runtime). Verify the path depth during M1.
- **Don't advance `lastSeenVersion` when the popup was skipped for another startup modal.** Otherwise a crash-report
  launch eats the what's-new forever. The stamp happens in exactly three places: first-run, downgrade, and
  popup-shown-or-feature-disabled.
- **Idempotent open.** Menu + palette + automatic trigger can race in principle; the standard `if already open, return`
  guard makes double-dispatch harmless (documented pattern, see go-to-path-plan.md § menu double-dispatch).
- **Dev-mode staleness** of the embedded changelog (no rebuild on markdown edit) is by design; don't "fix" it by adding
  the file to the Tauri watcher — that would restart the app on every changelog edit (see the `.taurignore` rationale in
  AGENTS.md § Debugging).
