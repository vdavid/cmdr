# Cmdr IntelliJ plugin: details

Depth for `tools/intellij-plugin/CLAUDE.md`. The plan and its settled decisions live in
`docs/specs/jetbrains-plugin.md`; this file records what was measured on the real platform, which is what the code rests
on.

## What ships

- **The core.** `CmdrProjectService` answers "is this project a Cmdr checkout, and what does its config say" by locating
  `tools/intellij-plugin/cmdr-plugin.json` under the project base dir. `CmdrPluginConfig` parses it into sections that
  features read through their own `FeatureConfig`.
- **Commit-hash links.** In the files `changelog.files` names, the hashes closing an entry render link-colored and
  ⌘-click opens the commit on GitHub.
- **i18n key preview.** A message key the English catalog resolves folds to its text, collapsed by default, in `.ts`,
  `.js`, and `.svelte`.
- **i18n key navigation.** ⌘-click on that key opens `messages/en/<area>.json` at the line it's defined on, which is
  also the line above its translator description.

Nothing else.

## The core seam

`CmdrProjectService.config` is `null` outside a Cmdr checkout, which is what keeps the plugin inert everywhere else.
Detection is the marker file's presence and nothing else: no project-name matching, no absolute paths, so a worktree, a
second clone, or a directory called anything at all is recognized for free. `CmdrProjectServiceTest` pins that by
running against a fixture project in a temp directory under a generated name.

Locating the file happens on every call (two VFS lookups), so a marker that appears or disappears with a branch switch
is picked up with no listener to keep in sync. Only the **parse** is cached, keyed by the file's modification stamp.

Adding a feature is three things and no core changes:

1. A package under `features/<name>/`.
2. A `FeatureConfig<T>` object, normally the feature's config companion, naming its section:
   `companion object : FeatureConfig<ChangelogConfig>("changelog")`. `config.get(ChangelogConfig)` then reads that
   section, memoized per config instance and typed by the key, and returns `null` when the section is absent. **Absent
   section means the feature is off**; that's the whole toggle story.
3. Registrations in `plugin.xml` (or `cmdr-svelte.xml` for anything `SvelteHTML`).

Core never learns what a section means, which is why a section written for a later build loads fine today.

**Registration is the one thing config can't drive.** `<lang.foldingBuilder>` and friends are static XML read at plugin
load, so a `languages` list in `cmdr-plugin.json` could only ever be decoration; it was in the plan and isn't in the
file. The languages live in `plugin.xml` and `cmdr-svelte.xml`, and `LanguageCoverageSpikeTest.FOLDING_LANGUAGES` is
what fails when one goes missing.

## Commit-hash links

`ChangelogRefs` is the pure text rule: a parenthesized, comma-separated group of `[0-9a-f]{8}` hashes anchored to the
**end** of a logical entry. Anchoring is the whole safety story, since entries close on asides like `(~40x speed-up!)`.
`ChangelogLinks.commitLinksIn` is the PSI-side bridge both extension points call, so the annotator's color and the goto
handler's target can't disagree about which hashes are links.

**Markdown does the entry-joining for us.** A bullet's `MarkdownParagraph` already spans the entry's wrapped source
lines, newlines and continuation indent included, and a nested bullet is its own list item rather than part of its
parent's paragraph. So there's no line-rejoining here, unlike `scripts/check/checks/changelog-commit-links.go` and
`apps/website/src/lib/changelog.ts`, which read raw lines. `ChangelogRefLinkTest` pins that platform assumption on real
PSI, because if Markdown ever stops doing it, wrapped entries silently stop matching.

### Why a goto-declaration handler and not a `PsiReferenceContributor`

The spec called for a contributed `WebReference`, which is the obvious shape. **It never reaches Markdown.** Measured on
IDEA 2026.2 build 262.8665.176, 2026-08-04, with a throwaway contributor registered for `language="Markdown"` over
`MarkdownParagraph`:

- `ReferenceProvidersRegistry.getReferencesFromProviders(paragraph)` → **1** reference. The contributor runs and builds
  the `WebReference` correctly.
- `paragraph.getReferences()` → **0**, and `file.findReferenceAt(offset)` → `null`.

