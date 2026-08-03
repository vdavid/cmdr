# Cmdr IntelliJ plugin: details

Depth for `tools/intellij-plugin/CLAUDE.md`. The plan and its settled decisions live in
`docs/specs/jetbrains-plugin.md`; this file records what M0 measured, which is what M2 onward build on.

## What M0 shipped

The scaffold, both feedback-loop tiers, and one throwaway feature: `M0ProbeFoldingBuilder` folds the string literal
`'CMDR_M0_PROBE'` to `«m0»`, and nothing else. It's the smallest thing that exercises PSI access, the folding lifecycle,
and the language-registration rules all at once, which is why the spike findings below could be measured rather than
reasoned about. Delete it when M3 lands a real feature.

Not built yet, on purpose: `cmdr-plugin.json`, `CmdrProjectService`, and every feature. M0 owns no product behavior.

## Versions, and where each was checked

- **IntelliJ Platform Gradle Plugin 2.18.1**, published 2026-07-10 (Gradle Plugin Portal, checked 2026-08-03). Its own
  minimum Gradle is 9.0.0.
- **Gradle 9.6.1** via the committed wrapper.
- **Kotlin 2.4.10** (Gradle Plugin Portal, checked 2026-08-03). 2.4.20-Beta2 is newer and skipped for being a beta.
  `languageVersion` and `apiVersion` are held at 2.2, below the compiler's own default, so nothing compiles against a
  stdlib API the IDE's runtime doesn't have.
- **JDK 25** (`temurin-25.0.4+7.0.LTS`), pinned in the scoped `tools/intellij-plugin/.mise.toml`. Platform 2026.2
  requires Java 25, up from 21 in 2026.1 (JetBrains build-number-ranges doc, checked 2026-08-03).
- **The local IDE**: IntelliJ IDEA 2026.2, build `262.8665.176`, product code `IU`
  (`Contents/Resources/product-info.json`, 2026-08-03). `sinceBuild` is `262`; `untilBuild` is deliberately unset.

## The spike: how `.svelte` really parses

Measured on IDEA 2026.2 build 262.8665.176 with Svelte plugin 262.8665.173, 2026-08-03, by `SveltePsiSpikeTest` and
`LanguageCoverageSpikeTest`. Re-run them after an IDE upgrade; they print everything below.

### Question 1: does `{tString(…)}` in a template surface as JavaScript PSI?

**Yes, completely.** There is no injection and no second root to chase. Walking the real
`apps/desktop/src/lib/crash-reporter/CrashReportDialog.svelte`, the ancestry at
`{tString('crashReporter.dialog.privacyNote')}` is:

```
LeafPsiElement (JS:STRING_LITERAL, language JavaScript)
  < JSLiteralExpressionImpl   [SvelteTS]
  < JSArgumentListImpl        [SvelteTS]
  < JSCallExpressionImpl      [SvelteTS]
  < SvelteJSLazyPsiElement    [SvelteTS]
  < SvelteHtmlTag ×3          [SvelteHTML]
  < HtmlDocumentImpl          [SvelteHTML]
```

`PsiTreeUtil.getParentOfType(…, JSCallExpression::class)` crosses the lazy-parse boundary and finds the call;
`methodExpression.referenceName` is `"tString"` and `JSLiteralExpression.stringValue` is the key. So M4 matches a PSI
shape, exactly as it does for `.ts`.

**The spec's regex fallback for Svelte is not needed. Drop it.**

### Question 2: the Svelte language IDs

The plugin registers three, with these base-language chains:

- `SvelteHTML` — the file language; `*.svelte` maps to it via `SvelteHtmlFileType`.
- `SvelteJS` → `ECMAScript 6` → `JavaScript`
- `SvelteTS` → `TypeScript` → `JavaScript`

Only `SvelteHTML` matters for registration, because of the next point.

### The structural fact that decides everything

