# Cmdr IntelliJ plugin

Private dev tooling: reading aids for this repo inside IntelliJ IDEA Ultimate. Never published, never a build
dependency. Uninstall it and nothing about Cmdr changes. Spec: `docs/specs/jetbrains-plugin.md`. Depth, including the
PSI findings every later feature rests on: `tools/intellij-plugin/DETAILS.md`.

Two features ship. In `CHANGELOG.md`, the commit hashes an entry closes on render link-colored, and ⌘-click opens the
commit on GitHub. In `.ts`, `.js`, and `.svelte`, a resolvable message key folds to its English text, collapsed by
default. ⌘-click from a key to its catalog line is next, and it reuses the folding feature's catalog index.

## Module map

- `build.gradle.kts` + `gradle.properties`: builds against the **local** IDE install (`cmdrIdePath`), so there's no IDE
  download. `seedIdeSandbox` pre-trusts the fixture project and copies the config marker into it.
- `src/main/resources/META-INF/`: `plugin.xml` plus `cmdr-svelte.xml`, loaded only when the Svelte plugin is present.
- `src/main/kotlin/com/getcmdr/idea/`: `core/` answers "is this a Cmdr checkout and what does its config say";
  `features/<name>/` is one feature. `src/test/kotlin/…`: the headless fixtures.
- `features/i18n/`: `MessageCatalogService` (the shared key index) and `KeySites` (the PSI walk), both reusable by any
  key-reading feature; `FoldedMessage` and `I18nKeyFoldingBuilder` are the folding itself.
- `sandbox-project/`: what the sandbox IDE opens, one fixture file per feature.

## Guardrails

- **Run everything through mise**: `mise exec -- ./gradlew …`. The JDK is pinned in the scoped
  `tools/intellij-plugin/.mise.toml` (never the root one), and a bare `./gradlew` picks up whatever JDK is on `PATH`.
- **Config-driven, or it silently dies.** Every extension point opens by asking `CmdrProjectService`, and a feature is
  off when its `cmdr-plugin.json` section is absent. That file is both the marker and the config; there's no settings
  panel and no `PersistentStateComponent`. The one thing config can't carry is which **languages** fold: those
  registrations are static XML.
- **A fold region keeps the placeholder it was built with.** Dropping the catalog index isn't enough when the copy
  changes under an open editor; `MessageCatalogService.refoldOpenEditors` is what gets past it, and only
  `scheduleAsyncFoldingUpdate` works. Don't "simplify" it to `updateFoldRegions`.
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
- **Not wired into `pnpm check`**, and the check runner must never learn about this directory. Renaming `tString` or
  moving `messages/en/` means editing `cmdr-plugin.json`; the tests read the real repo, so they fail loudly.

Human-facing build and sideload instructions: `tools/intellij-plugin/README.md`.