`PsiElementBase.getReferences()` doesn't consult the registry; asking it is a per-implementation opt-in, and Markdown's
PSI doesn't take it. So the references exist and nothing ever looks at them. To re-measure after an IDE upgrade, add a
`PsiReferenceContributor` over `MarkdownParagraph`, register it for `language="Markdown"`, and print those three values.

**This is a fact about Markdown, not about references.** JavaScript literals and XML attribute values do take the
opt-in, which is why i18n key navigation is a contributor rather than a second handler; § i18n key navigation has the
same three numbers for those hosts.

`ChangelogRefGotoDeclarationHandler` is what reaches the editor instead. The `WebReference` is still the platform's own
machinery, used directly for the navigation target it builds, and `ChangelogRefLinkTest` asserts the platform's own
goto-declaration lookup finds it.

### The color

`CodeInsightColors.HYPERLINK_ATTRIBUTES` via a silent `INFORMATION` annotation, so a commit ref looks like every other
link in the editor and follows the theme. Confirmed rendering in tier 2.

### What it costs

Measured 2026-08-04 on the real 2,054-line `CHANGELOG.md` (1,134 paragraphs, ~1,000 of them ending on a group), in a
headless fixture: **4.5 ms** for `commitLinksIn` over every paragraph, of which 2.5 ms is `changelogConfigFor` and the
rest the regex. Highlighting the whole file goes from 348 ms to 864 ms with the plugin active, so the ~500 ms is the
platform materializing a thousand annotations, not our lookup. Nothing here is worth caching harder;
`testTheRealChangelogGetsItsLinks` keeps the real file in the loop.

## i18n key preview

`MessageCatalogService` is the index: the JSON files `catalogGlob` matches, flattened into one map of key to
`Message(text, fileName)`, with `@key` metadata dropped the way `stripMetadata` in
`apps/desktop/src/lib/intl/messages.svelte.ts` drops it. English only, cached per project, keyed by the glob it came
from. **It's the reusable half**: `catalog()` answers "what does this key say", `sourceOf(key)` answers "which file is
it written in", and `keySitesIn` / `keySiteFor` are the matching PSI walk, which hands back both the element to fold and
the literal the key is written in.

`I18nKeyFoldingBuilder` turns a site plus a resolved message into a `FoldingDescriptor`. Three shapes, all from config:

- A call to a `functions` name whose first argument is a **quoted** literal. A template literal is excluded explicitly,
  because `JSLiteralExpression.isQuotedLiteral` counts backticks and a substitution-free template still has a
  `stringValue`. Keys built by template are the accepted miss.
- A `keyProperties` property with a string-literal value.
- A `componentAttributes` attribute, matched as an ordinary `XmlAttribute` by tag and attribute name.

A call folds whole; a property and an attribute fold their value only, so `labelKey:` stays visible to say which slot of
a settings definition the copy fills. `FoldedMessage` decides how the text reads: curly quotes, ICU's doubled
apostrophes collapsed, `{placeholder}` and `<tag>` markers verbatim, newlines to spaces, and never truncated.

### Keeping an open editor honest

The catalog cache is dropped by a `BulkFileListener` on any VFS event under the catalog directory. **That isn't enough
on its own**: a fold region that already exists keeps the placeholder it was built with, so an editor open when the copy
changed goes on showing the old sentence. Measured 2026-08-04 on IDEA 2026.2 build 262.8665.176, with the index proven
fresh and the builder returning the new text:

- `CodeFoldingManager.updateFoldRegions(editor)` → old text.
- `releaseFoldings(editor)` then `updateFoldRegions` → old text.
- `PsiManager.dropPsiCaches()`, and `DaemonCodeAnalyzer.restart()` → old text.
- Removing every region by hand then `updateFoldRegions` → **no regions at all**.
- Touching the `.ts` document → new text, which is why the staleness is invisible while you're typing in the file.
- `CodeFoldingManager.scheduleAsyncFoldingUpdate(editor)` → **new text**, and it's public API.

So `MessageCatalogService.refoldOpenEditors` schedules that for every editor of the project.
`I18nKeyFoldingTest.testTheCatalogReloadsAfterTheJsonChangesOnDisk` asserts both halves, so an IDE upgrade that changes
this goes red rather than quietly serving yesterday's copy.

### What it costs

Measured 2026-08-04 in a headless fixture, against the repo's real files. Ranges are across runs: the low end is a warm
JVM mid-suite, the high end a cold one, so the warm number is what an editor session feels.

