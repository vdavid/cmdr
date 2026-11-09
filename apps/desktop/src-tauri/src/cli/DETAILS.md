# The `cmdr` terminal command: architecture and decisions

Up: `CLAUDE.md`. Issue: [#95](https://github.com/getcmdr/cmdr/issues/95).

## What it does

- `cmdr <folder>` opens the folder in the focused pane; `cmdr <file>` opens its folder with the cursor on it. Several
  files from one folder all get selected (cursor on the first). `cmdr` alone means `cmdr .`.
- `--left` / `--right` pick the pane (and focus it); `--new-tab` opens a tab in that pane first. A pane at its tab limit
  refuses the tab, and the move then lands in the current tab (logged): landing where the user asked beats the tab.
- Starts Cmdr when it isn't running and raises the main window.
- When paths span folders, the script warns on stderr which ones Cmdr leaves out, mirroring `plan_reveal`'s rule (a
  folder first opens only itself; a file first opens its folder and selects the siblings there). It still sends them
  all: the app makes the final call.
- `--print-url` prints the URL instead of opening Cmdr (the tests drive the script through it); `--version` reads
  `CFBundleShortVersionString` from the bundle the script sits in.

## The transport

`script → open -a <bundle> 'cmdr-cli://open?path=%2F…&pane=left&tab=new' → application:openURLs: → RunEvent::Opened →
reveal::on_urls_opened → requests_from_urls → PendingReveals → deliver → tools::go_to_in_pane`.

**Decision: a custom URL through the reveal door, not a second channel.** `RunEvent::Opened` already solves the hard
part, a cold launch (the event arrives before `setup`, `reveal/DETAILS.md` § "How a cold-launch reveal arrives"), and
the pane move. A plain open-documents event can't carry the pane or the tab; a URL can. Considered and rejected:

- **The MCP server** (localhost HTTP): needs the app already running and a token, so a cold launch would need a second
  path anyway.
- **argv to a second process** (a single-instance plugin): Tauri's would add a dependency and a second delivery path to
  keep in step with the reveal one.

**Decision: the scheme stays unregistered.** Evidence (probe app with no `CFBundleURLTypes`, macOS 27.0, 2026-10-16):
`open -a /path/Probe.app 'cmdr-cli://…'` delivered the URL to `application:openURLs:` on a cold launch (before
`applicationDidFinishLaunching`) and while running; `open 'cmdr-cli://…'` with no app named failed with
`kLSApplicationNotFoundErr`. So only a local process that names our bundle can send it, and a local process could drive
Cmdr in other ways anyway. A registered scheme would let any web page fire it (browsers ask first, but still).

**The URL is still treated as untrusted** (`open_url.rs`): only host `open` with no path, port, user, or fragment;
`path` repeats (1–10,000, each absolute, no `..`, no NUL, at most 4 KiB); `pane` is `left` or `right`; `tab` is `new`;
`pane` / `tab` at most once. Unknown parameters are ignored, so an app a version behind a freshly updated script still
opens the path. The only thing a valid URL can do is move a pane.

**Encoding.** The script percent-encodes every byte (`od` + `sed`): wasteful, but nothing (`+`, `&`, `#`, `%`, UTF-8)
can change meaning on the way, and `query_pairs` decodes it back. `script_tests.rs` pins the round trip on a filename
holding all of those.

**Which app answers.** The script resolves its own symlink chain to find the bundle it ships in and uses `open -a
<bundle>`, so the copy that shipped the script is the one that answers. `CMDR_BUNDLE_ID` overrides it (`open -b`), for
a dev build whose id is suffixed (`apps/desktop/scripts/instance-id.ts`). Outside a bundle (the source tree), it falls
back to `open -b com.veszelovszki.cmdr`.

**Paths keep symlinks as typed** (`pwd`, not `pwd -P`): `cmdr ~/link` should show `~/link`, the way the user thinks of
it.

## The menu item

App menu > "Install “cmdr” command" (macOS only). Its label follows the filesystem, three ways:

- **`NotInstalled` / `TakenByOther`**: "Install “cmdr” command". A click on `TakenByOther` explains (another `cmdr` is
  in `/usr/local/bin`) and changes nothing.
- **`Installed`**: "Uninstall “cmdr” command".
- **`InstalledByHomebrew`**: "“cmdr” command installed by Homebrew". A click explains that Homebrew manages it.

