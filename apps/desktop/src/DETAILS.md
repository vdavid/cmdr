# Details

Depth and rationale for this area. `CLAUDE.md` holds only the must-knows that prevent silent breakage; everything else
(architecture narrative, data flows, decision rationale, edge-case catalogs) lives here.

## Leading

One scale for the whole app: the four `--font-line-height-*` tokens, picked per surface in `docs/design-system.md` §
Leading. The text surfaces (`.modal-dialog`, `.toast`, the sheet, the secondary windows) INHERIT `normal` from
`app.css`, which is why a component usually writes no `line-height` at all. The main window and the file lists
deliberately inherit no ratio: their rows size from the density tiers, and a ratio there would fight them.

## Dev gates

A `.svelte` script gates dev-only behavior on `isDevBuild()` (`#lib/app-mode`), never on an inline
`import.meta.env.DEV`. knip collects a Svelte file's imports with a regex, and a bare `import.meta` matches its import
pattern: the match runs on to the next quoted string, the synthetic module knip parses comes out broken, and every
`await import(...)` in that file disappears from its graph. One `import.meta.env.DEV` in `routes/(main)/+page.svelte`
was enough to make knip call `lib/debug/debug-window.ts` an unused file while the app was opening the debug window from
it (knip 6.36.0, `dist/compilers/compilers.js` § `importMatcher`, verified 2026-09-21). A `.ts` file is parsed by
TypeScript and may write `import.meta.env.DEV` directly.

`{#if}` MARKUP is the exception and keeps the literal (`routes/(main)/+layout.svelte`): Vite replaces
`import.meta.env.DEV` at build time, so the whole dev-only subtree leaves the prod bundle. A function call survives
minification, so gating markup on `isDevBuild()` would ship the dialog gallery and the dev pages to users.

## Glass material

Every frosted-glass surface (menus, selects, popovers, toasts, tooltips) draws from the tokens in `app.css` §
Frosted-glass material, and two per-window modules feed them the two macOS settings that shape it:

- **Reduced transparency.** WKWebView never reflects `@media (prefers-reduced-transparency)`, so the app can't key a
  fallback off it. The backend reads the `NSWorkspace` value and `#lib/display-accessibility` toggles an
  `html.reduce-transparency` CLASS instead (see § Display accessibility classes). Under it, `app.css` § Reduced
  transparency flips `--color-bg-glass` / `--color-bg-glass-steady` / `--color-border-glass` to opaque and
  `--glass-backdrop` to `none`, so a surface using the tokens needs no rule of its own. `prefers-reduced-motion`
  WKWebView does honor, so that one stays a media query.
- **The Liquid Glass slider** (macOS 27 Appearance). The backend reads the undocumented `NSGlassTintAmount` global
  default, re-reading it each time the app becomes active (`apps/desktop/src-tauri/src/glass_tint.rs` has why no
  notification works), and `#lib/glass-material` sets `--glass-tint` (0 clearest to 1 most tinted, 0.5 when macOS
  reports none). The glass fill's opacity and the blur grow with it, as native menus do. Toasts ride it with a higher
  opacity floor (`--glass-toast-opacity`), since they hold paragraphs over busy lists; tooltips and the unblurred Ask
  Cmdr drop hint use the fixed `--color-bg-glass-steady` instead.

