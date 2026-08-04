# Cmdr IntelliJ plugin

Private dev tooling: reading aids for this repo inside IntelliJ IDEA Ultimate. Never published, never a build
dependency. Uninstall it and nothing about Cmdr changes. Spec: `docs/specs/jetbrains-plugin.md`. Depth, including the
PSI findings everything rests on: `tools/intellij-plugin/DETAILS.md`.

Two features. In `CHANGELOG.md`, the commit hashes an entry closes on render link-colored, and ⌘-click opens the commit
on GitHub. In `.ts`, `.js`, and `.svelte`, a resolvable message key folds to its English text, collapsed by default, and
⌘-click opens its catalog entry.

## Module map

- `build.gradle.kts` + `gradle.properties`: builds against the **local** IDE install (`cmdrIdePath`), so there's no IDE
  download. `seedIdeSandbox` pre-trusts the fixture project and copies the config marker into it.
- `src/main/resources/META-INF/`: `plugin.xml` plus `cmdr-svelte.xml`, loaded only when the Svelte plugin is present.
- `src/main/kotlin/com/getcmdr/idea/`: `core/` answers "is this a Cmdr checkout and what does its config say";
  `features/<name>/` is one feature. `src/test/kotlin/…`: the headless fixtures.
- `features/i18n/`: `MessageCatalogService` (the key index) and `KeySites` (the PSI shapes) are the reusable half; the
  fold and the ⌘-click are both built on them.
- `sandbox-project/`: what the sandbox IDE opens, one fixture file per feature.

## Guardrails

- **Run everything through mise**: `mise exec -- ./gradlew …`. The JDK is pinned in this directory's scoped
  `.mise.toml`, never the root one, and a bare `./gradlew` takes whatever JDK is on `PATH`.
- **Config-driven, or it silently dies.** Every extension point opens by asking `CmdrProjectService`; a feature is off
  when its `cmdr-plugin.json` section is absent. That file is both marker and config: no settings panel, no
  `PersistentStateComponent`.
- **A fold region keeps the placeholder it was built with**, so dropping the index isn't enough when copy changes under
  an open editor. `MessageCatalogService.refoldOpenEditors` gets past it, and only via `scheduleAsyncFoldingUpdate`;
  don't "simplify" that to `updateFoldRegions`.
- **A `psi.referenceContributor` never reaches Markdown**: the reference gets built and no Markdown element ever asks
  the registry for it. Use a `gotoDeclarationHandler` there. JS literals and XML attribute values do ask, so key
  navigation is a contributor, registered once with no `language` attribute for every language at once.
- **The catalog index carries no offsets.** An entry says which file a key is in; `messageDeclaration` finds the line
  through that file's JSON PSI at click time, so nothing positional can go stale.
- **A `build.gradle.kts` task action may only capture locals**, never a script-level `val`: the configuration cache
  can't serialize a script reference, and `runIde` then won't configure.
- **Tier 2 can't confirm the changelog ⌘-click**: the sandbox IDE resolves `BrowserLauncher` to the remote-development
  one, so no page opens however well it works. Assert that headless with a replaced `BrowserLauncher`. Key navigation
  stays inside the IDE, so tier 2 does see that one.
- **Synthetic input into the sandbox IDE can edit the fixture.** `git diff sandbox-project/` after a tier 2 run.
- **One `<lang.foldingBuilder>` per language**: `LanguageExtension` returns the _nearest_ non-empty level of the
  base-language chain, not the union, so `JavaScript`'s is invisible to `TypeScript`. Adding one means a line in
  `LanguageCoverageSpikeTest.FOLDING_LANGUAGES` too. `.svelte` is one `SvelteHTML` root, never a JS root: register there
  and walk down.
- **Folding assertions need `CodeFoldingManager.updateFoldRegions`**: `buildInitialFoldings` and `doHighlighting()`
  leave the model empty, which reads as a passing test that asserts nothing. `FoldingHarnessTest` pins it.
- **Leave `untilBuild` open.** We target an EAP; a pinned upper bound makes the plugin vanish at the next IDE upgrade.
- **Not wired into `pnpm check`**, and the runner must never learn about this directory. Renaming `tString` or moving
  `messages/en/` means editing `cmdr-plugin.json`; the tests read the real repo, so they fail loudly.

Human-facing build and sideload instructions: `tools/intellij-plugin/README.md`.
