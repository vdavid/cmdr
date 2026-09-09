# macOS Dock integration: implementation map

Survey of the seams a macOS Dock integration needs, so an agent can write the code without re-deriving where things
live. Every path is repo-relative.

What is being built (four pieces):

1. ✅ **Landed.** A launch-day ledger: `usage.json` in the app data dir, local calendar days, appended once per launch
   by Rust at startup. Now documented at `apps/desktop/src-tauri/src/usage/CLAUDE.md`, which is the authority; the
   envelope key is `_schemaVersion`, per the house convention rather than this file's original `schemaVersion`.
2. ✅ **Landed.** A one-time "add Cmdr to your Dock" nudge. Authority: `apps/desktop/src-tauri/src/dock/CLAUDE.md` (the
   machine-facing half) and `apps/desktop/src/lib/dock/CLAUDE.md` (the decision, the toast, the events). Conditions (b)
   and (c) below turned out to be one answer, `DockPinState`, not two; both Applications folders count, not just
   `/Applications`.
3. ⏳ **Not started.** A Dock tile context menu, live only while the app runs (no `NSDockTilePlugIn`). §§ A and B are
   still the map for it, and are the reason this file is still here.
4. ✅ **Landed** with piece 2. Three PostHog events: `dock_pin_offered`, `dock_pin_answered` (`answer`:
   yes|no|dismissed), `dock_pin_failed` (typed reason).

**Where a landed piece and this file disagree, the colocated `CLAUDE.md` / `DETAILS.md` wins.** The corrections below
are the ones that cost real time to rediscover; everything else in §§ C–G is now background.

## Corrections from building pieces 1, 2, and 4

- **§ D's suggested single `$lib/dock/` module is an import cycle.** One module can't both raise the toast (importing
  the `*ToastContent.svelte`) and hold the answer handlers (which the component imports). `import-cycles` fails it. The
  shipped split is `dock-nudge.ts` (raise) + `dock-pin-answer.ts` (answers), which is also what `open-terminal/` already
  does. § E's "frontend, same file" for all three events is wrong for the same reason.
- **§ D's toast API summary omits `onDismiss`**, which is the seam that makes a frame-× dismissal distinguishable from
  an active "No". It's in `toast-store.svelte.ts`, and it fires ONLY on the × — never on a timeout, never on a
  programmatic `dismissToast` — so a component's own buttons can't contaminate it. Without it you'd be reduced to
  inferring a dismissal from unmount, which also fires on quit.