**Gotcha: the MCP bridge's `webview_screenshot` never paints `backdrop-filter`.** Every glass surface reads as bare
translucency in it, with the text underneath sharp through the fill, so a menu or popover over a busy list (a dialog's
preview list, say) looks broken while the real window blurs it fine. **Why it matters:** it reads like the nested
`backdrop-filter` trap (a filter whose ancestor has its own backdrop root sees only that ancestor's content), and an
agent will "fix" a surface that isn't broken. A menu inside a `ModalDialog` portals into the overlay, which carries no
`backdrop-filter` unless the dialog sets `blur`, and even then the panel sits inside that root, so the menu still blurs
the panel. Judge glass on a real display, never from that screenshot. (Verified in the dev app on macOS 27: a body-level
test div with `backdrop-filter: blur(20px)` above everything left the text under it pixel-sharp in the capture,
2026-10-09.)

Menu-like surfaces (`Menu`, `Select`, the breadcrumb popup) also take the macOS 26+ menu shape: `--radius-menu`,
`--shadow-glass` plus the `--shadow-glass-rim` top highlight, and rows highlighted as inset pills.

## Display accessibility classes

`#lib/display-accessibility` (inited per window) mirrors four macOS Accessibility > Display options onto `<html>` as
classes: `reduce-transparency`, `increase-contrast`, `differentiate-without-color`, and `show-borders`. The backend
reads all four from `NSWorkspace` in one typed `DisplayAccessibility` value and re-reads them on every
`NSWorkspaceAccessibilityDisplayOptionsDidChangeNotification` (`apps/desktop/src-tauri/src/display_accessibility.rs`),
so they follow System Settings live. The same value carries Invert colors, which gets no class: it only changes the
icons macOS draws, which `#lib/icon-cache` follows itself (`apps/desktop/src-tauri/src/icons/DETAILS.md` § Flushes and
in-flight fetches).

- **Why classes**: WKWebView ignores `prefers-reduced-transparency` (verified, see the module doc), and CSS has no query
  for Differentiate without color or Show borders at all. `increase-contrast` is ALSO on when `prefers-contrast: more`
  matches, so a webview that reflects it (Linux) gets the same styling. Style off the class, ❌ never the media query:
  one source keeps the two from drifting.
- **Show borders has no public API**: the backend calls an undeclared `NSWorkspace` getter behind a
  `respondsToSelector:` guard (evidence in `display_accessibility.rs`). A standalone button that draws no edge of its
  own carries `.borderless-control` and gains a bezel under `html.show-borders` (`app-utilities.css` § Show borders).
  Give a new borderless standalone button the class, but ❌ not inline items (path segments, a tab's close button):
  macOS leaves those bare, and boxing each made the path bar read as broken.

## Window drag strips sit at `--z-sticky`, under every menu

Each secondary window (settings, debug, shortcuts, queue) paints an invisible `.window-drag-region` over its top strip
so the user can move the window from the empty space beside the traffic lights. It's absolutely positioned, so it
already paints over the in-flow layout — a rung is only needed to beat other POSITIONED chrome, and `--z-sticky` is as
high as it should ever go. All four sat at `--z-dropdown` once, which put an invisible strip on the same rung as the
app's menus and let it swallow clicks on their top rows: the AI provider pop-up in settings opens over its trigger, so
its first two options landed under the strip and couldn't be picked. Anything a menu can open into belongs BELOW
`--z-dropdown`. (A menu clears the strip only if its own rung really applies, which depends on the element carrying it:
`lib/ui/DETAILS.md` § Select.)

## Global stylesheets

Seven sheets, all global (no Svelte scoping). Who owns what:

- `app.css`: the design tokens (`:root`, plus the dark-mode, Increase contrast, reduced-transparency, and old-WebKit
  override blocks), then the base element styles (focus ring, typography layer, `html` / `body`, `#app-root`).
- `app-palette.css`: the static Tailwind color scale. Scheme-independent reference data with no `var()` dependencies, so
  it's order-independent.
- `app-reset.css`: the ress-derived reset, inside `@layer ress-reset`.
- `app-field.css`: the `.text-field*` chrome behind `lib/ui/TextInput.svelte` and `lib/ui/TextArea.svelte`. Nothing else
  may use those classes.
- `app-utilities.css`: class-per-value utilities applied via class bindings (`.size-*` size tiers, `.age-*` date ages,
  `.spinner*`).
- `app-tooltip.css`: `.cmdr-tooltip`, the singleton element the tooltip action creates.
- `app-file-list.css`: the `.file-entry` row chrome that `FullList.svelte` and `BriefList.svelte` share (stripe,
  selection fill, selected-row hairline, the marked-file bar and text stroke, cursor fill and outline, the unfocused
  pane's dashed cursor outline) and the lists' always-visible classic scrollbars. Only rules that were identical in both
  views live here; see `lib/file-explorer/views/DETAILS.md` for what stayed per-view and why.

### Cascade order is load order, and it's manual

`app.css` `@import`s the two order-independent sheets (palette, reset) at its top. The other four are imported from
`routes/+layout.svelte`, in a fixed order, AFTER `app.css`.

**Why:** a CSS `@import` must sit at the top of its sheet, so `@import`-ing a sheet whose rules belong at the END would
put them BEFORE everything that precedes them today. Wherever specificity ties, the winner flips, and the regression is
invisible until someone notices a wrong border weeks later. Importing from the layout is what reproduces the original
single-file order exactly. So: don't convert those four to `@import`s, and don't reorder the imports in the layout.

### Moving a rule out of a component costs a class of specificity

Svelte scopes a component rule by appending `.svelte-<hash>` to its FIRST compound selector (the rest get a
zero-specificity `:where(.svelte-<hash>)`). So lifting `.file-entry.is-selected` out of a `<style>` block drops it from
(0,3,0) to (0,2,0), and it starts tying with things it used to beat. `app-file-list.css` pays this back by prefixing
every selector with the view's container class (`.full-list-container` / `.brief-list-container`), which restores the
original specificity exactly. Without that, `DualPaneExplorer`'s `:global(.file-entry.folder-drop-target)` (also
(0,2,0), and emitted LATER: component styles ride the route chunk, these sheets ride the root-layout chunk) would win
the tie and paint the drag-over highlight over the selection and cursor fills.

Load order between the two chunks is real but should never be load-bearing: keep a lifted rule's specificity at or above
what it had inside the component.

The dark-mode / Increase contrast / reduced-transparency blocks in `app.css` are order-load-bearing for the same reason
(they override the light defaults above them). Leave them where they are.

**Verifying a move.** Vite's content hashes make this cheap: build the frontend before and after (`vite build` in
`apps/desktop`), then compare `build/_app/immutable/assets/*.css`. A move between two global sheets leaves every emitted
file byte-identical, hashed filenames included. A move OUT of a component rewrites selectors, so compare declarations
instead: extract every `(selector, property, value)` triple from the before and after bundles, normalize away the
`.svelte-<hash>` classes and the added container prefix, and diff the sets. Anything that shows up on one side only is a
rule you changed, not moved.

## Error boundaries

A component that throws while it renders, runs an effect, or tears down must never blank a window. Four layers of
`<svelte:boundary>` make sure it can't, from the outside in:

- **Window**: `#lib/error-reporter/WindowBoundary.svelte`, wrapping everything in `routes/+layout.svelte`. Every window
  (main, Settings, viewer, queue, shortcuts, debug, the `/dev` pages) renders through that one layout, so one boundary
  covers them all. Its failed state is `PageCrashPanel` ("Something went wrong in this window", technical details, a
  report offer, Reload). Reload first re-arms in place with `reset()`, keeping the window's backend subscriptions; if
  the window breaks again within five seconds of that, the next Reload reloads the webview.
- **SvelteKit's per-route boundaries**: SvelteKit puts one around each route node below the root layout, and renders
  `routes/+error.svelte` (the same `PageCrashPanel`) when a page throws while rendering. They catch most page throws
  before the window boundary does; the window boundary takes the rest.
- **Pane**: `#lib/file-explorer/pane/PaneBoundary.svelte`, one per file pane in `DualPaneExplorer`'s `paneBlock`. A
  broken pane shows "This pane hit a problem" with Reload while the other pane, the tab bar, and every dialog keep
  working. Switching tabs in the broken pane re-arms it too.
- **Dialog**: `#lib/ui/boundary/DialogBoundary.svelte`. A broken dialog closes (through the host's `onClose`), a toast
  says so, focus goes home (`focusHome()`), and the boundary re-arms on the next tick, capped at three re-arms inside
  five seconds so a host whose close doesn't close can't spin. `DialogManager` puts ONE around every pane dialog, with
  an `onClose` that dismisses them all, because a stray `show*` flag suppresses the pane's keys
  (`lib/file-explorer/pane/DETAILS.md` § "A dialog that throws during render must never wedge input"). Every dialog a
  route mounts gets its own.

**Why a boundary has to sit ABOVE what it guards.** Svelte routes a throw from an effect's teardown to the nearest LIVE
ancestor boundary and skips one that is itself being destroyed (`execute_effect_teardown` → `invoke_error_boundary`,
verified on Svelte 5.57.2 by reading `internal/client/reactivity/effects.js` and `error-handling.js`, 2026-10-22). So a
dialog's boundary wraps the `{#if}` that mounts it, and the pane's wraps the per-tab `{#key}` that remounts it: a
boundary inside them is torn down along with what it guards, and the throw goes over its head to the window. That's how
a `derived_references_self` thrown while the Multi-rename sheet closed blanked the main window.

**The rule for new code.** A new window route needs nothing: the root layout covers it. A new dialog mounted from a
route goes in its own `<DialogBoundary name=… onClose=…>` around its `{#if}`; `cmdr/dialog-inside-boundary`
(`apps/desktop/eslint-plugins/dialog-inside-boundary.js`) fails the lint on one that isn't. `onClose` clears whatever
state mounts the dialog, usually its own close handler. Leave it out only for a dialog that owns its visibility and
starts closed when remounted (`StaleDriveDialog`). A dialog holding a paused operation's question
(`OperationConflictDialog`, `SourcesGonePrompt`, `LockedSourcesPrompt`) closes by cancelling that operation, since
nothing can answer it once its dialog is gone; the quit prompt calls the quit off.

**Reporting.** Each boundary's `onerror` logs the throw at error level, naming the boundary that caught it, so it lands
in the log file, error-report bundles, and an opted-in auto-send. SvelteKit also hands every boundary throw to
`hooks.client.ts`, which logs it with its frames and turns it into the `{ message, detail }` the boundary receives
(`lib/logging/DETAILS.md`). Boundaries never see throws in event handlers, timers, or promise callbacks: those reach the
global `app·uncaught` logger (`lib/logging/uncaught-errors.ts`) and leave the UI as it was.

Tests: `DialogBoundary.svelte.test.ts`, `PaneBoundary.svelte.test.ts`, and `WindowBoundary.svelte.test.ts` each mount a
component that throws while rendering and one that throws while tearing down (`test/fixtures/`), and assert the host
survives and reports.
