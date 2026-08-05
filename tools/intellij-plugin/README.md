# Cmdr IntelliJ plugin

A small IntelliJ IDEA plugin that makes this repo nicer to read. It's private tooling: never published to the JetBrains
Marketplace, never part of a build, and never a dependency of anything Cmdr ships. Uninstall it and the app, the
website, and every check behave exactly the same.

It does two things. In `CHANGELOG.md`, the commit hashes an entry closes on render link-colored, and ⌘-click opens the
commit on GitHub. And in `.ts` and `.svelte` files, a message key folds to the English text it resolves to, so reading a
screen's code reads like the screen:

```
const note = tString('crashReporter.dialog.privacyNote')
const note = “It includes the app version, macOS version, and which part of the code crashed. No file names…”
```

Folds open with the stock shortcuts: ⌘+ expands the one under the caret, ⌘⇧+ expands the whole file, ⌘− and ⌘⇧− collapse
again. A key built from a template can't fold, since there's nothing to look up until the code runs.

⌘-click on the key itself (once it's unfolded) opens `messages/en/<area>.json` on the line the key is defined, which is
also the line above the translator description explaining what the copy is for. It works from a `t` / `tString` /
`getMessage` call, from a `labelKey`-style property, and from `<Trans key="…">`.

Everything is off unless the open project carries `tools/intellij-plugin/cmdr-plugin.json`, which is both the marker
that says "this is a Cmdr checkout" and the config for what the features do. Open any other project and the plugin does
nothing at all.

## What you need

- **IntelliJ IDEA Ultimate.** The plugin walks JavaScript and TypeScript PSI, which Community doesn't have.
- **The IDE installed locally**, by default `/Applications/IntelliJ IDEA 2026.2 EAP.app`. The build compiles against it,
  so there's no multi-gigabyte IDE download. Point `cmdrIdePath` in `gradle.properties` somewhere else if yours lives
  elsewhere.
- **A JDK**, pinned in this directory's `.mise.toml`. Nothing to install by hand: prefix commands with `mise exec --`
  and mise fetches it the first time.

## Build and sideload

```sh
cd tools/intellij-plugin
mise exec -- ./gradlew buildPlugin
```

That writes `build/distributions/cmdr-idea-plugin-0.1.0.zip` (the version comes from `build.gradle.kts`). In the IDE, go
to Settings > Plugins, click the gear, choose "Install Plugin from Disk…", pick the zip, and restart when asked.

## Try it without installing anything

```sh
mise exec -- ./gradlew runIde
```

This starts a second, sandboxed IDE with its own settings and plugins, opened on three files from `sandbox-project/`:
`sample.ts` and `Sample.svelte`, where the message keys show up folded to their English text, and `CHANGELOG.md`, where
the eight-character trailing hashes show up link-colored. It can't touch the IDE you already have running, and closing
it leaves no trace. The sandbox does remember which folds you opened, so a second run opens the files the way you left
them rather than freshly collapsed.

The sandbox copies your IDE's license file in on the way up, so Ultimate-only plugins load there too, Svelte among them.
That's what makes `.svelte` folding visible without sideloading, and it's a read: your own IDE never notices.

One thing it can't show you: it resolves the browser launcher to the remote-development one, so ⌘-click on a hash lights
up but never opens a page. Sideload into your own IDE for that; the headless tests cover it either way.

## Run the tests

```sh
mise exec -- ./gradlew test
```

Headless, a few seconds, no window and no license needed.
