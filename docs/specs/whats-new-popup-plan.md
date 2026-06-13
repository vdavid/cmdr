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

- **Content is offline; one outbound link.** The popup renders the changelog embedded in the running binary, so the
  notes always match the version and work with no network. The single exception is a **"See full changelog"** button
  that opens [getcmdr.com/changelog](https://getcmdr.com/changelog/) in the browser. We never render the full changelog
  in-app: the popup shows at most the latest five releases, and "older than that" lives on the website by design.
- **Display caps at five releases.** Both the automatic post-update diff and the manual Help reopen show at most the
  five newest in-range releases, newest first. A user who skipped eight versions sees the latest five plus the "See full
  changelog" link, never an eight-deep dump. The whole changelog is still embedded (≈150 KB, noise next to the bundled
  llama-server); the cap is a display rule, not a build-time slice, so there's no pipeline step.
- **No images, no per-release marketing.** v1 renders the existing markdown (lead + bulleted sections). If a release
  ever deserves a hero screenshot, that's a future iteration.
- **No Linux/Windows forks needed.** The dialog is a plain soft modal; nothing platform-specific beyond the Help menu
  registration that already forks per platform.
- **No backlog replay on re-enable.** When the feature is off, `lastSeenVersion` still advances silently on every
  launch. Re-enabling shows future updates only, never a months-deep dump.

## Confirmed behavior (the contract)

### When the popup shows

On main-window startup, after settings load. No `whatsNew.lastSeenVersion` stored means "never stamped", which happens
in exactly two situations the rules below disambiguate via the existing `isOnboarded` setting: a genuine fresh install
(not onboarded) versus an existing user launching for the first time after the release that introduces this feature
(onboarded).

1. **Fresh install** (no `lastSeenVersion`, not onboarded): silently stamp `lastSeenVersion` to current and never show
   the popup. Fresh installs get onboarding, not a changelog. This also keeps every E2E run popup-free by default (fresh
   instance data dir → not onboarded → this rule).
2. **Inaugural showcase** (no `lastSeenVersion`, already onboarded): the user updated _into_ the feature, so show it
   off. Treat the effective `lastSeen` as the release just before current, i.e. show the **current version only**, then
   stamp. If the feature is already off, stamp silently with no popup. This is the one case where "no stored version"
   still shows a popup, and it's deliberate: otherwise the release that ships this feature would never demonstrate it.
3. **Version unchanged**: do nothing.
4. **Downgrade**: rewrite `lastSeenVersion` to current (so a later re-upgrade behaves sanely), no popup.
5. **Version increased**: if `whatsNew.showOnUpdate` is on, open the dialog showing the released sections
   `lastSeen < v <= current`, newest first, capped at five, then stamp. If the feature is off, stamp silently (the
   no-backlog-on-re-enable rule).

Sequencing: never over onboarding. Gate on the same mechanism the update toast uses (`onboardingShowing` + re-attempt
when onboarding closes, see `updater.svelte.ts` `shouldShowUpdateToast`). If another startup modal (crash report prompt,
expiration modal) is already up, a popup that _would show_ waits for the next launch rather than queueing, and
`lastSeen` is NOT advanced until the popup actually ran. The silent-stamp paths (fresh install, downgrade,
feature-disabled) don't show a modal, so they stamp immediately regardless of other modals. Missing one launch is fine;
double-showing or stacking modals is not.

If the would-show diff resolves to zero displayable releases (every in-range release dropped per the Non-app/empty rule
below), the **automatic** popup does not open: it stamps and moves on. An empty result only ever surfaces as a visible
empty state through the manual Help entry, never as an empty auto-popup.

### The dialog

- A **soft modal** (`ModalDialog`, draggable, with `onclose`), `dialogId: 'whats-new'` registered in
  `SOFT_DIALOG_REGISTRY`. Sized roughly like the Search dialog: comfortable reading width (~560 px), body scrolls,
  max-height capped to the window.
- Title: **"What's new in Cmdr"**. Body, per release, newest first (at most five):
  - A version heading: `0.25.0 · 2026-06-11` (version prominent, date subtle).
  - The prose lead as an intro paragraph.
  - The Added / Changed / Fixed / Security sections that exist, as small subheadings with bulleted entries. Non-app
    never appears. Commit-link groups are already stripped by the parser (see below); remaining inline markdown (bold,
    `code`, quotes, ⌘-symbols) renders via the existing `snarkdown` path.
- At the bottom of the scrolling body, after the last release block, a **"See full changelog"** `LinkButton` that calls
  `openExternalUrl('https://getcmdr.com/changelog/')` (the same external-open path the About window uses; Tauri blocks
  raw `<a>` navigation). Always present, including in the empty state.
