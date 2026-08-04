# Cmdr IntelliJ plugin

A small IntelliJ IDEA plugin that makes this repo nicer to read. It's private tooling: never published to the JetBrains
Marketplace, never part of a build, and never a dependency of anything Cmdr ships. Uninstall it and the app, the
website, and every check behave exactly the same.

Today it does one thing: in `CHANGELOG.md`, the commit hashes an entry closes on render link-colored, and ⌘-click opens
the commit on GitHub. Folding i18n keys to their English text comes next.

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

That writes `build/distributions/cmdr-idea-plugin-<version>.zip`. In the IDE, go to Settings > Plugins, click the gear,
choose "Install Plugin from Disk…", pick the zip, and restart when asked.

## Try it without installing anything

```sh
mise exec -- ./gradlew runIde
```

This starts a second, sandboxed IDE with its own settings and plugins, opened on `sandbox-project/CHANGELOG.md`, where
the eight-character trailing hashes show up link-colored. It can't touch the IDE you already have running, and closing
it leaves no trace.

Two things it can't show you. The sandbox IDE starts without a license, so Ultimate-only plugins stay disabled, Svelte
among them. And it resolves the browser launcher to the remote-development one, so ⌘-click on a hash lights up but never
opens a page. Sideload into your own IDE for the real thing; the headless tests cover both either way.

## Run the tests

```sh
mise exec -- ./gradlew test
```

Headless, a few seconds, no window and no license needed.
