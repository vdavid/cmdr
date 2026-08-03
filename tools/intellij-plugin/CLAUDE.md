# Cmdr IntelliJ plugin

Private dev tooling: reading aids for this repo inside IntelliJ IDEA Ultimate. Never published, never a build
dependency. Uninstall it and nothing about Cmdr changes. Spec: `docs/specs/jetbrains-plugin.md`. Depth, including the M0
PSI findings every later milestone rests on: `tools/intellij-plugin/DETAILS.md`.

**Status**: M0 only. The scaffold, both feedback-loop tiers, and the language spike. The one shipped extension is
`M0ProbeFoldingBuilder`, a do-nothing probe that folds the literal `'CMDR_M0_PROBE'`; delete it when a real feature
lands.

## Module map

- `build.gradle.kts` + `gradle.properties`: builds against the **local** IDE install (`cmdrIdePath`), so there's no IDE
  download. `seedIdeSandbox` pre-trusts the fixture project so `runIde` has nothing to click.
- `src/main/resources/META-INF/`: `plugin.xml` plus `cmdr-svelte.xml`, loaded only when the Svelte plugin is present.
- `src/main/kotlin/com/getcmdr/idea/`: features. `src/test/kotlin/…`: the tier 1 fixtures.
- `sandbox-project/`: the tier 2 fixture project the sandbox IDE opens.

## Guardrails

- **Run everything through mise**: `mise exec -- ./gradlew …`. The JDK is pinned in the scoped
  `tools/intellij-plugin/.mise.toml` (never the root one), and a bare `./gradlew` picks up whatever JDK is on `PATH`.
- **One `<lang.foldingBuilder>` per language.** `LanguageExtension` returns the _nearest_ non-empty level of the
  base-language chain, not the union, so a registration for `JavaScript` is invisible to `TypeScript`. Adding a language
  means adding a registration and a line in `LanguageCoverageSpikeTest.REGISTERED_LANGUAGES`.
- **`.svelte` is one `SvelteHTML` root**, never a JS root. Register for `SvelteHTML` and walk down to the JS PSI.
- **Tier 1 folding assertions need `CodeFoldingManager.updateFoldRegions`.** `buildInitialFoldings` and
  `doHighlighting()` both leave the folding model empty, which reads as a passing test asserting nothing.
- **Leave `untilBuild` open.** We target an EAP; a pinned upper bound makes the plugin vanish at the next IDE upgrade.
- **Not wired into `pnpm check`, and nothing guards the coupling.** Renaming `tString` or moving `messages/en/` silently
  stops the folding, and that's accepted. The check runner must never learn about this directory.

Human-facing build and sideload instructions: `tools/intellij-plugin/README.md`.