- **Empty state** (zero displayable releases, only reachable via the manual Help entry): a short "Nothing to see here"
  line plus the "See full changelog" link. (Copy is a draft; David reviews all user-facing strings.)
- Footer: **"Not interested in changelogs"** as a subtle text button (LinkButton style, `--color-text-secondary`)
  bottom-left; **"Close"** as the primary button bottom-right. Esc closes. No other actions.
- "Not interested in changelogs" sets `whatsNew.showOnUpdate = false`, closes the dialog, and fires a default-level
  toast: "Got it, no more update notes. Re-enable them anytime in Settings > Updates & privacy." (Copy is a draft; David
  reviews all user-facing strings per the humans-to-humans principle.)
- The five-release cap means the scroll never grows unbounded; the "See full changelog" link carries anything older.

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
  **latest five** releases (no lower bound), with the "See full changelog" link for the rest. A manual reopen never
  stamps `lastSeenVersion`. Works regardless of the `showOnUpdate` setting; the setting governs only the automatic
  popup. If nothing is displayable (e.g. a build whose recent releases all dropped to nothing), it opens the empty state
  rather than failing to open.
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

- Recognize `## [x.y.z] - YYYY-MM-DD` release headings, walking top-down. Skip the top `## [Unreleased]` block (no date,
  not a release).
- Per release: capture the **lead** (prose paragraphs between the heading and the first `###`), and the Added / Changed
  / Fixed / Security sections. **Drop Non-app entirely.** Tolerate absent sections and unknown section names (drop those
  too, log a debug line). A release that ends up with no lead AND no displayable section is omitted from the result.
- Per entry: join continuation lines, then strip the trailing commit-link group: the
  ` ([hash](https://github.com/vdavid/cmdr/commit/…), …)` parenthetical. Hashes run **6–8 hex chars** and a single entry
  can carry **several comma-separated links wrapped across lines**, so the stripper matches a variable-length,
  multi-link trailing group, not a fixed single 8-char link. Convert any other markdown link to its plain text (the
  webview never navigates in-dialog; the only outbound link is the dedicated "See full changelog" button). Keep
  bold/italic/code/quotes verbatim for the frontend renderer.
- Version comparison via the `semver` crate (already a direct dependency of `src-tauri`, used by the updater).
- **No `### Development history` cutoff is needed.** Because the slice never exposes more than the five newest releases
  and the diff is always recent, the parser stops after at most five releases (current down to `since`, or five,
  whichever is fewer) and never walks as deep as the `### Development history` block in the oldest section. Don't add
  special handling for it; if a future refactor ever parses the whole file eagerly, that's when to revisit.

"Current" is `env!("CARGO_PKG_VERSION")` (the same source the updater compares), which in a tagged build always equals
the top _released_ `## [x.y.z]` section, never `[Unreleased]`.

```rust
#[derive(Serialize, Type)]
#[serde(rename_all = "camelCase")]
struct WhatsNewRelease {
    version: String,          // "0.25.0"
    date: String,             // "2026-06-11" (display string, no parsing needed)
    lead: Option<String>,     // markdown
    sections: Vec<WhatsNewSection>, // { title: "Added" | "Changed" | "Fixed" | "Security", entries: Vec<String> }
}
```

IPC: `get_whats_new(since_version: Option<String>, max: u32) -> Vec<WhatsNewRelease>`, returning the displayable
releases `since < release <= current`, newest first, truncated to `max`. `since_version = None` means no lower bound
(the latest `max`). Call sites:

- **Upgrade diff:** `get_whats_new(Some(lastSeen), 5)`.
- **Inaugural showcase:** `get_whats_new(None, 1)` (current only).
- **Manual Help reopen:** `get_whats_new(None, 5)`.

Thin pass-through command per the thin-IPC principle; the parsing and slicing logic lives in a `whats_new/` module with
plain unit-testable functions. `pnpm bindings:regen` required.

### Trigger logic is a pure function

Frontend keeps a tiny pure
`decideWhatsNew({ lastSeen, current, enabled, onboarded, onboardingShowing, otherStartupModalOpen })` helper plus a thin
effect that calls it on startup and on onboarding close (mirroring the update-toast re-attempt). It returns one of:

- `{ action: 'show', since, max }` — open the dialog with that slice, then stamp `lastSeen = current`.
  (`since=null, max=1` for the inaugural showcase; `since=lastSeen, max=5` for a normal upgrade.) If the fetched slice
  is empty, the effect treats it as `stamp` instead of opening an empty auto-popup.