The catalog is 31 files and 2,700 keys: **7–52 ms** to parse from text, once per project, then free until it changes.

Folding one file, warm (five runs, mean), with the first pass in parentheses:

- `settings/definitions/advanced.ts`, 406 lines, 74 folds: **1–3 ms** (23–125 ms, catalog build included).
- `ipc/bindings.ts`, 9,001 lines, no keys at all, so pure walk cost: **3–5 ms** (39–87 ms).
- `file-explorer/pane/FilePane.svelte`, 1,972 lines, 5 folds: **1 ms** (60–167 ms, the high figure being the lazy-parsed
  script blocks expanded once).

The first-pass numbers are one-time per file and overlap work highlighting does anyway. `testFoldingTheRealRepoIsCheap`
keeps all three in the loop with a 50 ms warm budget, well above the numbers and well below anything that would be felt.

## i18n key navigation

⌘-click on a key opens the catalog file at the entry, which puts the translator's `@key` description one line below the
caret. It reuses the folding feature's index and its key-site matchers whole; the only new question is where in a file a
key sits.

**The index answers which file, the file's own PSI answers where in it.** A catalog entry carries
`Message(text, fileName)` and no offset. `messageDeclaration` turns that into the exact line at click time:
`PsiManager.findFile(…) as JsonFile`, then `JsonObject.findProperty(key)`. Two properties fall out of that split. The
index stays one Gson parse of text, with nothing positional in it to keep in step with a file that's being edited; and
the line that reaches the editor is the file as it is now, not as it was when the index was built. It costs a JSON PSI
parse of one file per click, which the platform then caches.

**A `psi.referenceContributor`, not a `gotoDeclarationHandler`.** A contributed reference is the platform's own shape
for "this text points at that declaration", and it carries the ⌘-hover underline, ⌘B, quick definition, and Find Usages
along with the click. The changelog feature has a handler only because Markdown never asks the reference registry;
that's a fact about Markdown, and it doesn't hold here. Measured 2026-08-04 on IDEA 2026.2 build 262.8665.176 by
`I18nKeyNavigationTest.testTheHostPsiReallyAsksTheReferenceRegistry`, which prints both numbers for every host:

- `.ts` and `.svelte`, `JSLiteralExpressionImpl`: registry **1**, `element.references` **1**.
- `.svelte`, `XmlAttributeValueImpl` (`<Trans key="…">`): registry **1**, `element.references` **1**.
- Markdown, for contrast: registry 1, `element.references` **0**, and `findReferenceAt` null.

**One registration covers every language**, unlike the folding builder. A contributor with no `language` attribute
registers for `Language.ANY`, and `ReferenceProvidersRegistryImpl` builds each language's registrar from
`allForLanguageOrAny`, so the ANY registrations are merged in everywhere rather than shadowed. The `.svelte` half needs
nothing in `cmdr-svelte.xml`; the patterns (`JSLiteralExpression`, `XmlAttributeValue`) are what narrow it.

**A key that doesn't resolve gets no reference at all**, rather than an unresolved one. So a renamed key reads as
ordinary text: no underline promising a jump that can't happen. That's also why the provider asks `catalog()` before
building anything. The reference is soft on top of that, so nothing paints it red.

`keySiteFor` is the walk-up counterpart to `keySitesIn`'s walk-down, over the same three matchers, because a gesture
knows one element rather than a file. It insists the site's own `keyElement` be the element asked about, which is what
keeps a click on a call's second argument from resolving the first one's key.

### What it costs

Measured 2026-08-04 in a headless fixture, resolving a real key against the repo's real catalog: **18–22 ms** for the
first click, which builds the 31-file index and parses the JSON file it lands in, and **under 1 ms** for every later
one. `testNavigatingTheRealCatalogIsCheap` keeps that in the loop. Folding is unaffected, since nothing was added to the
index build: it still measures 1–3 ms warm per file.

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

## How `.svelte` really parses

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
`methodExpression.referenceName` is `"tString"` and `JSLiteralExpression.stringValue` is the key. So i18n folding
matches a PSI shape in `.svelte` exactly as it does in `.ts`, and needs no regex fallback.

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
down; `PsiTreeUtil.findChildrenOfType` reaches the JS nodes fine, which is what
`SveltePsiSpikeTest.testTemplateExpressionSurfacesAsJavaScriptPsi` asserts by pulling both the `<script>` and the
template key out of one `SvelteHTML` root.