A `.svelte` file has a **`SingleRootFileViewProvider` with exactly one language, `SvelteHTML`**. The JS lives inline in
that same tree behind `SvelteJSLazyPsiElement`. The folding pass dispatches per view-provider root, so a builder
registered for `SvelteJS` or `SvelteTS` would never run on a `.svelte` file at all. Register for `SvelteHTML` and walk
down; `PsiTreeUtil.findChildrenOfType` reaches the JS nodes fine. `M0ProbeFoldingBuilder` folds both the `<script>`
occurrence and the template occurrence from that single registration, which is the proof.

### The registration rule, and the trap in it

`LanguageExtension.allForLanguage` walks the base-language chain but returns the registrations of the **nearest**
language that has any, not the union. `TypeScript`'s chain does contain `JavaScript`, and yet
`LanguageFolding.allForLanguage(TypeScript)` does not contain the platform's own `JavaScriptFoldingBuilder`: the
`TypeScript` level is occupied, so the climb stops there.

Consequence: **registering for `JavaScript` silently does nothing for `.ts` files.** Register explicitly for every
language, and add it to `LanguageCoverageSpikeTest.REGISTERED_LANGUAGES` so a missing registration fails a test rather
than quietly folding nothing.

### What M4 should therefore do

- **`.ts` and `.js`**: one Kotlin `FoldingBuilder`, registered twice in `plugin.xml`, for `JavaScript` and `TypeScript`.
  Match `JSCallExpression` (for `t` / `tString` / `getMessage`) and `JSProperty` (for `labelKey` and friends).
- **`.svelte`**: the same Kotlin class, registered for `SvelteHTML` in `cmdr-svelte.xml`. No regex, no second code path.
- **`<Trans key="…">` is the exception, and it is not JavaScript.** On the real
  `apps/desktop/src/lib/ui/LoadingIcon.svelte`, the key sits in
  `XmlAttributeValueImpl[SvelteHTML] < SvelteHtmlAttribute[SvelteHTML]`, with leaf type `XML_ATTRIBUTE_VALUE_TOKEN` in
  language `XML`. It's reachable as an ordinary `XmlAttribute` (`name == "key"`, `parent.name == "Trans"`) from the same
  `SvelteHTML` root, so it's still PSI, just a second walk in the same builder rather than a JS match.

## The feedback loop, as actually built

### Tier 1: headless fixtures. Works, and it's the loop.

`mise exec -- ./gradlew test`. 11 tests, **7.5 s cold** (full recompile plus in-process platform boot), ~2 s warm. No
window, no display, no license. Everything M3 and M4 need to assert is assertable here.

Three things cost real time to discover; none of them are guessable:

- **`CodeFoldingManager.updateFoldRegions(editor)` is the only call that populates the folding model.**
  `buildInitialFoldings(Editor)` returns `void` here and leaves the model empty; `myFixture.doHighlighting()` does
  nothing for folding either. Both failure modes look like a passing test that asserts nothing, so
  `FoldingTestCase.foldRegions()` is the only place that should ever call this.
- **`updateFoldRegionsAsync(editor, true)` throws on the EDT.** It's the one that applies default collapse state, so
  `isCollapsedByDefault` isn't observable from a region in tier 1. Assert it on the `FoldingDescriptor` instead and
  leave "a freshly opened file really shows the placeholder" to tier 2.
- **JUnit isn't on the test compile classpath.** `testFramework(TestFrameworkType.Platform)` doesn't bring it, and
  `BasePlatformTestCase` is a `junit.framework.TestCase`, so every test class fails to compile until
  `testImplementation("junit:junit:4.13.2")` is added.

### Tier 2: a real sandboxed IDE. Works, with one honest limit.

Confirmed 2026-08-03: `probe.ts` opens showing `export const marker = «m0»`, collapsed by default, with the unrelated
`'ordinary string'` on the next line untouched. That's the whole point of the tier, and it works.

