# Cmdr IntelliJ plugin

A small IntelliJ IDEA plugin that makes this repo nicer to read. It's private tooling: never published to the JetBrains
Marketplace, never part of a build, and never a dependency of anything Cmdr ships. Uninstall it and the app, the
website, and every check behave exactly the same.

Right now it's a scaffold. The only thing it does is fold the literal `'CMDR_M0_PROBE'` to `«m0»`, which exists to prove
the build, the tests, and the sandbox IDE all work. The two real features (commit-hash links in `CHANGELOG.md`, and i18n
keys folded to their English text) come next.

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

This starts a second, sandboxed IDE with its own settings and plugins, opened on `sandbox-project/probe.ts`, where the
`'CMDR_M0_PROBE'` literal shows up folded as `«m0»`. It can't touch the IDE you already have running, and closing it
leaves no trace.

One thing it can't show you: the sandbox IDE starts without a license, so Ultimate-only plugins stay disabled. Svelte is
one of them, which means `sandbox-project/Probe.svelte` opens there with no Svelte support at all. To see `.svelte`
behavior, sideload the plugin into your own IDE as above. The headless tests cover Svelte either way.

## Run the tests

```sh
mise exec -- ./gradlew test
```

Headless, a few seconds, no window and no license needed.