### The registration rule, and the trap in it

`LanguageExtension.allForLanguage` walks the base-language chain but returns the registrations of the **nearest**
language that has any, not the union. `TypeScript`'s chain does contain `JavaScript`, and yet
`LanguageFolding.allForLanguage(TypeScript)` does not contain the platform's own `JavaScriptFoldingBuilder`: the
`TypeScript` level is occupied, so the climb stops there.

Consequence: **registering for `JavaScript` silently does nothing for `.ts` files.** Register explicitly for every
language, and add it to `LanguageCoverageSpikeTest.FOLDING_LANGUAGES` so a missing registration fails a test rather than
quietly folding nothing.

### What i18n folding does with all that

- **`.ts` and `.js`**: one Kotlin `FoldingBuilder`, registered twice in `plugin.xml`, for `JavaScript` and `TypeScript`.
  It matches `JSCallExpression` (for `t` / `tString` / `getMessage`) and `JSProperty` (for `labelKey` and friends).
- **`.svelte`**: the same Kotlin class, registered for `SvelteHTML` in `cmdr-svelte.xml`. No regex, no second code path.
  Registering there joins the Svelte plugin's own `SvelteFoldingBuilder` rather than shadowing anything, confirmed by
  the list `LanguageCoverageSpikeTest` prints.
- **`<Trans key="…">` is the exception, and it is not JavaScript.** On the real
  `apps/desktop/src/lib/ui/LoadingIcon.svelte`, the key sits in
  `XmlAttributeValueImpl[SvelteHTML] < SvelteHtmlAttribute[SvelteHTML]`, with leaf type `XML_ATTRIBUTE_VALUE_TOKEN` in
  language `XML`. It's reachable as an ordinary `XmlAttribute` (`name == "key"`, `parent.name == "Trans"`) from the same
  `SvelteHTML` root, so it's still PSI, just another branch of the same walk rather than a JS match.

## The feedback loop, as actually built

### Tier 1: headless fixtures. Works, and it's the loop.

`mise exec -- ./gradlew test`. 80 tests, ~10 s cold, ~2 s warm. No window, no display, no license. Everything the
features need to assert is assertable here, **including both ⌘-clicks**: for the changelog, replace the application's
`BrowserLauncher` with a recorder, run `IdeActions.ACTION_GOTO_DECLARATION`, and assert the URL it was asked to open;
for a message key, `GotoDeclarationAction.findAllTargetElements` returns the catalog entry itself.

Four things cost real time to discover; none of them are guessable:

- **`CodeFoldingManager.updateFoldRegions(editor)` is the only call that populates the folding model.**
  `buildInitialFoldings(Editor)` returns `void` here and leaves the model empty; `myFixture.doHighlighting()` does
  nothing for folding either. Both failure modes look like a passing test that asserts nothing, so
  `FoldingTestCase.foldRegions()` is the only place that should ever call this and `FoldingHarnessTest` is what keeps
  the harness honest.
- **`updateFoldRegionsAsync(editor, true)` throws on the EDT.** It's the one that applies default collapse state, so
  `isCollapsedByDefault` isn't observable from a region in tier 1. Assert it on the `FoldingDescriptor` instead and
  leave "a freshly opened file really shows the placeholder" to tier 2.
- **JUnit isn't on the test compile classpath.** `testFramework(TestFrameworkType.Platform)` doesn't bring it, and
  `BasePlatformTestCase` is a `junit.framework.TestCase`, so every test class fails to compile until
  `testImplementation("junit:junit:4.13.2")` is added.
- **`BrowserLauncher.getInstance()` is `com.jetbrains.rdserver.browsers.BackendBrowserLauncher`** here, not a local
  launcher, which is why the click has to be asserted through a replaced service rather than by watching for a browser.

### Tier 2: a real sandboxed IDE. Works, with honest limits.

Confirmed 2026-08-04: `CHANGELOG.md` opens with every eight-character trailing hash link-colored, the seven-character
one and the `(~40x speed-up!)` and mid-sentence `(deadbeef)` asides left as plain prose, and ⌘-hover over a hash showing
the platform's own "Open in browser" affordance.

