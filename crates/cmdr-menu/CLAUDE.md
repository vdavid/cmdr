# cmdr-menu

Cmdr's menu bar and every right-click menu as data, with no UI toolkit. Two hosts: the desktop builds native muda menus
from it (`apps/desktop/src-tauri/src/menu/CLAUDE.md`), and the browser demo draws the macOS bar and its right-click
menus through `cmdr-listing-wasm` (`crates/cmdr-listing-wasm/CLAUDE.md`).

## Module map

- `bar.rs`: `MENU_BAR`, the macOS, Windows, and Linux bars, one row per item, in the words `spec.rs` defines. `ids.rs`:
  every menu id and the id ↔ command map. `enabled.rs`: the enabled-state fold. `click.rs`: what a row's click asks for.
- `sort.rs` (Sort by checks), `labels.rs` (the live labels as key + tokens), `accelerators.rs` (frontend shortcut ↔
  Tauri accelerator ↔ macOS glyphs).
- `context/`: the right-click menus. `file.rs` (the file menu), `popups.rs` (the small ones), `click.rs` (what a pick
  asks for), `image_index.rs` (the image-search group). Pinned by `context_test.rs`.

## Must-knows

- **Two snapshots pin the bar**: `bar_test.rs` (rows, ids, accelerators, label keys) and `bar_states_test.rs` (every
  row's command and enabled state under four input sets), one block per platform. A bar change is a diff of both; review
  it, then update.
- **The bar's keys follow the registry**: `bar_parity_test.rs` requires each row's accelerator to be what its command's
  first default key registers, per platform, read from `registry_default_keys.tsv` (written by the frontend's
  `default-keymap.test.ts`, ❌ never by hand). A row that differs on purpose goes in its `EXCEPTIONS` with a reason; a
  stale one fails. It also compares what a row SHOWS, so a key the bar leaves to keydown dispatch is shown.
- **The bar never registers a key the page owns** (native text keys; on Windows and Linux the keys of the windows that
  wear the bar), past a row's own built key: `page_owned_keys.tsv`, generated like the registry file, ❌ never by hand
  (`DETAILS.md` § "Keys the page owns").
- **Three platforms, no leftover**: a row is on every platform, `macos_only`, or `windows_and_linux`; an accelerator is
  `same_on_all`, `cmd_or_ctrl!`, `macos`, or `split(macos, windows, linux)`, and a shown key is `shown_on_all`,
  `shown_split`, or `shown_off_macos` (`displayed_item`, or `.shown(…)` where the row registers nothing), each in that
  platform's spelling. Windows has the Linux bar until its menu model is decided (`DETAILS.md` § "Three platforms").
- **A context menu is rows for a set of facts** (`file_menu`, `tab_menu`, …): a new item is a row there plus its lane in
  `context_click`; `every_row_the_crate_builds_has_a_click` fails a row whose pick does nothing. A `Slot` is a group
  only the OS fills (Open with, Share, tags, the provider, Services): the desktop builds it, the demo leaves it out.
- **No `cfg` in the data**: every bar exists on every host, so a Mac test pins the Windows and Linux bars.
  `Platform::current()` is the desktop's call; a wasm build answers Linux, so the demo passes `Platform::MacOs`.
- **Every accelerator here is a string muda must parse**, and Tauri drops one it can't, silently. This crate has no
  muda; the desktop's `menu/accelerators.rs` tests parse every one. ❌ Never `Opt`, ❌ never `Cmd+…` on Windows or Linux
  (`same_on_all` / `split` refuse it at compile time; use `cmd_or_ctrl!`).
- **The enabled fold is the ONE rule** both hosts apply: a new input goes into `MenuItemInputs`, ❌ never a host-side
  special case.
- **Wire words are the frontend's** (`ViewMode::wire`, `Pane::wire`, `SortCheck::wire`, `VolumeAction::wire`, the label
  targets' serde shapes): `bindings.ts` and `menu-sort-command.ts` read them. Change both sides or neither.
- **`ids.rs` is parsed by `rust-command-id-drift.test.ts`** by path and by the `pub const NAME: &str = "…";` shape. Keep
  that shape; move the file and the test moves with it.
- No `tauri` here, enforced by `index-crate-isolation`.

Decisions, the hosts' split, and what each host still owns: `DETAILS.md`.