```sh
cd tools/intellij-plugin
mise exec -- ./gradlew runIde &                       # ~40 s to a usable window
PID=$(ps -Ao pid,command | grep "[c]mdr-idea-plugin" | grep "Contents/Home/bin/java" | awk '{print $1}' | head -1)
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to set frontmost to true"
screencapture -x -o /tmp/tier2.png
kill $PID
```

- **Screen Recording permission is already granted**, so `screencapture` runs without a TCC prompt (verified
  2026-08-03). No workaround needed.
- **Target the process by PID, never by name.** David's real IDE is also called `idea`;
  `first process whose name is "idea"` raises his window, not the sandbox's. The `grep cmdr-idea-plugin` above is what
  disambiguates.
- **`seedIdeSandbox` writes the sandbox's `trusted-paths.xml` and the fixture's `.idea/workspace.xml`** before launch.
  Without the first, the only thing a screenshot ever captures is the modal "Trust and Open Project?" dialog.
- **Pre-seeding `workspace.xml` does NOT restore the open editor** on 2026.2; the file survives on disk untouched and
  the IDE still opens with an empty editor. Passing the file as a second launch argument (`runIde` →
  `[<projectDir>, <file>]`) is what actually opens it, and that's what `build.gradle.kts` does. It points at `probe.ts`;
  `Probe.svelte` sits in the same fixture project for sideloaded verification, per the next point.
- **The sandbox IDE runs unlicensed, so Ultimate-only plugins don't load.** The log says it plainly:
  `Plugin 'Svelte' (dev.blachut.svelte.lang) requires plugin with id=com.intellij.modules.ultimate to be enabled`, and
  `cmdr-svelte.xml` is excluded along with it. **`.svelte` folding is therefore not observable in tier 2** — a `.svelte`
  file opens with no Svelte support at all. Tier 1 has no such limit: it loads plugins from paths rather than through
  the licensing gate, which is why the Svelte spike tests pass there. Confirm Svelte behavior by sideloading into
  David's own licensed IDE (`README.md`), not by staring at `runIde`.
- Building against the local install sidesteps the licensing question for _compilation_, not for `runIde`. The spec's
  "no licensing question" line is true of the build and not of tier 2.

### Tier 3: scripted UI runs

Not built. Two reading aids don't have a UI surface worth a Starter-framework harness. See the spec.

## Decisions taken during M0

- **The Svelte plugin comes from the local IDE's plugin directory**, not the marketplace (`cmdrSveltePluginPath` in
  `gradle.properties`), so its version can never drift from the IDE we compile against. It's wired as `localPlugin` only
  when that directory exists, and the Svelte tests print a skip line rather than failing when it doesn't, so a fresh
  clone on another machine still builds green.
- **`cmdr-svelte.xml` is an optional `<depends>` config file**, not part of the main descriptor. A
  `language="SvelteHTML"` registration in `plugin.xml` would log a resolution warning on every start of an IDE without
  the Svelte plugin.
- **The Markdown plugin is already a dependency** even though nothing uses it yet. M3 needs Markdown PSI, and proving
  both bundled plugins resolve is what a scaffold milestone is for.
- **Gradle's configuration cache is on.** It's why a warm `test` is ~2 s. `seedIdeSandbox` opts out of up-to-date checks
  because it writes into the sandbox, which other tasks own.

## Where things live

- `build.gradle.kts` — the whole build. `gradle.properties` — the two machine-specific paths and the Kotlin stdlib
  opt-out.
- `src/main/resources/META-INF/plugin.xml` — the descriptor. `cmdr-svelte.xml` — the Svelte-only half.
- `src/main/kotlin/com/getcmdr/idea/m0/` — the probe. Features get sibling packages under `com/getcmdr/idea/`.
- `src/test/kotlin/com/getcmdr/idea/m0/` — `FoldingTestSupport.kt` (the `updateFoldRegions` gotcha, in one place),
  `M0ProbeFoldingTest`, `SveltePsiSpikeTest`, `LanguageCoverageSpikeTest`.
- `sandbox-project/` — the tier 2 fixture. Its `.idea/` is generated and ignored.
