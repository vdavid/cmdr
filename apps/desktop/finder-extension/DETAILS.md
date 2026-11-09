# Finder extension: architecture and decisions

Must-knows: `CLAUDE.md`. The service it calls and the delivery behind it: `src-tauri/src/reveal/DETAILS.md` § "Open in
Cmdr". Why it exists and David's decisions: [#853](https://github.com/getcmdr/cmdr/issues/853).

## Why it calls the service

Measured with a throwaway sandboxed extension on macOS 27.0.1, 2026-11-02:

- **`NSWorkspace.open([url], withApplicationAt: host)` with an unclaimed scheme fails**: `-10820`,
  `kLSAppDoesNotClaimTypeErr`, "sandboxed callers only". That's the transport the `cmdr` command uses, and it's closed
  to an extension. Registering the scheme would open it to every web page and app, which `src-tauri/src/cli/DETAILS.md`
  rejects on purpose.
- **`NSPerformService` works**: the host received the paths on the pasteboard, with no scheme and no entitlement beyond
  the sandbox. A distributed notification also reached the host, but it can't launch a Cmdr that isn't running.

**Decision: the extension is a thin button over the service.** One delivery path, and the extension holds no logic and
needs no file access. ❗ If someone switches the service off in System Settings, this item stops working too, with a log
line in the extension's process. That's accepted: they turned off this very action.

## Translation

The item follows the system's language, with English where the system's language isn't one Cmdr ships. Cmdr's own
language setting can't reach it: macOS reads these titles from files, and nothing lets a sandboxed extension read Cmdr's
settings without an app group. Both titles come from `settings.finderExtension.card`, through `pnpm intl:native-strings`
(guarded by `native-strings-fresh`).

- **The extension's item**: a `Localizable.strings` per language in its own bundle, the standard way. An app bundle
  holding the same `Resources` resolved `de`, `uk`, `zh-TW` → `zh-Hant`, `pt-BR` → `pt`, and `ja` → English
  (`-AppleLanguages`, macOS 27.0.1, 2026-11-02). Test it with the strings in the MAIN bundle: a non-main one follows the
  process's language.
- **The service's item**: language keys in the `NSMenuItem` dictionary in `src-tauri/Info.plist`. `pbs -dump_pboard`
  keeps them, and it's the same dictionary `ServicesMenu.strings` feeds (`en = …` beside `default`). That Finder picks
  the right one hasn't been seen on a non-English Mac: if it doesn't, the title is English.
- **Decision: no `ServicesMenu.strings`, though it's Apple's documented way.** It needs `.lproj` folders in the MAIN
  bundle, and those decide the language of everything AppKit and WebKit draw in Cmdr (open panels, text menus), which
  today is English and unaffected by Cmdr's language setting. That's a separate change with its own testing. It's also
  where to go if the plist keys turn out not to work. A lone `de.lproj` was read as `en` by `pbs`, so it'd need
  `en.lproj` too (macOS 27.0.1, 2026-11-02).
- **`NSPerformService` takes the English title in any language**, and refuses a translated one (a probe app with
  `-AppleLanguages (de)`, macOS 27.0.1, 2026-11-02). That's why `serviceName` is fixed.

## Costs, and why it's opt-in

- **Off by default in macOS.** The Settings row switches it through `pluginkit` (`-e use` and `-e ignore`).
- **Always running once on**: one process for as long as Finder runs, plus one per Open or Save dialog in any app. A
  probe extension measured about 40 MB of resident memory on macOS 27.0.1 (`ps`, 2026-11-02); three other apps'
  extensions measured 12–27 MB.
- **Startup disk only**: monitoring doesn't cross drive boundaries, and each external or network drive listed shows the
  app's icon in Finder's sidebar (FB16171915). The service covers those drives.
- **Reportedly no menu in File Provider folders** (iCloud Drive, Dropbox, Google Drive, OneDrive). The service covers
  those too.
- **It also shows in other apps' Open and Save dialogs.** Decision: left in. It works there, and hiding it would take a
  check on which app is hosting the extension, for little gain.

## Release

- `compile` job: `build.sh compile` per arch, uploaded as the `finder-extension-<arch>` artifact. It stays in this job
  because the `sign` job compiles nothing.
- `sign` job: fetches the slices, checks each is one file of the right arch, then `build.sh assemble` signs with the
  Developer ID, hardened runtime, and a timestamp. `tauri bundle -c` adds the signed `.appex` under
  `bundle.macOS.files`. The bundler copies files as they are and signs the app around them, the same way the
  llama-server binaries go in.
- A plain `tauri build` carries no extension, so dev, worktree, and E2E bundles can't register one.