- `{ action: 'stamp' }` — write `lastSeen = current`, no dialog (fresh install, downgrade, or feature-disabled).
- `{ action: 'wait' }` — onboarding or another startup modal is up; do nothing and don't stamp, retry next launch / on
  onboarding close.
- `{ action: 'none' }` — version unchanged.

Modal gating (`onboardingShowing`/`otherStartupModalOpen`) only blocks the `show` actions; the silent `stamp` paths run
regardless. All state via the existing `getSetting`/`setSetting`; no new persistence mechanism. The manual Help command
bypasses this helper entirely: it always fetches `get_whats_new(None, 5)`, opens the dialog (empty state if the slice is
empty), and never stamps.

### Dev simulation: forcing the startup popup

`CMDR_SIMULATE_UPDATE_FROM=0.22.0` makes a dev session behave as if the app just updated from that version, so the
startup popup is visible without hand-editing the dev `settings.json`. The env var is a backend-process value invisible
to the Vite frontend, so the backend reads it and surfaces it through a tiny `whats_new_dev_override()` command. When
set, the startup effect bypasses `decideWhatsNew` and forces the show path: it diffs from the given version
(`get_whats_new(Some(v), 5)`), opens the dialog regardless of the `showOnUpdate` setting, onboarding, or modal gates,
and **does not stamp** `lastSeenVersion`, so every relaunch shows it again until the var is unset. Fits the existing
`CMDR_MOCK_LICENSE` / `CMDR_MOCK_FDA` / `CMDR_FORCE_ONBOARDING` dev-flag family. Document it in AGENTS.md § Debugging
and the module CLAUDE.md.

### Rendering

The dialog reuses `renderErrorMarkdown()`'s `snarkdown` path for entry/lead markdown (`{@html}` is safe here: the
content is our own committed changelog, not user input — same trust level as `FriendlyError`'s `md!` output). Styling
follows the design system: tokens only, sentence case, dark/light both verified, `prefers-reduced-motion` respected (no
entrance animation beyond the standard ModalDialog one). The "See full changelog" button uses the shared
`openExternalUrl()` command + `LinkButton`; add the changelog URL as a named constant near its use (or in
`lib/beta-links.ts` if it fits the shared-links set).

## Critical files

**Backend (Rust)**

- `src-tauri/src/whats_new/mod.rs` — **new.** `include_str!`, parser (heading/lead/section/entry extraction,
  link-stripping, Non-app drop, empty-release drop), `releases_between(since, current, max)` slicing, `OnceLock` cache,
  unit tests.
- `src-tauri/src/whats_new/CLAUDE.md` — **new.** Parser contract, dev-staleness note, the "changelog is the source of
  truth; fix formatting there, not here" guardrail.
- `src-tauri/src/commands/whats_new.rs` — **new.** Thin `get_whats_new` IPC plus `whats_new_dev_override()` (reads
  `CMDR_SIMULATE_UPDATE_FROM`); register both in `generate_handler!` + specta builder.
- `src-tauri/src/menu/mod.rs` — `HELP_WHATS_NEW_ID`, both `menu_id_to_command` (→ `help.whatsNew`, `CommandScope::App`)
  and `command_id_to_menu_id` mappings; dispatch test list.
- `src-tauri/src/menu/macos.rs` — Help menu item "What's new" above Send feedback; SF Symbol if one fits (e.g.
  `sparkles`); exact-title-match gotcha applies.
- `src-tauri/src/menu/linux.rs` — mirrored item with unique mnemonic.

**Frontend (Svelte/TS)**

- `src/lib/whats-new/WhatsNewDialog.svelte` — **new.** ModalDialog host: version blocks, section rendering, empty state,
  "See full changelog" link, footer buttons, opt-out toast.
- `src/lib/whats-new/whats-new.ts` — **new.** `decideWhatsNew` pure helper + the startup/onboarding-close trigger
  effect + `lastSeenVersion` bookkeeping + the `CMDR_SIMULATE_UPDATE_FROM` override short-circuit (via
  `whats_new_dev_override()`).
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
every release) plus one smoke test over the real embedded changelog (the latest five releases parse cleanly, the current
version is present and is the newest, each has a lead — the real-file test is the drift alarm if the changelog format
ever changes). `commands/whats_new.rs`, registration, `pnpm bindings:regen`. No UI.

### M2 — Frontend: dialog + trigger + setting

Dialog component (incl. empty state and "See full changelog" link), trigger wiring, both settings entries,
dialog-registry entry. Vitest for `decideWhatsNew` and the lastSeen bookkeeping (fresh-install, inaugural, upgrade,
downgrade, disabled, wait paths). Verify in `pnpm dev` by setting `whatsNew.lastSeenVersion` to an old version in the
dev `settings.json` and relaunching.

