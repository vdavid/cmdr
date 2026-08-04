# Cmdr IntelliJ plugin

Private dev tooling: reading aids for this repo inside IntelliJ IDEA Ultimate. Never published, never a build
dependency. Uninstall it and nothing about Cmdr changes. Spec: `docs/specs/jetbrains-plugin.md`. Depth, including the
PSI findings every later feature rests on: `tools/intellij-plugin/DETAILS.md`.

One feature ships: in `CHANGELOG.md`, the commit hashes an entry closes on render link-colored, and ⌘-click opens the
commit on GitHub. Folding i18n keys to their English text is next, and `cmdr-plugin.json` already carries its config.

## Module map

- `build.gradle.kts` + `gradle.properties`: builds against the **local** IDE install (`cmdrIdePath`), so there's no IDE
  download. `seedIdeSandbox` pre-trusts the fixture project and copies the config marker into it.
- `src/main/resources/META-INF/`: `plugin.xml` plus `cmdr-svelte.xml`, loaded only when the Svelte plugin is present.
- `src/main/kotlin/com/getcmdr/idea/`: `core/` answers "is this a Cmdr checkout and what does its config say";
  `features/<name>/` is one feature. `src/test/kotlin/…`: the headless fixtures.
- `sandbox-project/`: the fixture project the sandbox IDE opens.

## Guardrails

- **Run everything through mise**: `mise exec -- ./gradlew …`. The JDK is pinned in the scoped
  `tools/intellij-plugin/.mise.toml` (never the root one), and a bare `./gradlew` picks up whatever JDK is on `PATH`.
- **Config-driven, or it silently dies.** Every extension point opens by asking `CmdrProjectService`, and a feature is
  off when its `cmdr-plugin.json` section is absent. That file is both the marker and the config; there's no settings
  panel and no `PersistentStateComponent`.
- **A `psi.referenceContributor` never reaches Markdown.** The contributor runs and the reference is built, but no
  Markdown PSI element asks the registry for it, so `findReferenceAt` stays null. Use a `gotoDeclarationHandler`.
- **A task action in `build.gradle.kts` may only capture locals**, never a script-level `val`: Gradle's configuration
  cache can't serialize a script reference, and `runIde` then fails to configure at all.
- **Tier 2 can't confirm the ⌘-click.** The sandbox IDE resolves `BrowserLauncher` to the remote-development backend
  one, so nothing opens however well the feature works. Assert it headless with a replaced `BrowserLauncher`.
- **Synthetic input into the sandbox IDE can edit the fixture.** `git diff sandbox-project/` after a tier 2 run.
- **One `<lang.foldingBuilder>` per language.** `LanguageExtension` returns the _nearest_ non-empty level of the
  base-language chain, not the union, so a registration for `JavaScript` is invisible to `TypeScript`. Adding a language
  means adding a registration and a line in `LanguageCoverageSpikeTest.FOLDING_LANGUAGES`.
- **`.svelte` is one `SvelteHTML` root**, never a JS root. Register for `SvelteHTML` and walk down to the JS PSI.
- **Folding assertions need `CodeFoldingManager.updateFoldRegions`.** `buildInitialFoldings` and `doHighlighting()` both
  leave the folding model empty, which reads as a passing test asserting nothing. `FoldingHarnessTest` pins it.
- **Leave `untilBuild` open.** We target an EAP; a pinned upper bound makes the plugin vanish at the next IDE upgrade.
- **Not wired into `pnpm check`, and nothing guards the coupling.** Renaming `tString` or moving `messages/en/` silently
  stops the folding, and that's accepted. The check runner must never learn about this directory.

Human-facing build and sideload instructions: `tools/intellij-plugin/README.md`.
