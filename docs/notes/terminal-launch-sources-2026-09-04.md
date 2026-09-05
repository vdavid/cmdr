# Terminal launch sources (2026-09-04)

The external evidence behind `apps/desktop/src-tauri/src/file_system/terminal.rs`: where each bundle id came from, why
each app gets the launch recipe it gets, and what each one does with the window-vs-tab question. The decisions
themselves live in `apps/desktop/src-tauri/src/file_system/DETAILS.md` § "Open terminal here"; this note is the paper
trail, so a future entry can be added to `KNOWN_TERMINALS` to the same standard.

## Why a table at all

macOS has no system-wide "default terminal" setting. Finder's own "New Terminal at Folder" service is hardcoded to
Terminal.app, and iTerm2 works around that by installing a competing service. Every app that offers this action (VS
Code, Marta, Raycast, the "New Terminal Here" App Store app) ships its own list of known terminals plus a setting, so
Cmdr does the same rather than inventing a mechanism macOS doesn't have.

## Bundle ids

Each id's source and date live in the doc comment above `KNOWN_TERMINALS`, which is where somebody adding an app is
already standing, so they are not repeated here. All eight were verified 2026-09-04, three by `mdls` against the copy
installed on the machine and five by reading the id out of the vendor's own build config. **A new entry owes the same: a
source and a date, not a recollection.**

Two things worth knowing before you go looking for one:

- ⚠️ **iTerm2's repository carries several ids.** The app is `com.googlecode.iterm2`; the `com.iterm2.*` ids sitting
  beside it in the same project file belong to helper targets (pidinfo, the proxy, the sandboxed worker). Picking one of
  those gives a table entry that never matches an installed app, and nothing fails loudly.
- **Warp's id names a release channel.** `dev.warp.Warp-Stable` is the stable build, which is the one that was measured.
  Whether Warp's preview channel ships under a different id was not checked; if it does, it needs its own entry, and
  until then a user on that channel reaches Warp through "Choose an app…", like any app the table doesn't carry.

## Launch recipes

Three shapes cover the eight apps, plus one for an app the user points at by hand.

- **Folder as a document** (`open -b <id> <dir>`) covers Terminal, Ghostty, Hyper, iTerm2, kitty, and WezTerm: each
  registers as a folder handler and starts there.
- **A working-directory flag** (`open -n -b org.alacritty --args --working-directory <dir>`) is Alacritty, which does
  not accept a folder as a document. The `-n` is load-bearing: without it `open` merely activates an already-running
  instance and drops the args, so the second invocation of the day would silently open at the wrong place.
- **Warp's URI scheme** (`open warp://action/new_window?path=<percent-encoded dir>`), documented at
  <https://docs.warp.dev/terminal/more-features/uri-scheme/>. Warp is the one app whose recipe ends in a URI rather than
  a path, which is why the E2E mock records the FOLDER rather than the argv.
- **A hand-picked app** ("Choose an app…") gets `open -a <app path> <dir>`, which works for any terminal that registers
  as a folder handler, and nearly all of them do.

Verified 2026-09-04 by opening a folder named `Ünnepi "terv" & co` in the installed Terminal, Ghostty, and Warp and
reading back the spawned shell's working directory. That is what pins the percent-encoding: Warp's `path=` value keeps
`/` literal (legal in a query, and the shape Warp's docs show) and encodes `&`, `?`, `#`, `%`, `+`, space, quotes, and
every non-ASCII byte, so a folder name cannot rewrite the URI.

## Window against tab: why there's no control for it

Surveyed 2026-09-03 against each vendor's documentation and discussions. There is no portable mechanism, and the per-app
answers do not rhyme:

- **Terminal** follows the system-wide "Prefer tabs when opening documents" setting, so the choice is already the user's
  and macOS already owns it.
- **Ghostty** opens a tab in the running instance when handed a folder, and a window only via `--args`:
  <https://github.com/ghostty-org/ghostty/discussions/5910>.
- **Warp** has both actions in its URI scheme (`new_window` and `new_tab`), so it is the one app that could honor a
  toggle cleanly.
- **iTerm2** would need AppleScript for a tab, which raises an Automation permission prompt the action does not
  otherwise need.
- **kitty** and **WezTerm** each need their own CLI or remote-control channel for it.
- **Alacritty** has no tabs at all, so the question does not apply.

So each app is launched the way it natively takes a folder, and the app's own preferences decide. A toggle shown only
for the apps that honor it (Warp, Ghostty, WezTerm) is the shape to revisit if anyone asks; nobody has.

## What would change these answers

Any of these is a reason to re-verify rather than trust the lines above: an app changing its bundle id (a rename or a
new vendor), Warp versioning its URI scheme, Ghostty gaining a documented folder-to-window flag, or macOS retiring the
"Prefer tabs" setting. The recipes are pure (`launch_argv`), so each one is unit-tested without launching anything, and
a wrong recipe shows up as a failing argv test rather than as a mystery on someone's Mac.