### M3 — Menu + palette + E2E + docs

The four-places menu/command pass, one Playwright spec, CLAUDE.md files, `docs/architecture.md` rows, full `pnpm check`.

## Testing

- **Rust unit (M1), fixture-based:** heading recognition incl. `[Unreleased]` skip; lead extraction (present, absent,
  multi-paragraph); Non-app dropped; unknown section dropped; release with no lead and only Non-app omitted; commit-link
  group stripped while keeping inline code/bold, incl. variable-length hashes (6–8 chars) and multi-link groups wrapped
  across lines; non-commit links flattened to text; `releases_between` slicing incl. `max` cap (skip-8-show-5),
  since=current (empty), since older than oldest (all in range, still capped), since=None with max=1 (current only) and
  max=5 (latest five), since missing/garbage (treat as no-lower-bound and log); semver ordering with double-digit
  components (0.9.0 < 0.10.0).
- **Vitest (M2):** `decideWhatsNew` truth table across (lastSeen, current, enabled, onboarded, onboardingShowing,
  otherStartupModalOpen): fresh-install silent stamp; inaugural showcase (onboarded, no lastSeen) → show current only;
  inaugural-disabled → stamp; upgrade → show with `max:5`; downgrade rewrite; disabled-still-stamps; modal-up →
  wait-no-stamp; empty-slice on a show action collapses to stamp; dev override (`CMDR_SIMULATE_UPDATE_FROM`) forces show
  and never stamps; opt-out button writes the setting and fires the toast (mocked IPC).
- **Component a11y test** for the dialog (tier-3 suite; the `a11y-coverage` check will demand it).
- **Playwright (M3), one spec:** launch with pre-seeded settings (`isOnboarded: true`,
  `whatsNew.lastSeenVersion: '0.1.0'`), assert the dialog appears with at least one version block and no more than five,
  click "Not interested in changelogs", assert it closes and the setting flipped; relaunch-shows-nothing can be asserted
  cheaply via the stamped `lastSeenVersion`. All other E2E specs stay popup-free automatically: a fresh instance data
  dir is not onboarded, so the fresh-install rule silent-stamps. Verify one unrelated spec still passes before calling
  M3 done.

## Docs to update

- **New:** `src/lib/whats-new/CLAUDE.md`, `src-tauri/src/whats_new/CLAUDE.md`.
- `docs/architecture.md`: frontend `whats-new/` and backend `whats_new/` rows; Help-menu note in the `menu/` row.
- `src-tauri/src/menu/CLAUDE.md` (or its DETAILS.md): the new Help item.
- `.claude/commands/release.md`: one line noting the changelog now ships in-app, so the self-edit pass is also a UI copy
  review.
- `AGENTS.md` § Debugging: document `CMDR_SIMULATE_UPDATE_FROM=<version>` as the way to force the startup popup.

## Trade-offs and gotchas

- **The changelog is now UI copy.** Whatever lands in Added/Changed/Fixed/Security renders inside the app verbatim. The
  release flow's audience contract and self-edit pass already account for this; the parser must NOT grow "fix up bad
  entries" logic. Garbage in the popup gets fixed in CHANGELOG.md.
- **Parser resilience over strictness.** A malformed future section must never panic or block startup: skip what doesn't
  parse, log at debug, show what does. The smoke test over the real file is the canary.
- **`include_str!` path fragility.** The relative path from the module to the repo root breaks silently-at-compile-time
  if files move — which is the good failure mode (compile error, not runtime). Verify the path depth during M1.
- **Don't advance `lastSeenVersion` when a popup that would show was skipped for another startup modal** (the `wait`
  action). Otherwise a crash-report launch eats the what's-new forever. Stamping happens on every other resolved action:
  fresh install, inaugural showcase, downgrade, upgrade-shown, and feature-disabled.
- **The inaugural showcase hangs on `isOnboarded`, not on the version key alone.** "No `lastSeenVersion`" can't tell a
  fresh install from an existing user updating into the feature; `isOnboarded` is the discriminator. Get this backwards
  and either every fresh install eats a changelog popup, or the release that ships the feature never demonstrates it.
- **Idempotent open.** Menu + palette + automatic trigger can race in principle; the standard `if already open, return`
  guard makes double-dispatch harmless (documented pattern, see go-to-path-plan.md § menu double-dispatch).
- **Dev-mode staleness** of the embedded changelog (no rebuild on markdown edit) is by design; don't "fix" it by adding
  the file to the Tauri watcher — that would restart the app on every changelog edit (see the `.taurignore` rationale in
  AGENTS.md § Debugging).