Confirmed 2026-08-04 for the folding too: `sample.ts` opens with every resolvable key already collapsed to its sentence
in curly quotes, `labelKey:` and `descriptionKey:` still showing their names in front of the folded value, `Here''s`
rendering as `Here's`, `{countText}` intact, and the unknown key, the bare string, and the template-built key left as
source. `runIde` opens both fixture files; the last one in the list is the focused tab.

Confirmed 2026-08-04 for key navigation, one half of it: with the caret inside a key the catalog doesn't have, Navigate
→ Declaration or Usages says "Cannot find declaration to go to" and nothing else happens, which is the must-not-throw
case. **The positive half was not confirmed in tier 2**, because getting the caret into a resolvable key needs input,
and driving input turned out to be the wrong thing to do on this machine; see the focus-stealing gotcha below. Tier 1
asserts the same platform call the menu item runs (`GotoDeclarationAction.findAllTargetElements` → the `JsonProperty`),
including against the repo's real catalog.

```sh
cd tools/intellij-plugin
mise exec -- ./gradlew runIde &                       # ~40 s to a usable window
PID=$(ps -Ao pid,command | grep "[c]mdr-idea-plugin" | grep "Contents/Home/bin/java" | awk '{print $1}' | head -1)
osascript -e "tell application \"System Events\" to tell (first process whose unix id is $PID) to set frontmost to true"
screencapture -x -o /tmp/tier2.png
kill $PID
```

**Capture the window, not the screen.** `screencapture -x -o -l <window-id>` grabs the sandbox IDE alone even when it's
buried, so a screenshot can't pick up whatever David has open. There's no CLI that prints window IDs and the system
Python has no `Quartz`, so get one from a four-line Swift script over `CGWindowListCopyWindowInfo` filtered by window
name (`sandbox-project – …`); `swift <file>.swift` runs it with no project. Raising the window first is then only about
keyboard focus, never about the picture.

- **Check for a sandbox IDE that's already running before launching one.** A stale instance from a previous session
  keeps its old plugin build, logs `Failed to unload modified plugins`, and every screenshot then shows code that isn't
  yours. `ps` for `cmdr-idea-plugin` first, and read `.intellijPlatform/sandbox/…/log/idea.log` if anything looks off.
- **A ⌘-hover only fires if the mouse moves while the modifier is already down.** Pressing ⌘ with the pointer already
  parked on the target does nothing, which reads exactly like a broken feature.
- **Raising the sandbox IDE steals the keyboard from whoever is at the machine**, and their typing lands in the fixture
  file. A 2026-08-04 session watched `a.`, `e.g`, and `hours.` appear inside `sample.ts` seconds apart while the sandbox
  window held focus, one of them saved to disk. So: don't raise the window unless you need to, do it for as few seconds
  as possible, `git diff sandbox-project/` afterwards, and treat a confirmation you can only get by holding focus as one
  to leave to tier 1. Undo through the menu (Edit → Undo Typing) rather than ⌘Z; sending more keystrokes into a window
  that's already collecting someone else's is how a one-character mess becomes a five-character one.
- **Synthetic input can edit the fixture.** A tier 2 session left a stray `****` inside a hash in
  `sandbox-project/CHANGELOG.md` and it got committed. `git diff sandbox-project/` when you're done.
- **`System Events`' `click at {x, y}` does nothing to the IDE**, silently, which reads as a click that missed. A real
  click needs a posted `CGEvent` (`.leftMouseDown` / `.leftMouseUp` at `.cghidEventTap`), again a few lines of Swift.
  Calibrate before trusting the coordinates: the window's AX `position` and the `-l` screenshot don't share an origin,
  and the first measured offset was three editor lines.
- **`runIde` arguments don't place the caret.** `--line` / `--column` ahead of the file path (the `idea` launcher's own
  options) left the caret at the position the previous session had remembered, so there's no input-free way to put it
  inside a key. Folding state is remembered per file the same way, so a second run doesn't open collapsed.
- **Screen Recording permission is already granted**, so `screencapture` runs without a TCC prompt (verified
  2026-08-03). No workaround needed.
- **Target the process by PID, never by name.** David's real IDE is also called `idea`;
  `first process whose name is "idea"` raises his window, not the sandbox's. The `grep cmdr-idea-plugin` above is what
  disambiguates.
