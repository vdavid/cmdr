# Cmdr IntelliJ plugin

Private dev tooling: reading aids for this repo inside IntelliJ IDEA Ultimate. Never published, never a build
dependency. Uninstall it and nothing about Cmdr changes. Spec: `docs/specs/jetbrains-plugin.md`. Depth:
`tools/intellij-plugin/DETAILS.md`.

Two features. In `CHANGELOG.md`, the hashes an entry closes on render link-colored, and ⌘-click opens that GitHub
commit. In `.ts` and `.svelte`, a resolvable message key folds to its English text, collapsed by default, and ⌘-click
opens its catalog entry.

## Module map

- `build.gradle.kts` + `gradle.properties`: builds against the **local** IDE install (`cmdrIdePath`), so there's no IDE
  download. `seedIdeSandbox` pre-trusts the fixture project and seeds it with the marker and the IDE's license.
- `src/main/resources/META-INF/`: `plugin.xml` plus `cmdr-svelte.xml`, loaded only when the Svelte plugin is present.
- `src/main/kotlin/com/getcmdr/idea/`: `core/` answers "is this a Cmdr checkout and what does its config say";
  `features/<name>/` is one feature. `src/test/kotlin/…`: the headless fixtures.
- `features/i18n/`: `MessageCatalogService` (the key index) and `KeySites` (the PSI shapes) are the reusable half; the
  fold and the ⌘-click are both built on them.
- `sandbox-project/`: what the sandbox IDE opens, one fixture file per feature and language.

## Guardrails

- **Run everything through mise**: `mise exec -- ./gradlew …`. The JDK is pinned in this directory's scoped
  `.mise.toml`, never the root one; a bare `./gradlew` takes whatever JDK is on `PATH`.
- **Config-driven, or it silently dies.** Every extension point opens by asking `CmdrProjectService`; an absent
  `cmdr-plugin.json` section means the feature is off. That file is both marker and config: no settings panel.
- **A fold region keeps the placeholder it was built with**, so dropping the index isn't enough when copy changes under
  an open editor. `MessageCatalogService.refoldOpenEditors` gets past it, only via `scheduleAsyncFoldingUpdate`; never
  `updateFoldRegions`.
- **A `psi.referenceContributor` never reaches Markdown**: the reference gets built and nothing ever asks the registry
  for it. Use a `gotoDeclarationHandler` there. JS literals and XML attribute values do ask, so key navigation is one
  contributor with no `language` attribute, covering every language at once.
- **The catalog index carries no offsets**: `messageDeclaration` finds the line through the file's own JSON PSI at click
  time, so nothing positional goes stale.
- **A `build.gradle.kts` task action may only capture locals**, never a script-level `val`: the configuration cache
  can't serialize a script reference, and `runIde` then won't configure.
- **Tier 2 is licensed, so it sees `.svelte`**: `seedIdeSandbox` copies the IDE's `idea.key` in, without which
  Ultimate-only plugins won't load. Confirm every `.svelte` change there; headless can't see this one. It still can't
  confirm either ⌘-click (no page opens, the caret needs input), so assert those headless.
- **Don't raise the sandbox window**: it takes the keyboard from whoever's at the machine, and their typing lands in the
  fixture. `screencapture -l $(swift scripts/sandbox-window-id.swift | head -1 | cut -f1)` needs no focus.
- **Folding registration cuts both ways.** It doesn't merge _down_ the base-language chain (`JavaScript`'s is invisible
  to `TypeScript`, so register per language and list it in `LanguageCoverageSpikeTest.FOLDING_LANGUAGES`), yet a dialect
  with no registration of its own does inherit (`SvelteTS` reaches ours through `TypeScript`). So **fold `PsiFile` roots
  only**: `.svelte` offers embedded roots too, and two regions over one range leave none.
- **Folding assertions need `CodeFoldingManager.updateFoldRegions`**: `buildInitialFoldings` and `doHighlighting()`
  leave the model empty, which reads as a passing test that asserts nothing. `FoldingHarnessTest` pins it.
- **Not wired into `pnpm check`**, and the runner must never learn about this directory. Gradle doesn't track the repo
  as an input, so use **`test --rerun`** whenever it moved: that's what catches a renamed `tString` or a moved
  `messages/en/` (both live in `cmdr-plugin.json`).

Human-facing build and sideload instructions: `tools/intellij-plugin/README.md`.