**Decision: the label is re-read, never stored.** `commands::refresh_menu_item` runs after every bar build
(`menu::install::at_startup`, `menu::rebuild`) and on every main-window focus-gain (`activate_window_menu('main')`),
since returning from a terminal is when it may have changed. The palette row (`app.toggleCliCommand`) reads the same
fact through `#lib/cli-command/`.

### Install

1. Refuse when blocked (`NotProductionBuild`, `NotInApplications`), or when the state isn't `NotInstalled`.
2. Try `symlink(own_script, /usr/local/bin/cmdr)` as the user. On Intel Macs with Homebrew the folder is often
   user-owned, and this is the whole job.
3. On `PermissionDenied` (root-owned, the default) or `NotFound` (no `/usr/local/bin`, common on Apple Silicon), run
   `mkdir -p … && ln -s …` as root behind the admin prompt. ❌ Never `ln -sf`: whatever appeared there meanwhile stays.

### Uninstall

Only when the state is `Installed`. Try `remove_file` as the user; on `PermissionDenied`, the root step is
`[ "$(readlink link)" = '<target we saw>' ] && rm link`, so a link swapped in after we looked can't be what's deleted.

**What counts as ours:** a symlink whose target ends in `.app/Contents/Resources/bin/cmdr`, and whose bundle either
carries a `com.veszelovszki.cmdr*` id or is gone (a moved or deleted Cmdr leaves exactly that dangling link, and the
user has to be able to remove it). A lookalike path inside another app's bundle is `TakenByOther`.

**Homebrew:** a link at `<prefix>/bin/cmdr` into a Cmdr bundle, with `<prefix>/Caskroom/cmdr` present, is the cask's,
for both prefixes (`/opt/homebrew`, `/usr/local`). On Intel the cask's link sits exactly where ours would, and the
Caskroom is what tells them apart. When both our link and an Apple Silicon cask's exist, ours wins: it's the one this
item made, so it's the one it can remove.

### The admin prompt

`system_host::run_as_admin` spawns `/usr/bin/osascript` with `do shell script … with prompt … with administrator
privileges`; the shell command and the prompt go in as `argv`, never spliced into the script. The prompt sentence comes
from the frontend (`main.cliCommand.adminPrompt*`) so it's in the user's language; macOS adds "Enter your password to
allow this." The script returns `0` or the AppleScript error number (`-128` is Cancel), so the outcome is typed. It runs
under `subprocess::output_within` with a 5-minute deadline that stops `osascript` (and with it the dialog).

**Decision: `osascript`, not `NSAppleScript`.** `NSAppleScript` must run on the main thread, which would freeze the app
for as long as the password dialog is up. The cost: the dialog names `osascript` rather than Cmdr in its title, which
the `with prompt` sentence makes up for.

**Decision: dev builds change nothing.** Same gate as the reveal handler (`reveal::registration::own_bundle_id`): a
build in `target/` would plant a link that dangles on the next `cargo clean`, and a dev build removing the installed
copy's link would be reaching into a setup it doesn't own. Such a build still READS the state, so its label is honest.

## Verified end to end

Bundled debug build (`CMDR_INSTANCE_ID=cmdr-cli … pnpm build --debug --bundles app`, id `com.veszelovszki.cmdr-cmdr-cli`),
driven through a symlink to the bundled script, state read over MCP (`cmdr://state`), macOS 27.0, 2026-10-16:

- Cold launch with a file: the focused pane opened its folder, cursor on the file (name with a space).
- Cold launch with `--right --new-tab <file>`: the right pane got a new tab there, cursor on the file, focus right.
- Running, `--right --new-tab <folder>`: new tab in the right pane inside the folder, focus moved right.
- Running, `--left a b <other-folder>/c`: stderr warned about the stray path; left pane on the folder, `a` and `b`
  selected, cursor on `a`.
- Running, no path, `--new-tab`: a new tab in the focused pane on the working folder.
- The bundled script kept its executable bit, `--version` read the bundle's version through the symlink, and no
  `behavior.revealActivationNoticeSeen` landed in `settings.json` (the Finder notice wasn't spent).
- App menu (read via System Events): "Install “cmdr” command" sits under "Onboarding…"; a click on this dev build shows
  the `notProductionBuild` toast and changes nothing.

## What's unverified or left out

- Linux: nothing yet. The script and the URL are macOS-shaped (`open -a`, bundles).
- The admin-prompt path needs a production build in `/Applications` and a human at the password dialog, so it's covered
  by unit tests over a fake host plus manual QA, not by an automated run. Same for the label flipping to Uninstall or
  to Homebrew's wording on a real Mac, and the cask's `binary` stanza (it needs a release that ships the script).