- **`seedIdeSandbox` writes the sandbox's `trusted-paths.xml`** before launch, and copies `cmdr-plugin.json` into the
  fixture project. Without the trust seeding, the only thing a screenshot ever captures is the modal "Trust and Open
  Project?" dialog; without the marker, every feature correctly does nothing, which looks exactly like a broken plugin.
  The marker is copied rather than committed so the fixture can't drift from the config the repo ships.
- **Don't reach for `.idea/workspace.xml` to pre-open a file.** It looks like the tidy way and it doesn't work: on
  2026.2 the seeded file survives on disk untouched and the IDE still opens an empty editor. Passing the file as a
  second launch argument (`runIde` → `[<projectDir>, <file>]`) is what opens it.
- **The sandbox IDE runs unlicensed, so Ultimate-only plugins don't load.** The log says it plainly:
  `Plugin 'Svelte' (dev.blachut.svelte.lang) requires plugin with id=com.intellij.modules.ultimate to be enabled`, and
  `cmdr-svelte.xml` is excluded along with it. **`.svelte` behavior is therefore not observable in tier 2.** Tier 1 has
  no such limit: it loads plugins from paths rather than through the licensing gate, which is why the Svelte spike tests
  pass there. Confirm Svelte behavior by sideloading into David's own licensed IDE (`README.md`).
- Building against the local install sidesteps the licensing question for _compilation_, not for `runIde`. The spec's
  "no licensing question" line is true of the build and not of tier 2.

### Tier 3: scripted UI runs

Not built. Two reading aids don't have a UI surface worth a Starter-framework harness. See the spec.

## Build decisions

- **A task action may only capture local `val`s.** Gradle's configuration cache is on, and a `doLast` or a
  `CommandLineArgumentProvider` that reads a script-level `val` serializes a reference to the build script itself, which
  the cache refuses: `runIde` then fails to configure with "cannot serialize Gradle script object references" and never
  launches. Copy what the action needs into locals first.
- **The Svelte plugin comes from the local IDE's plugin directory**, not the marketplace (`cmdrSveltePluginPath` in
  `gradle.properties`), so its version can never drift from the IDE we compile against. It's wired as `localPlugin` only
  when that directory exists, and the Svelte tests print a skip line rather than failing when it doesn't, so a fresh
  clone on another machine still builds green.
- **`cmdr-svelte.xml` is an optional `<depends>` config file**, not part of the main descriptor. A
  `language="SvelteHTML"` registration in `plugin.xml` would log a resolution warning on every start of an IDE without
  the Svelte plugin, so everything naming a Svelte language lives there.
- **Three bundled plugins are dependencies**: JavaScript for the i18n features, Markdown for the commit links, and JSON
  for the catalog entries a key navigates into. All three ship with the IDE, so none is a download or a runtime cost.
- **The configuration cache stays on.** It's why a warm `test` is ~2 s. `seedIdeSandbox` opts out of up-to-date checks
  because it writes into the sandbox, which other tasks own.

## Where things live

- `build.gradle.kts` — the whole build. `gradle.properties` — the two machine-specific paths and the Kotlin stdlib
  opt-out.
- `src/main/resources/META-INF/plugin.xml` — the descriptor. `cmdr-svelte.xml` — the Svelte-only half.
- `src/main/kotlin/com/getcmdr/idea/core/` — `CmdrProjectService`, `CmdrPluginConfig`, `FeatureConfig`.
  `features/changelog/` — the commit links. `features/i18n/` — `MessageCatalog` and its service (the index), `KeySites`
  (the PSI shapes), `I18nKeyFoldingBuilder` plus `FoldedMessage` (the fold), and `I18nKeyReferenceContributor` plus
  `MessageDeclaration` (the ⌘-click). New features get sibling packages under `features/`.
- `src/test/kotlin/com/getcmdr/idea/` — `RepoFiles` (reads real repo files through the `cmdr.repo.root` system
  property), `core/`, `features/changelog/`, `features/i18n/` (with `CatalogFixture.kt`, the fixture project both i18n
  tests set up), and `platform/` (`FoldingTestSupport.kt` with the `updateFoldRegions` gotcha and its harness test, plus
  the two spikes).
- `sandbox-project/` — the tier 2 fixture: `CHANGELOG.md` for the links, `sample.ts` plus
  `apps/desktop/src/lib/intl/messages/en/sandbox.json` for the folding. Its `.idea/` and the copied marker are generated
  and ignored.