- **§ G's lane list is missing `i18n-coverage`, the biggest hidden cost of any new user-facing string.** It's
  ERROR-level and fails until every full-translation locale carries the new key, so adding English copy pulls in the
  whole translator process (`docs/guides/i18n-translation.md` § "New feature → add strings and translate to ALL
  languages"): `sync-locale-keys.ts`, then one translator agent per language mining the reference pile. Budget for it
  when a milestone adds copy. `en-GB` / `en-AU` are overlays and correctly stay empty unless the wording forks
  regionally.
- **A new `$lib/tauri-commands/*.ts` wrapper needs its own test.** `svelte-tests` enforces a 70% per-file coverage
  floor, so the wrapper fails the lane until a `*.test.ts` sits beside it. This is what caught `usage.ts` from piece 1
  at 0%.

---

## A. The native-menu / Objective-C seam

### How native menus are built today

Everything lives in `apps/desktop/src-tauri/src/menu/` (20 files, `CLAUDE.md` + `DETAILS.md` present). Read
`apps/desktop/src-tauri/src/menu/CLAUDE.md` first; it is short and every line is load-bearing.

- `menu/command_map.rs`: the item-ID constants (`SEARCH_FILES_ID = "search_files"`, `GO_TO_PATH_ID = "go_to_path"`, …)
  and the two id↔command maps, `menu_id_to_command(id) -> Option<(&str, CommandScope)>` and its inverse.
- `menu/menu_items.rs` / `menu/menu_structure.rs`: build the pieces and assemble them. `build_context_menu` is at
  `menu/menu_structure.rs:150`.
- `menu/macos.rs` / `menu/linux.rs`: per-platform menu-bar layout.
- `menu/menu_handlers.rs`: `handle_menu_event(app, event)`, wired in `lib.rs` as
  `.on_menu_event(menu::handle_menu_event)` (`apps/desktop/src-tauri/src/lib.rs:753`).
- `menu/macos_appkit.rs`: the objc2 boundary. `cleanup_macos_menus`, `set_macos_menu_icons`, and the reusable helpers
  `menu_item_text` (line 351), `find_ns_submenu`, `find_ns_item`, `set_sf_symbol`, `observe_menu_tracking`,
  `tracking_menu`, `detach_from_supermenu`.
- The recent right-click work: `menu/services_context.rs` (borrows AppKit's one Services menu), `menu/share_submenu.rs`
  (builds `Share` from `file_system::share::services_for`, ids `share-service:<index>`), `menu/context_menu_icons.rs`
  (SF Symbols applied on `NSMenuDidBeginTrackingNotification`), `menu/context_menu_header.rs`, `menu/open_with.rs` (ids
  `open-with:<bundle-id>`), `menu/tag_icons.rs`.

### Crates doing the objc work (all already dependencies — no new crate needed)

From `apps/desktop/src-tauri/Cargo.toml`, `[target.'cfg(target_os = "macos")'.dependencies]`:

- `objc2 = { version = "0.6", features = ["std", "exception"] }` (locked at 0.6.4)
- `objc2-foundation = "0.3"` (feature list at Cargo.toml:341)
- `objc2-app-kit = "0.3"` — already has `NSApplication`, `NSMenu`, `NSMenuItem`, `NSImage`, `NSRunningApplication`,
  `NSWorkspace`, `NSAttributedString`, `NSFont` (Cargo.toml:351)
- `objc2-core-foundation = "0.3.2"` with
  `CFData, CFDictionary, CFString, CFNumber, CFBase, CFCGTypes, CFUserNotification, CFDate, CFURL` (Cargo.toml:402)
- `core-foundation = "0.10.1"`, `plist = "1.8.0"`, `block2 = "0.6"`
- muda is **not** a direct dependency; it arrives through `tauri = "2"` (Tauri 2.11.5 → muda 0.19.3, tao 0.35.3).

### Where the `NSApplication` delegate comes from

tao owns it. `tao-0.35.3/src/platform_impl/macos/app_delegate.rs:47` registers a class literally named
**`TaoAppDelegateParent`** (superclass `NSResponder`) via the deprecated `ClassDecl` API, and
`tao/…/macos/event_loop.rs:178` does `msg_send![app, setDelegate: *delegate]`. The class implements
`applicationDidFinishLaunching:`, `applicationWillTerminate:`, `application:openURLs:`, the two user-activity selectors,
`applicationShouldHandleReopen:hasVisibleWindows:`, and `applicationSupportsSecureRestorableState:`.

**It does NOT implement `applicationDockMenu:`.** That is good news: the selector is free, so no swizzle is needed —
just add the method to the live delegate's class.

- `objc2-app-kit` already declares the protocol method (`objc2-app-kit-0.3.2/src/generated/NSApplication.rs:1213`,
  `fn applicationDockMenu(&self, sender: &NSApplication) -> Option<Retained<NSMenu>>`), so the signature and encoding
  are documented.
- `objc2::ffi::class_addMethod` is public (`objc2-0.6.4/src/ffi/class.rs:78`). Type encoding for
  `- (NSMenu *)applicationDockMenu:(NSApplication *)sender` is `"@@:@"`.
- Recommended install point: `apps/desktop/src-tauri/src/app_lifecycle.rs`'s `on_run_event`, `RunEvent::Ready` arm
  (line 97) — the same place `drag_image_detection::install` runs. The delegate exists by then, and AppKit only asks for
  the Dock menu on a right-click, so the timing is generous.
- Guard the add: check `cls.instance_method(sel!(applicationDockMenu:)).is_none()` first, and log a warning if the
  selector ever appears (a tao upgrade adding it would mean a swizzle instead of an add).

**Existing precedent for reaching into a foreign class:** `apps/desktop/src-tauri/src/drag_image_detection.rs` (swizzles
wry's webview class; `install_swizzles` at line 106 shows the `AnyClass::get`/`instance_method`/`set_implementation`
shape, the `extern "C-unwind"` callback shape, and the `catch_unwind` discipline). Copy its degrade-gracefully-and-log
style. The repo has **no** `objc2::define_class!` / `declare_class!` usage today — introducing one would be new ground.

### Is any NSMenu built dynamically at click time?

Yes, but through Tauri/muda, not raw AppKit. `apps/desktop/src-tauri/src/commands/menu.rs::show_file_context_menu` (line
~197) enumerates share services, builds a fresh `Menu`/`Submenu` per right-click via
`menu_structure::build_context_menu`, then `popup()`s it. Every dynamic list in the app (Open with, Share, tag colors)
is built this way. **No raw `NSMenu` is constructed anywhere in the repo** — `macos_appkit.rs` only _finds_ and
_mutates_ NSMenus that Tauri already made.

### ❗ The load-bearing gap: getting an `NSMenu*` to return from `applicationDockMenu:`

`tauri::menu::Submenu` does **not** expose its underlying `NSMenu`. Its `inner()` is `pub(crate)`
(`tauri-2.11.5/src/menu/submenu.rs:238`) and the public `ContextMenu` trait (`tauri-2.11.5/src/menu/mod.rs:722`) offers
only `popup`, `popup_at`, and `hpopupmenu` (Windows). This is exactly why `macos_appkit.rs` resolves menus by walking
`NSApplication.mainMenu()` and matching on live titles — Tauri gives no ID→NSMenu route. A Dock menu is not in the main
menu bar, so that trick does not transfer.

Three routes, in the order I would try them:

**(a) Build the Dock `NSMenu` by hand with objc2, route clicks ourselves. Recommended.** `NSMenu::new(mtm)`,
`NSMenuItem` per row, `setTarget:` / `setAction:` pointing at one small custom class created with `objc2::define_class!`
(or an `ffi::objc_allocateClassPair` + `class_addMethod` pair, matching the house style in `drag_image_detection.rs`),
plus `setRepresentedObject:` or `setTag:` to carry the row identity. Advantages:

- `macos_appkit.rs::set_sf_symbol` already works on a bare `NSMenuItem`, so icons come free.
- It sidesteps the `CommandScope::FileScoped` focus guard (see § B) entirely — Dock clicks arrive while the app is
  usually **not** focused, and the guard would silently drop them.
- No new crate, no version coupling. Cost: one hand-written ObjC target class, which is new in this repo.

**(b) Add `muda` as a direct dependency and use `muda::Submenu::ns_menu()`.** `muda-0.19.3/src/lib.rs:455` declares
`fn ns_menu(&self) -> *mut std::ffi::c_void` on its `ContextMenu` trait, and Tauri installs a **global** muda event
handler at `tauri-2.11.5/src/app.rs:2350` (`muda::MenuEvent::set_event_handler(...)`), so _any_ muda menu item click
anywhere in the process is forwarded to `.on_menu_event(menu::handle_menu_event)`. That means a muda-built Dock menu
would route through the existing `handle_menu_event` dispatcher with zero new plumbing. ⚠️ Two risks: (i) the pin only
works while cargo unifies our `muda = "0.19"` with Tauri's — a Tauri bump to muda 0.20 would silently produce two copies
of the crate, two sets of statics, and a Dock menu whose clicks go nowhere; (ii) it inherits the FileScoped focus guard
problem below.

**(c) Not viable:** returning a Tauri `Submenu`'s NSMenu. There is no accessor. Do not add one by forking Tauri.

---

## B. The data the Dock menu needs, and whether Rust can reach it

### Favorites — reachable from Rust, synchronously, with no `AppHandle`

`apps/desktop/src-tauri/src/favorites/store.rs::list() -> Vec<Favorite>` (line 430). `Favorite { id, path, name }`. The
store resolves its own data dir without an `AppHandle` (`CMDR_DATA_DIR` else `dirs::data_dir()/BUNDLE_ID`,
store.rs:230), which is exactly what a main-thread menu build needs. Read `favorites/CLAUDE.md` before touching it — the
seed-once-by-file-presence rule is subtle.

A richer view (favorites already mapped to `LocationInfo`, with the FDA-pending skip applied) is
`apps/desktop/src-tauri/src/volumes/mod.rs::list_locations()` (line 219) — also sync and `AppHandle`-free. ❗ It does
**not** include devices or servers; see below.

### Recent locations — exists

`apps/desktop/src-tauri/src/go_to_path/history.rs`: `pub static RECENT_PATHS: RecentsFile<RecentPathEntry>`, capped at
`MAX_RECENTS = 10`, persisted as `go-to-path-history.json`. Entry is `{ id, timestamp, path }`. The generic list
machinery is `apps/desktop/src-tauri/src/recents/` (read its `CLAUDE.md`). Two sibling lists exist for search and
selection; the Dock menu wants the path one.

⚠️ These are recorded only by explicit "Go to path" dialog jumps, not by ordinary pane navigation. If the Dock menu's
"Recent locations" is meant to reflect where the user has actually been, this list will look sparse. Either accept that
(and label it as the Go-to-path recents), or a new list is needed. **Flag this to David before building** — it changes
what the submenu means.

### Mounted volumes and connected devices — reachable, but the full list is `async`

- `apps/desktop/src-tauri/src/volume_listing.rs::complete(local) -> Vec<LocationInfo>` (line 107) is the one assembly
  point: local discovery, then `device_volumes::append_device_volumes` (MTP + ADB), then
  `server_volumes::append_server_volumes` (SFTP/WebDAV/pinned saved servers), then `enrich_from_volume_registry`.
- `volume_listing::list_with_timeout(timeout)` (line 132) is the whole pipeline. It is **`async`**.
- The IPC face is `apps/desktop/src-tauri/src/commands/volumes.rs::list_volumes` (line 47), 2-second timeout.

❗ **A Dock menu must be built synchronously on the main thread** inside `applicationDockMenu:`. Awaiting there is not
an option. `apps/desktop/src-tauri/src/volume_broadcast.rs` keeps `LAST_GOOD_LOCAL` (line 50) but it is private and
holds only the _local_ half. **Plan: keep our own snapshot.** `volume_broadcast::do_emit` publishes the finished list on
every `volumes-changed`; have the Dock module cache a cheap projection (id, name, path, category, connection state)
there and read that cache when the menu is built. This also matches the house "subscribe, don't poll" principle.

`LocationInfo`'s fields are at `apps/desktop/src-tauri/src/volumes/mod.rs:65` — `category: LocationCategory`,
`connection_state`, `device_readiness`, `pinned` are the ones that decide which rows belong under "Connected devices".

### How Rust asks the frontend to act

The unified path, at the bottom of `menu/menu_handlers.rs::handle_menu_event` (line ~530):

```rust
if let Some((command_id, scope)) = menu_id_to_command(id) {
    if scope == CommandScope::FileScoped { /* main window must be focused, else drop */ }
    let _ = crate::window_events::ExecuteCommand { command_id: command_id.to_string() }.emit_to(app, "main");
}
```

Concrete existing examples of a native item driving a frontend dialog:

- `GO_TO_PATH_ID` (`"go_to_path"`) → `("nav.goToPath", CommandScope::FileScoped)` (`command_map.rs:322`)
- `SEARCH_FILES_ID` (`"search_files"`) → `("search.open", CommandScope::FileScoped)` (`command_map.rs:298`)

Items that act on something the frontend can't infer emit their own typed event instead — see `FAVORITES_ADD_CONTEXT_ID`
(spawns a blocking store write, then `volume_broadcast::emit_volumes_changed()`), `MediaIndexFolderExclusion`,
`MediaIndexFolderChoice`, and the `VolumeContextActionKind` table (`menu_handlers.rs::volume_row_action`, line ~560).
Typed-event payloads live in `menu/mod.rs` and ride `tauri_specta::Event`.

### 🚩 The gotcha that will bite: `CommandScope::FileScoped`

`handle_menu_event` **drops** every `FileScoped` command unless `app.get_webview_window("main").is_focused()` is true. A
Dock-tile right-click happens with Cmdr in the background by definition. So:

- "Go to folder…" (`nav.goToPath`) and "Search files…" (`search.open`) reached through `menu_id_to_command` **will do
  nothing** when clicked from the Dock.
- Fix: give the Dock menu its **own** item IDs (`dock:go-to-folder`, `dock:search`, `dock:favorite:<id>`,
  `dock:recent:<n>`, `dock:device:<volume-id>`) handled in their own branch **before** the unified dispatch, and have
  that branch (a) `NSApplication::activate` / `window.set_focus()` the main window, then (b) emit `ExecuteCommand`
  directly. The prefix-routing precedent is `share-service:<index>` and `open-with:<bundle-id>` in the same function;
  `menu/share_submenu.rs`'s `share_service_index` shows the house discipline (answer, never trust, plus a test that
  neighbouring prefixes don't resolve).
- The focus dance also has a precedent: `commands/menu.rs::focus_for_context_menu` (line ~40) and its doc comment
  explain exactly why the focus guard exists and why a "menu is up" flag is the wrong fix. Read it.

### Is the app single-window?

Effectively yes for the main explorer. `apps/desktop/src-tauri/src/app_lifecycle.rs::on_window_event`:

- One window labelled `"main"`. Child windows exist (Settings, Debug, `viewer-*`, queue) but they are secondary.
- **Closing the main window quits the whole app**, gated by `quit::request_quit` (app_lifecycle.rs, `CloseRequested`
  arm, line ~65): if the gate holds, `api.prevent_close()`; otherwise `stop_background_services()` +
  `app_handle().exit(0)`.

❗ Consequence for the Dock menu: "New window" has no existing implementation and, given the above, cannot mean a second
main window today without new lifecycle work. Two honest options — reuse `applicationShouldHandleReopen:` semantics
(bring the existing window forward) and label the item accordingly, or scope a real multi-window feature. **This needs
David's call.** Do not silently ship a "New window" that just focuses the existing one under that label.

---

## C. Startup and the app data dir

### Where startup init runs

`apps/desktop/src-tauri/src/lib.rs`, the Tauri `.setup(move |app| { … })` closure starting at line 207 and ending at
line ~751. It runs on the main thread, inside tao's `did_finish_launching`. The tail of it (lines ~690–751) is the
natural neighbourhood for a ledger append; existing neighbours there:

- `config::resolved_app_data_dir(app.handle())` → `ai::llm_log::init(&data_dir)` +
  `search::start_importance_weight_subscriber(data_dir)`
- `operation_log::start(app.handle())` (line 726)
- `agent::start(app.handle())`
- `window_state::init(app.handle())` / `track` / `restore`

**Recommended hook point:** right beside the `resolved_app_data_dir` block, since the ledger needs the same value. Keep
the write off the main thread if it is anything but trivial (`tauri::async_runtime::spawn_blocking`), matching how the
favorites write is dispatched in `menu_handlers.rs`.

`app_launched` (the existing PostHog event) also fires from `lib.rs` setup — a useful anchor for "once per launch".

### `crate::config::resolved_app_data_dir` semantics

`apps/desktop/src-tauri/src/config.rs:27`:

```rust
pub fn resolved_app_data_dir<R: Runtime>(app: &AppHandle<R>) -> Result<PathBuf, String>
```

1. `CMDR_DATA_DIR` env var if set and non-empty (private `data_dir_from_env` handles the empty case);
2. otherwise Tauri's `app.path().app_data_dir()`. Creates the directory. `log_app_data_dir` (line 52) logs which branch
   won.

**Isolated / dev dirs.** `CMDR_DATA_DIR` is set by `tauri-wrapper.js` for dev, by every per-worktree
`pnpm dev --worktree <slug>` run, and by the E2E harness. Full matrix: `docs/tooling/instance-isolation.md`.

❗ **Worktree dev runs cannot pollute the real ledger** as long as the ledger goes through `resolved_app_data_dir` (or
the `AppHandle`-free twin below) — an isolated instance gets its own dir. If the nudge should additionally be suppressed
in non-production processes, the canonical test is `apps/desktop/src-tauri/src/prod_instance.rs::non_prod_env_var_in`
over `NON_PROD_ENV_VARS` (`CI`, `CMDR_INSTANCE_ID`, `CMDR_DATA_DIR`, `CMDR_E2E_MODE`, `CMDR_MOCK_FDA`). ❌ Never restate
that list.

**`AppHandle`-free variant** (needed if the ledger must be readable from a sync, handle-free context such as a menu
build): copy the shape used by `install_id.rs:59` and `favorites/store.rs:230` — `CMDR_DATA_DIR` else
`dirs::data_dir().map(|base| base.join(BUNDLE_ID))`, with `const BUNDLE_ID: &str = "com.veszelovszki.cmdr"` kept in sync
with `tauri.conf.json`.

### Precedent for a small durable JSON file that is not `settings.json`

Plenty. Existing files in the data dir: `favorites.json`, `go-to-path-history.json`, `search-history.json`,
`selection-history.json`, `install-ids.json`, `license.json`, `known-shares.json`, `manual-servers.json`,
`known-sftp-hosts.json`, `known-sftp-servers.json`, `ai-state.json`, `app-status.json`, `crash-report.json`,
`secrets.json`.

Closest models to copy:

- `apps/desktop/src-tauri/src/favorites/store.rs` — envelope + cache + seed-once + `.broken` quarantine.
- `apps/desktop/src-tauri/src/recents/persistence.rs` — the envelope, the durable temp+rename write, the quarantine.
- `apps/desktop/src-tauri/src/config.rs::durable_write_json(path, tmp, content)` (line 76) — **use this** for the write.
  It fsyncs the temp _and_ the parent directory, which plain `write` + `rename` does not.

🚩 **Naming mismatch to resolve.** The house convention is `_schemaVersion` (leading underscore), enforced by nothing
but used by both `favorites/store.rs:64` and `recents/persistence.rs:16`, and asserted in `recents/persistence.rs:143`.
The brief specifies `"schemaVersion"` without the underscore. Pick one and say why in the module doc; matching the house
convention is the low-friction choice, and the file has no readers outside Cmdr.

---

## D. Toast, settings, i18n

### The seam to raise a toast at startup, after onboarding

`apps/desktop/src/routes/(main)/startup-gates.ts` is the file. It has a `CLAUDE.md` + `DETAILS.md` pair in the same
directory.

- `resolveOnboardingMount(ctx)` decides wizard-vs-explorer. Its two `ctx.showApp()` branches (FDA already granted, and
  denied-but-onboarded) each call `maybeFireUpgradeNudge()`. That function **is the template**:

```ts
export function maybeFireUpgradeNudge(): void {
  if (isE2eRun()) return
  if (getSetting('onboarding.upgradeNudgeShown')) return
  const message = isMacOS() ? tString('main.upgradeNudge.mac') : tString('main.upgradeNudge.other')
  addToast(message, { level: 'info' })
  setSetting('onboarding.upgradeNudgeShown', true)
}
```

Note the three guards it carries and why: `isE2eRun()` (a fresh data dir per Playwright shard would leak the toast into
whichever spec runs first and trip the fixture safety net), the seen-flag, and the fact that both call sites are already
past the wizard so no visibility check is needed. **Copy all three.**

- `maybeShowOldMacosNotice(ctx)` in the same file is the async variant (it awaits an IPC probe before deciding) — the
  closer shape for the Dock nudge, which needs two backend answers (is Cmdr in the Dock, is it in `/Applications`).

### How `updater.svelte.ts` gates its toast

`apps/desktop/src/lib/updates/updater.svelte.ts`:

- Module-level `let onboarded = $state(false)` and `let onboardingShowing = $state(false)` (lines 31–32).
- A pure decision function (line ~43):
  `shouldShowUpdateToast({ onboarded, onboardingShowing, status }) => args.onboarded && !args.onboardingShowing && args.status === 'ready'`.
- The raise (line ~70): `addToast(UpdateToastContent, { id: 'update', dismissal: 'persistent' })`.
- `onboarded` is seeded from `getSetting('onboarding.completed')` at line ~510; `notifyOnboardingComplete()` flips it;
  `setOnboardingShowing(value)` (line ~142) records the wizard's visibility and **re-attempts** the toast when the
  wizard closes.

For the Dock nudge, the equivalent of `status === 'ready'` is the two async probes; the wizard-closed re-attempt is
worth copying so the nudge is not simply lost when the user finishes onboarding on the third launch.

### The toast API

`apps/desktop/src/lib/ui/toast/` — `index.ts` re-exports from `toast-store.svelte.ts`.

```ts
addToast(content: ToastContent, options?: ToastOptions): string
// ToastLevel = 'default' | 'info' | 'success' | 'warn' | 'error'
// ToastOptions = { level?, dismissal?: 'transient' | 'persistent', id?, timeoutMs?, toastGroup?, maxInGroup?, props?,
//                  onDismiss?, closeTooltip?, widthPx?, originPane?, suppressErrorReportAction? }
```

`dismissal: 'persistent'` forces `timeoutMs: 0`. `props` is forwarded into the component, with the toast's own id
appended under the `toastId` key so the component can call `dismissToast(toastId)`.

❗ **`onDismiss` fires only when the user closes the toast with the frame's ×** — never on a timeout, never on a
programmatic `dismissToast`. That makes it the one honest way to tell "swept it away" from an active refusal by a
button, and the pin nudge's `dismissed` answer rides it. Inferring a dismissal from unmount instead would also count a
quit with the toast still up.

### The house pattern David wants copied

A pure `should-show-*.ts` decision function with unit tests, plus a `*ToastContent.svelte`:

- `apps/desktop/src/lib/file-explorer/navigation/should-show-pin-hint.ts` + `.test.ts` — exports the thresholds as named
  consts, an inputs interface, an output interface, and one total function returning `null` for silence.
- `apps/desktop/src/lib/file-explorer/navigation/ServersPinHintToastContent.svelte` — title / body / optional extra line
  / a "Got it" `Button size="mini" variant="primary"` that calls `dismissToast(toastId)`.
- `apps/desktop/src/lib/open-terminal/OpenTerminalHintToastContent.svelte` — the sibling in a module that also owns a
  `CLAUDE.md` + `DETAILS.md` pair, plus an `open-terminal-toasts.a11y.test.ts`. **Every toast-content component in this
  repo has an `.a11y.test.ts`** — write one (`desktop-svelte-a11y-coverage` has an allowlist and you should not touch
  it).
- `apps/desktop/src/lib/adb/should-show-adb-hint.ts` + `.test.ts` — a third instance of the same shape.

The raise site for the pin hint is `apps/desktop/src/lib/stores/volume-store.svelte.ts::notePinnedCount` (line ~108):
decide → set the seen flag → `addToast(Component, { level: 'info', dismissal: 'persistent', id: '…', props: {…} })`.
Note the flag is set **when the toast is raised**, not when it is dismissed — matching `maybeFireUpgradeNudge`. Keep
that; the rationale is spelled out in `maybeShowOldMacosNotice`'s doc comment.

Home for this code (as shipped): `apps/desktop/src/lib/dock/`, holding `should-show-dock-nudge.ts`, `dock-nudge.ts` (the
raise), `dock-pin-answer.ts` (the answers), `DockPinNudgeToastContent.svelte`, their tests, and a `CLAUDE.md` +
`DETAILS.md` pair. ❗ The raise and the answers are two modules on purpose: the raise imports the toast component and
the component imports the answers, so one module holding both is a cycle `import-cycles` fails.

### Adding a setting — full procedure

Canonical checklist: `docs/guides/adding-a-new-setting.md`. Read `apps/desktop/src/lib/settings/CLAUDE.md` too. For a
hidden seen-flag the path is short:

1. **`apps/desktop/src/lib/settings/definitions/behavior.ts`** — add the entry. Copy `behavior.serversPinHintSeen` (line
   ~54) verbatim in shape:
   ```ts
   {
     id: 'behavior.dockPinNudgeSeen',
     section: ['Behavior', 'Navigation & file ops'],
     labelKey: 'settings.behavior.dockPinNudgeSeen.label',
     descriptionKey: 'settings.behavior.dockPinNudgeSeen.description',
     keywords: [],
     type: 'boolean',
     default: false,
     component: 'switch',
     hidden: true,
   }
   ```
2. **`apps/desktop/src/lib/settings/types.ts`** — add `'behavior.dockPinNudgeSeen': boolean` to `SettingsValues`.
   Skipping this is a `svelte-check` error at the registry entry (`SettingDefinition.id` is `keyof SettingsValues`).
3. **`apps/desktop/src/lib/intl/messages/en/settings.json`** — add the `…label` / `…description` values **and** their
   ARB `@`-sibling `description` entries. See the `serversPinHintSeen` block at settings.json:1886 for the exact shape
   and the "Internal, never shown in the UI" wording.
4. `hidden: true` ⇒ **skip step 2 of the guide** (no section rendering).
5. Frontend-only ⇒ **skip step 3** (no Tauri command, no `settings-applier.ts` case).
6. A new key is additive ⇒ **no `SCHEMA_VERSION` bump**.

**`node scripts/gen-analytics-defaults.ts` — yes, it must be re-run and committed.** Adding a registry entry changes the
defaults manifest. Run `pnpm --filter @cmdr/desktop analytics:defaults` (or just
`pnpm check analytics-settings-defaults`, which regenerates in place outside `--ci` so the fix is already staged) and
commit `apps/analytics-dashboard/src/lib/server/settings-defaults.gen.json`. The check
(`scripts/check/checks/analytics-settings-defaults.go`) asserts the `next` block matches the working tree; in CI it
restores the original and fails on drift.

### Adding i18n message keys — full procedure

Read `apps/desktop/src/lib/intl/messages/CLAUDE.md` and `apps/desktop/src/lib/intl/CLAUDE.md`.

**Two families, and they behave differently.**

_Frontend (toast) copy_ — a normal ICU key:

1. Add to `apps/desktop/src/lib/intl/messages/en/<area>.json`. Key shape `area.feature.leaf`, lowerCamel segments, first
   segment a known area matching the filename (`desktop-message-key-naming`). A new area needs both a new catalog file
   and the area registered.
2. **Double every apostrophe (`''`)** — ICU escaping.
3. Add the ARB `@key` sibling with a `description` (surface + trigger + constraints + placeholder meanings). Every key
   should carry one.
4. `pnpm --filter @cmdr/desktop intl:keys` regenerates `apps/desktop/src/lib/intl/keys.gen.ts`. ❌ Never hand-edit it.
   `desktop-message-keys-fresh` fails if stale.
5. The key needs a real call site in `apps/desktop/src/` or `src-tauri/src/` or `desktop-message-keys-unused` (ERROR)
   fails.
6. Translations: only `en/` is authored by us. `desktop-i18n-coverage` / `-parity` / `-stale` govern the rest; check
   `docs/guides/i18n-translation.md` for whether a new key blocks or just gets flagged.

_Native Dock-menu labels_ — the `menu.*` family, drawn by Rust:

1. Add to `apps/desktop/src/lib/intl/messages/en/menu.json`.
2. ❗ `menu.*` is a **RAW** family (`isRawKey`): it never meets ICU, so apostrophes stay **single** and `{token}`s are
   literal. `i18n-icu` (ERROR) fails a raw value containing `''`.
3. `pnpm --filter @cmdr/desktop intl:native-strings` regenerates
   `apps/desktop/src-tauri/src/intl/native_strings.gen.rs`. ❌ Never hand-edit. Guarded by `native-strings-fresh`.
4. Read it in Rust with `crate::intl::menu_t("menu.…")` / `menu_t_with`. ❌ Never `t()`, ❌ never a literal
   (`menu/CLAUDE.md`: "Every label comes from `menu_t`").
5. No screenshot can photograph a native surface, so the `@key` `description` is the translator's whole aid: say which
   menu, what the item does, and whether it is a VERB or a NOUN.
6. House rule from `menu/CLAUDE.md`: a trailing `…` (always U+2026) means the dialog changes _what_ the command acts on.
   `Go to folder…` and `Connect to server…` earn it; `New window` and the submenu titles do not.

**Locale-completeness checks** (`scripts/check/checks/desktop-i18n-*.go`): `i18n-coverage`, `i18n-parity`, `i18n-stale`,
`i18n-icu`, `i18n-plural`, `i18n-terms`, `i18n-aria`, `i18n-dont-translate`, `i18n-tag-param-collision`,
`i18n-trans-snippets`, plus `message-key-naming`, `message-keys-fresh`, `message-keys-unused`,
`message-screenshots-fresh`, `shipped-locales-fresh`, `native-strings-fresh`.

---

## E. Analytics

### The exact procedure to add a PostHog event

Read `apps/desktop/src-tauri/src/analytics/CLAUDE.md`, then `DETAILS.md` § "How to add an event" (line 123).

**Backend:** at the success chokepoint,

```rust
crate::analytics::posthog::capture("dock_pin_offered", serde_json::json!({ "kind": some_enum }));
```

**Frontend:** `import { trackEvent } from '$lib/tauri-commands'` then
`void trackEvent('dock_pin_answered', { answer })`. The wrapper is `apps/desktop/src/lib/tauri-commands/analytics.ts:14`
(`trackEvent(name, props: Record<string, string | number | boolean>)`), re-exported from
`$lib/tauri-commands/index.ts:330`. It `JSON.stringify`s the props and calls the `track_event` IPC
(`apps/desktop/src-tauri/src/commands/analytics.rs`), a thin pass-through to `capture`.

**So: yes, events can be emitted from the frontend.** Given the nudge decision lives in the frontend, all three events
are naturally frontend events. `dock_pin_failed`'s typed reason must arrive from Rust as a typed enum across IPC and be
stringified for the prop — ❌ never string-match a message (`AGENTS.md` hard rule, enforced by `error-string-match` /
`cmdr/no-error-string-match`).

### The emitter shapes the check recognizes

`scripts/check/checks/analytics-event-catalog.go` scans `apps/desktop/src`, `apps/desktop/src-tauri/src`, and `crates`
(skipping `tests/`, `node_modules`, `target`) for exactly four regexes:

- `posthog::capture\(\s*"([a-z0-9_]+)"`
- `\bcapture\(\s*"([a-z0-9_]+)"` — **only inside a file named `analytics.rs`**
- `analytics\(\)\.record\(\s*"([a-z0-9_]+)"` — the `AnalyticsSink` seam for tauri-free crates
- `trackEvent\(\s*['"]([a-z0-9_]+)['"]`

Event names must be `[a-z0-9_]+` **literals at the call site**. An event smuggled through a helper taking a runtime
`&str` is invisible to the check — and, per the check's own doc comment, the answer is not to write one.

### The section that must be updated

`apps/desktop/src-tauri/src/analytics/DETAILS.md`, heading **`## Starter event set`** (line 157). The check reads
bullets under that heading; a bullet opens with one or more backticked names joined by `/`:

```
- `dock_pin_offered` (frontend, `$lib/dock/dock-nudge.ts` `offerDockPin`): no props.
- `dock_pin_answered` (frontend, `$lib/dock/dock-pin-answer.ts`): `answer` (`yes` / `no` / `dismissed`).
- `dock_pin_failed` (frontend, same file as `dock_pin_answered`): `reason`, the typed refusal from Rust.
```

(The three don't share one file: see the import-cycle correction at the top.)

**The check is an ERROR, not a warning, and it fails in both directions**: an emitted-but-undocumented event fails, and
a documented-but-unemitted one fails too. So do not add the catalog bullets before the emitters exist (or land both in
one commit).

Prop rules from `analytics/CLAUDE.md`: every prop value must be categorical, a count, or a bool — ❌ never a path, name,
query, prompt, or hostname. `posthog::sanitize_props` only `warn!`s in debug builds; it is a smoke alarm, not a filter.
`source`, `app_version`, `os_version`, `arch` ride every event automatically.

⚠️ **`capture` is a no-op in dev.** `suppression_reason()` suppresses debug builds and every isolated data dir; set
`CMDR_ANALYTICS_FORCE=1` to test against a local Worker.

---

## F. Dock manipulation prerequisites

### Nothing Dock-related exists today

Confirmed by a full-tree grep. Every "Dock" hit is either prose (`quit/CLAUDE.md`, `analytics/DETAILS.md`,
`app_lifecycle.rs:103`, `updater/installer.rs:401`), the per-instance **Dock label** (`productName`, see
`docs/tooling/instance-isolation.md`), a translation-glossary entry, or DevTools _docking_ (`drag-position.ts`). There
is no `com.apple.dock` read, no `persistent-apps`, no `NSDockTile`, no `applicationDockMenu:`.

### Which already-present crate reads and writes another app's preferences domain

**`objc2-core-foundation` 0.3.2, already a dependency** — add the `CFPreferences` feature to its feature list in
`apps/desktop/src-tauri/Cargo.toml:402`. No new crate, no version check, no 3-day-old-release gate.

`objc2-core-foundation-0.3.2/src/generated/CFPreferences.rs` exposes everything needed:

- Domain constants: `kCFPreferencesCurrentUser`, `kCFPreferencesAnyHost`, `kCFPreferencesCurrentHost`,
  `kCFPreferencesAnyUser`, `kCFPreferencesAnyApplication`, `kCFPreferencesCurrentApplication`.
- Read: `CFPreferencesCopyAppValue(key, application_id) -> Option<CFRetained<CFPropertyList>>` (line 39), and the
  fully-qualified `CFPreferencesCopyValue(key, application_id, user, host)` (line 132).
- Write: `CFPreferencesSetAppValue` (line 90) and `CFPreferencesSetValue(key, value, application_id, user, host)` (line
  176).
- Flush: `CFPreferencesAppSynchronize(application_id) -> bool` (line 123) and
  `CFPreferencesSynchronize(application_id, user, host)` (line 201).
- `CFPreferencesAppValueIsForced(key, application_id)` (line 253) — worth checking before writing, in case an MDM
  profile manages the Dock.

The `CFArray` / `CFDictionary` features are already enabled, which `persistent-apps` needs (it is an array of
dictionaries; each entry is
`{ "GUID": …, "tile-data": { "file-data": { "_CFURLString": "file:///Applications/Cmdr.app/", "_CFURLStringType": 15 }, … }, "tile-type": "file-tile" }`).

Alternatives considered: `core-foundation = "0.10.1"` (also present, and used by `updater/bundle_location.rs`) has **no
`preferences` module** — its `src/` is array/bundle/data/dictionary/number/ propertylist/string/url/etc.
`plist = "1.8.0"` is present and could parse `~/Library/Preferences/com.apple.dock.plist` directly, but ❌ do not:
`cfprefsd` owns that file, an in-memory cache sits in front of it, and a direct write will be overwritten.

🚩 **Gotchas to design for** (none verified on a live system by this survey — verify at implementation time and stamp
the finding per the house evidence-anchor rule):

- The Dock caches its preferences in memory and rewrites them on quit, so the sequence must be set →
  `CFPreferencesAppSynchronize("com.apple.dock")` → restart the Dock. Restarting: `NSRunningApplication` (already
  enabled in `objc2-app-kit`) filtered on bundle id `com.apple.dock`, then `terminate` — `launchd` respawns it. A
  `killall Dock` subprocess would also work (`crate::subprocess` exists) but the AppKit route is cleaner and matches the
  house preference for OS-native APIs.
- Reading `persistent-apps` to answer "is Cmdr already in the Dock" must compare **resolved bundle paths**, not the raw
  `_CFURLString` (which may be a file-reference URL, may or may not carry the trailing slash, and may point at a
  translocated or old copy).
- Sandboxing: Cmdr is not sandboxed (it does FDA, SMB, MTP), so `com.apple.dock` is readable and writable for the
  current user. Confirm on a signed, notarized build before shipping.
- A Dock that is managed by a configuration profile (`CFPreferencesAppValueIsForced`) must produce a typed
  `dock_pin_failed` reason rather than a silent no-op.

### How the app learns its own bundle path

`apps/desktop/src-tauri/src/updater/installer.rs`:

- `pub(super) fn running_bundle() -> Result<PathBuf, String>` (line 170): `std::env::current_exe()` then
  `find_app_bundle_above` — the closest ancestor whose segment ends in `.app`.
- `find_app_bundle_above(start)` (line 178) is the pure, unit-tested walk.
- `pub fn is_running_from_app_bundle() -> bool` (line 84) — `running_bundle().is_ok()`. In a dev build `current_exe()`
  is `target/<triple>/release/Cmdr` with no `.app` ancestor, so this is `false`, which conveniently suppresses the whole
  feature in dev.

❗ `running_bundle` is `pub(super)` (visible only inside `updater`). Either widen it to `pub` or lift the pair into a
neutral home. Widening is the smaller change; a `crate::platform` or a new `crate::dock` helper that re-exports it is
tidier. **There is no existing "is Cmdr in `/Applications`" test** — `updater/bundle_location.rs::classify` answers a
different question (translocated / read-only volume, for whether an update can be _written_). Add a small pure predicate
beside `find_app_bundle_above` and unit-test it: `/Applications/Cmdr.app` yes, `~/Applications/Cmdr.app` — decide and
document (the brief says `/Applications`, and `updater/installer.rs:455` shows the user-local path is a real case worth
naming).

`objc2-foundation`'s `NSBundle` is also enabled and used at `file_system/open_with.rs:231`, so
`NSBundle::mainBundle().bundleURL()` is available as a cross-check. `current_exe()` is what the repo already trusts.

---

## G. Docs and checks

### `CLAUDE.md` / `DETAILS.md` pairs to update

`claude-md-details-sibling` (`scripts/check/checks/registry.go:1557`, DisplayName "CLAUDE.md has a sibling DETAILS.md")
enforces that **every `CLAUDE.md` has a `DETAILS.md` next to it**. ❌ Never `@`-import a `DETAILS.md` from a
`CLAUDE.md`. Budget: `CLAUDE.md` aims at 300–400 words; `claude-md-length` warns past 600. `claude-md-reminder` warns
when source files change under a directory with a colocated `CLAUDE.md` and no doc on the ancestor chain was touched —
it counts `.rs`, `.ts`, `.svelte`, `.css`, `.go`, `.js`.

Existing pairs the work touches:

- `apps/desktop/src-tauri/src/menu/` — the Dock menu, its item IDs, and the FileScoped bypass.
- `apps/desktop/src-tauri/src/analytics/` — `DETAILS.md` § "Starter event set" (**required**, the check is an error).
- `apps/desktop/src-tauri/src/favorites/` — only if the Dock menu changes how favorites are read.
- `apps/desktop/src-tauri/src/updater/` — if `running_bundle` is widened or an `/Applications` predicate lands there.
- `apps/desktop/src-tauri/src/recents/` and `apps/desktop/src-tauri/src/go_to_path/` — if recents gain a second reader.
- `apps/desktop/src/routes/(main)/` — the new startup gate.
- `apps/desktop/src/lib/settings/` — probably not; a hidden flag is unremarkable.
- `apps/desktop/src/lib/intl/messages/` — probably not.
- `apps/desktop/src/lib/updates/` — only if the nudge borrows the updater's re-attempt plumbing.

New pairs to create (both files, always together):

- `apps/desktop/src-tauri/src/dock/CLAUDE.md` + `DETAILS.md` — the ledger, the CFPreferences read/write, the Dock
  restart, the `/Applications` predicate.
- `apps/desktop/src/lib/dock/CLAUDE.md` + `DETAILS.md` — the decision function, the toast, the events.

Also update `docs/architecture.md` (the subsystem map: what + where + a pointer, never how) so `docs-reachable` keeps
the new docs connected.

### Relevant `pnpm check` lanes

Always `pnpm check` from the repo root; ❌ never raw `cargo` / `vitest`. ❌ Never tail or truncate its output.

Rust:

- `pnpm check clippy` — ❗ not in `--fast`; a lint error here blocks every other session's `rust-tests`.
- `pnpm check rustfmt`, `pnpm check rust-tests`
- `pnpm check cargo-deny`, `cargo-machete`, `cargo-udeps` — a new Cargo _feature_ on an existing crate should be
  invisible to these, but run them once.
- `pnpm check macos-framework-floor`, `macos-availability` — CFPreferences is ancient, but the checks are cheap.
- `pnpm check error-string-match` — the typed-reason rule.
- `pnpm check cfg-gate`, `lock-poison`, `discarded-outcome`, `log-error-macro`.

Frontend:

- `pnpm check svelte` (the tech group), or individually `desktop-svelte-eslint`, `eslint-typecheck-ts`,
  `eslint-typecheck-svelte`, `svelte-check`, `stylelint`, `css-unused`, `a11y-contrast`, `a11y-coverage`,
  `ui-primitive-coverage`, `import-cycles`, `bare-poll`, `knip`. (There is no bare `eslint` or `desktop-tests` lane;
  `pnpm check --help` lists the real names.)
- `pnpm check svelte-tests` (Vitest) — the `should-show-*` unit tests and the `.a11y.test.ts`. ❗ It also enforces a
  **70% per-file coverage floor**, so a new `$lib/tauri-commands/*.ts` wrapper fails the lane until it has its own
  `*.test.ts`.
- ❗ `pnpm check i18n-coverage` — ERROR-level, and it fails on every new English key until all ten full-translation
  locales carry it. See the correction at the top: adding user-facing copy pulls in the whole translator process, and
  that's a milestone-sized cost, not a lane you fix in a minute.

Generated-artifact freshness (each has a matching `pnpm` script; the checks regenerate in place outside `--ci`, so
**commit the rewrite**):

- `message-keys-fresh` ← `pnpm --filter @cmdr/desktop intl:keys`
- `native-strings-fresh` ← `pnpm --filter @cmdr/desktop intl:native-strings`
- `analytics-settings-defaults` ← `pnpm --filter @cmdr/desktop analytics:defaults`
- `bindings-fresh` ← `pnpm --filter @cmdr/desktop bindings:regen` (only if a new IPC command lands)

Contracts:

- `analytics-event-catalog` (**ERROR**), `message-key-naming`, `message-keys-unused` (**ERROR**), the ten `i18n-*`
  lanes.
- `docs-reachable` (**ERROR**), `docs-dead-links`, `docs-section-refs`, `docs-link-text`, `docs-table-hygiene`,
  `claude-md-details-sibling`, `claude-md-length`, `file-length`, `resident-doc-budget`.

Cadence per `AGENTS.md`: `--fast` while iterating, plain `pnpm check` per milestone, `--include-slow` after roughly
every second milestone.

### Adding an IPC command (if the nudge needs backend probes)

Two lists in `apps/desktop/src-tauri/src/ipc.rs`: the specta-collected list (line ~215 onwards) and a
`dispatch_only: [ … ]` block (line ~459) for commands generic over `R: tauri::Runtime`, which `collect_functions!`
cannot take. A non-generic command goes in the first list only. Then run `pnpm --filter @cmdr/desktop bindings:regen`
and commit `apps/desktop/src/lib/ipc/bindings.ts` (`bindings-fresh` guards it). The typed frontend wrapper goes in
`apps/desktop/src/lib/tauri-commands/` and is re-exported from its `index.ts`.

---

## Open questions for David

Both remaining ones belong to piece 3, the Dock tile menu.

1. **"New window"** has no meaning today: closing the main window quits the app, and there is only ever one `"main"`
   window. Should the item bring the existing window forward (and be renamed), or is a real second-window feature in
   scope?
2. **"Recent locations"** would come from the Go-to-path dialog's history (`go_to_path/history.rs`, cap 10), which
   records only explicit dialog jumps — not ordinary pane navigation. Accept that, or add a new list?

Two more are settled, and the answers live in the code: `usage.json` uses `_schemaVersion` (the house convention), and
both `/Applications` and `~/Applications` count as an Applications folder (`dock/location.rs`).

## Still unverified

- Whether a muda-built menu returned from `applicationDockMenu:` actually posts its `MenuEvent` (the global handler is
  installed process-wide, so it should, but AppKit tracks a Dock menu in its own context and this was not tested). The
  one open item for piece 3.

The other two are answered: the real `persistent-apps` entry shape and the TCC findings for writing `com.apple.dock`
from a signed build are recorded, with their evidence, in `apps/desktop/src-tauri/src/dock/DETAILS.md`.
