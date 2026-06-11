# Updating screenshots everywhere

How to reshoot Cmdr's product screenshots and refresh every place they appear. This is the general process; the website
hero has extra compositing on top, documented in
[`apps/website/public/hero/CLAUDE.md`](../../apps/website/public/hero/CLAUDE.md).

## Canonical masters

The pristine full-window screenshots live in [`brand/screenshots/`](../../brand/screenshots/) and are the single source
of truth:

- `app-main-dark.png` / `app-main-light.png`: the two-pane main view, full macOS window with shadow, no background.

One master pair feeds three consumers, so they never drift:

- **README**: referenced directly via a `<picture>` element (dark/light by the viewer's GitHub theme).
- **Website hero**: the compositing in `hero/CLAUDE.md` reads these masters and splits them into animated frame + pane
  layers.
- **AlternativeTo**: uploaded as-is on each refresh.

Settings and Search shots (AlternativeTo-only extras) aren't part of this pass yet. Add `settings-{dark,light}.png` when
you next refresh AlternativeTo; hold Search until it's presentable.

## Prerequisites

- [CleanShot X](https://cleanshot.com/) for the macOS window shot with shadow.
- ImageMagick (`magick` CLI), only if you're also regenerating the hero.
- The app running via `pnpm dev`.
- The Tauri MCP bridge connected: the `manage_window` and `webview_execute_js` steps below need an active
  `driver_session` on the app's actual port (a bare `driver_session start` connects to the wrong port). See
  [`docs/tooling/mcp.md`](../tooling/mcp.md) § "Tauri MCP pitfalls". `set_setting` needs the auth token, so run it via
  `./scripts/mcp-call.sh` (§ "Authentication" there).

## 1. Size and clean up the window

The window size is dictated by the hero geometry, so use it for every shot even when you only need the
README/AlternativeTo masters: that keeps one master serving all three. Via the Tauri MCP:

```
manage_window action: "resize", width: 1142, height: 705, logical: true
```

This produces a 2284 x 1410 retina window matching the hero frame proportions.

Hide the dev-mode indicators (only needed when shooting the `pnpm dev` build, not a prod build). Via Tauri MCP
`webview_execute_js` on the main window:

```js
document.querySelector('.title-bar').classList.remove('dev-mode')
document.querySelector('.title-text').textContent = 'Cmdr'
```

## 2. Set up the app so it looks nice

Using the Cmdr MCP tools. This is the state that reads well as a product shot and fits the hero crop:

- Set the accent to Cmdr gold: `set_setting id: "appearance.appColor", value: "cmdr-gold"`
- Close all but one tab on both sides (`tab close` extra tabs, or `tab close_others`).
- Left pane: navigate to `src-tauri/src`, full mode, tab lock off, cursor on `mcp`.
- Right pane: navigate to `src/lib`, brief mode, tab lock on (pinned), cursor on `indexing`.
- Hidden files visible (toggle if needed).
- Focus the left pane (`switch_pane` if needed).

The pane contents are aesthetic, not load-bearing: pick folders that look good. The window size is what matters for the
hero crop. After the shots, revert the accent: `set_setting id: "appearance.appColor", value: "system"`.

Order matters for the pinned tab: navigate first, **pin last**. Navigating a pinned tab forks a new tab (the pinned one
stays put), so pinning before you finish moving leaves you with two tabs to clean up.

If the panes sit in a git repo, the breadcrumb shows a git chip (for example `main · +5 / dirty`). For a clean
screenshot, get it to a bare `main` with reversible git state, then re-navigate each pane (parent and back) so the chip
re-reads:

```bash
git stash push -m "screenshot (pop after)"          # clears "dirty" (untracked files don't count)
git rev-parse refs/remotes/origin/main > /tmp/real-origin.sha   # save the true ref
git update-ref refs/remotes/origin/main main        # clears the ahead count (now 0 ahead/behind)
# … capture …
git update-ref refs/remotes/origin/main "$(cat /tmp/real-origin.sha)"   # restore
git stash pop
```

Gotcha: don't bother with `git branch --unset-upstream` to drop the ahead count. The chip caches the repo's upstream
config at open and won't see the unset without an app restart, whereas it reads the `origin/main` ref fresh on every
lookup. See the git backend `DETAILS.md` § Gotchas.

## 3. Capture with CleanShot (human step)

CleanShot can't be driven by an agent, so this part is yours:

- Make sure CleanShot is running.
- Open CleanShot's top menu and click **Capture Window**. Don't use the ⌘⇧4 shortcut: Cmdr's bottom bar looks different
  with Shift held.
- After capturing, click **Edit** → switch background to **None**, then **Save**.

Do it once per theme:

- Dark mode: capture, save as `brand/screenshots/app-main-dark.png` (overwrite).
- Press ⌘D to switch to light mode.
- Light mode: capture, save as `brand/screenshots/app-main-light.png` (overwrite).

## 4. Refresh each consumer

- **README**: nothing to edit. It points at `brand/screenshots/app-main-{dark,light}.png`, so replacing the files is
  enough. Commit the new PNGs.
- **Website hero**: regenerate the composited WebP layers from the new masters. Follow
  [`apps/website/public/hero/CLAUDE.md`](../../apps/website/public/hero/CLAUDE.md) § "Regenerate the layers".
- **AlternativeTo**: re-upload `app-main-{dark,light}.png` manually on the listing. (Add the settings/search shots here
  when you have them.)
