# Cmdr IntelliJ plugin

**Status**: specced, not started. **Owner**: David. **Date**: 2026-08-03.

One JetBrains plugin that carries Cmdr-specific editor affordances, built so that feature number three is a new
directory rather than a redesign. Two features at v1:

1. **Commit-hash links**: the trailing `(75121419, 14aacf89)` group on a `CHANGELOG.md` entry renders link-colored and
   opens the commit on GitHub.
2. **i18n key preview**: `tString('crashReporter.dialog.privacyNote')` folds to the English text it resolves to.

Both are reading aids. Neither writes to the repo, and the plugin is never a build dependency: uninstall it and nothing
about Cmdr changes.

## Why our own rather than a marketplace plugin

[Easy I18n](https://plugins.jetbrains.com/plugin/16316-easy-i18n) does key folding and covers Svelte, so it's the
obvious thing to try first, but three properties of our catalog fight it:

- One locale is a **directory** of per-area files (`messages/en/crashReporter.json`, …) while the keys inside are
  already fully qualified (`crashReporter.dialog.title` lives inside `crashReporter.json`). Every i18next-shaped parser
  reads the file name as a namespace and looks for `crashReporter:crashReporter.dialog.title`.
- The catalogs carry ARB-style `@key` metadata siblings, which a generic parser reads as real keys.
- Our keys also reach the UI as bare literals on `labelKey` / `descriptionKey` properties, not only inside a call.

And the changelog feature has no marketplace equivalent at all, because the hash convention is ours. Once we're writing
one plugin, the second feature is cheap.

## Settled decisions

Decided. Don't relitigate while implementing.

1. **It lives at `tools/intellij-plugin/`.** `scripts/` is for things the repo runs; this runs inside an editor.
   `apps/*` is a pnpm workspace glob, so a Gradle project there would confuse `pnpm install`. `crates/` is Rust. The
   `tools/` precedent is `tools/privatesize-poc/`.
2. **In-repo, not a separate repo.** The plugin hardcodes catalog paths and function names, so it has to move when they
   move. That coupling is the argument.
3. **Never published to the Marketplace.** It's sideloaded from `build/distributions/*.zip`. This keeps the BSL license
   out of the discussion; if we ever want to publish, that's an extraction, not a config flag.
4. **The JDK is pinned in a scoped `tools/intellij-plugin/.mise.toml`, not the root one.** The root file pins
   node/pnpm/go for everyone; nobody should pull a JDK for a tool only David runs.
5. **Not wired into `pnpm check`, and nothing guards the coupling.** A Gradle build drags a JVM into CI for zero product
   value. Renaming `tString` or moving `messages/en/` will silently stop the folding, and that's accepted: this is
   private dev tooling, the loss is a reading aid rather than a shipped behavior, and noticing takes one glance at a
   file.
6. **Every feature is config-driven and inert outside Cmdr.** Detection is the presence of
   `tools/intellij-plugin/cmdr-plugin.json` under the project base dir. No project-name matching, no absolute paths, and
   a worktree checkout is recognized for free.
7. **No dependency on any language plugin.** Folding and links are built on core-platform extension points that take a
   language ID as a string and hand back plain text ranges, so the plugin never references JavaScript, TypeScript,
   Svelte, or Markdown PSI and runs in any JetBrains IDE. Reasoning and the costs this accepts: § No language
   dependencies.

## Architecture

```
tools/intellij-plugin/
  README.md                        # human-facing: how to build and sideload
  CLAUDE.md  DETAILS.md            # the agent pair the repo requires
  cmdr-plugin.json                 # the project marker AND the feature config
  .mise.toml                       # java, scoped to this dir
  build.gradle.kts  settings.gradle.kts  gradle/
  src/main/resources/META-INF/plugin.xml
  src/main/kotlin/com/getcmdr/idea/
    core/                          # CmdrProjectService, config loading, settings UI
    features/changelog/            # feature 1
    features/i18n/                 # feature 2
  src/test/kotlin/…
```

`core/CmdrProjectService` is a project-level service that answers one question, "is this project a Cmdr checkout, and
what does its config say", by locating and parsing `cmdr-plugin.json`. Every feature's extension point returns
immediately when the service says no. That's the whole extensibility story: a feature is a package under `features/`, a
section in the config, a toggle in the settings panel, and one or more registrations in `plugin.xml`.

The settings panel (Settings > Tools > Cmdr) carries a per-feature on/off toggle plus the handful of display options
below. Config-file values are the defaults; the panel overrides them per IDE, so David can turn folding off for an
afternoon without a repo edit.

## Feature 1: commit-hash links

Ship this first. It's lower risk than feature 2 and delivers on its own.

**Behavior.** In a file matching `changelog.files` (just `CHANGELOG.md` today), each bare hash inside a trailing `(…)`
group gets hyperlink coloring and resolves to `https://github.com/vdavid/cmdr/commit/<hash>`.

**Honest constraint on the gesture.** IntelliJ editors have no plain-left-click navigation; the gesture is ⌘-click (or
Cmd+B on the caret), with the underline appearing on ⌘-hover. That's the same gesture as every other reference in the
IDE, so it's the right one, but it isn't the single click the request asked for and there's no supported way to get
single click in an editor. Opening lands in the default browser per the IDE's web-browser settings, which is where "new
window" is decided; the plugin doesn't get a say.

**Parsing.** Modelled on `apps/website/src/lib/changelog.ts`, tightened:

- Only the group **anchored to the end of a logical entry** counts. This is what keeps prose safe: entries routinely
  close on `(~40x speed-up!)` or `(smb2 0.8.0)`, and a hex-looking word mid-sentence is never considered.
- A logical entry is a bullet opened by `- ` / `* ` / `+ ` at column zero plus its indented continuation lines, joined
  before matching, so a group that wrapped across two source lines still matches.
- Hashes are exactly `[0-9a-f]{7}`, comma-separated. This is only legal because M1 normalizes the file first; the
  changelog carries three different lengths today.

Drift against the website or `scripts/check/checks/changelog-commit-links.go` is fine and needs no guard: this is
private dev tooling, and the failure mode is a link not showing up, which is visible the moment you look at the file.

**Implementation.** A `PsiReferenceContributor` matching generic `PsiElement` text (no Markdown PSI classes, see § No
language dependencies), contributing a `WebReference` per hash range (that's what gives ⌘-click plus the standard
tooltip), and an `Annotator` applying the hyperlink text attribute so the color shows without hovering. Verify the exact
attribute key at implementation time; the platform has moved it between `CodeInsightColors` and `EditorColors` across
versions.

**Tests.** `BasePlatformTestCase` over a markdown fixture, headless. Cases: single hash; multi-hash group; a group
wrapped across two source lines; a trailing `(~40x speed-up!)` that must NOT match; a hex-looking word mid-sentence that
must NOT match; a nested/indented bullet.

## Normalizing the changelog to 7 characters

The exactly-7 rule needs the file to actually be uniformly 7. It isn't: of 1,383 unique refs, 909 are 8 characters, 425
are 7, and 93 are 6 (counted 2026-08-03). Git's auto-abbreviation grows with the object count, which is why old entries
are short and new ones long.

**It's safe.** All 1,383 resolve, and every one abbreviates uniquely at 7 today (verified 2026-08-03 by resolving each
to its full SHA and re-abbreviating with `git rev-parse --short=7`, which lengthens on ambiguity; none lengthened).

**The rewrite.** A one-shot script: for each trailing group, resolve each hash to its full SHA, re-abbreviate at 7,
refuse to continue if any comes back longer than 7, write in place. Then run the formatter, because 909 refs get one
character shorter and 93 get one longer, so entry wrapping shifts.

**Keeping it 7 going forward.** `.claude/commands/release.md` currently instructs `git log --format='%h' --abbrev=8` and
states the convention is 8 characters. Both change to 7 in the same commit, or the next release quietly reintroduces
8-character refs and those links stop rendering.

**What tightens and what deliberately does not.** Only two places should learn the exactly-7 rule: the plugin, and
`scripts/check/checks/changelog-commit-links.go` (plus its `DETAILS.md` rationale, which explains the 6-character floor
that no longer applies). Tightening the check is what makes the convention enforced rather than aspirational.

The three renderers stay permissive at `{6,40}`:

- **`apps/desktop/src-tauri/src/whats_new/`, the shipped one, must not be touched.** It strips hash groups out of
  user-facing release notes. A shipped app version renders whatever changelog it's given, including older ones, and a
  stricter matcher there means a group it fails to recognize gets shown to users as raw hex. Its tests explicitly cover
  six- and eight-character groups; that coverage is the point.
- `apps/website/src/lib/changelog.ts` and its e2e spec: tightening buys nothing, since it renders the current file.

**A future 7-character ref could become ambiguous.** Git judges uniqueness across all objects, and the repo has ~99,000
of them in-pack at 4,559 commits, so a collision is a matter of time. It fails loudly (the `changelog-links` check
resolves every ref) and the fix is to let that one ref be 8 characters, which costs exactly one unrendered link in the
plugin. Not worth designing around.

## Feature 2: i18n key preview

**Behavior.** A resolvable key folds to its English text, collapsed by default:

```
{tString('crashReporter.dialog.privacyNote')}
→  {“It includes the app version, macOS version, and which part of the code crashed. No file names…”}
```

**What counts as a key site** (all configured, not hardcoded in Kotlin):

- A call to `t` / `tString` / `getMessage` whose first argument is a string literal.
- A `<Trans key="…">` attribute in a `.svelte` file.
- A string literal assigned to a property named in `i18n.keyProperties`: `labelKey`, `descriptionKey`, `titleKey`,
  `cardKey`. The settings registry (`src/lib/settings/types.ts` and the `definitions/` files) carries a lot of user copy
  this way, and folding only call expressions would leave it opaque.

**Known miss, accepted.** Keys built by template (``getMessage(`errors.provider.${p}.appName` as MessageKey)``) can't
fold. There are a handful, mostly in `error-messages/`. Not worth a resolver.

**Rendering the value.** The whole call folds, not just the argument. Unescape ICU's doubled apostrophes (`Here''s` →
`Here's`), leave `{countText}`-style placeholders and `<tag>` markers as-is (seeing them is the point), collapse
newlines to spaces, and **never truncate**: the full sentence is what makes the fold worth having, and a long line is
what horizontal scrolling is for. Curly quotes wrap the fold so it reads as a fold and not as a string literal in the
source.

**Getting back to the real code** costs nothing to build: a fold region is a fold region, so the stock IntelliJ actions
already do it. ⌘+ expands the one under the caret, ⌘⇧+ expands every fold in the file, ⌘− and ⌘⇧− collapse again, and
the platform remembers per-file which regions you left open. The only choice we make is `isCollapsedByDefault = true`,
so a freshly opened file shows text rather than keys.

**The catalog index.** Parse `apps/desktop/src/lib/intl/messages/en/*.json`, dropping `@`-prefixed keys the way
`stripMetadata` in `messages.svelte.ts` does. English only: this is a code-reading aid, and the other locale dirs plus
the non-locale `screenshots/` dir are noise here. Cache per project, invalidated by a VFS listener on that directory,
because a `pnpm dev` session rewrites catalogs while the IDE is open.

**Bonus, same index, cheap**: ⌘-click on the key literal jumps to its line in the JSON. Arguably worth more day to day
than the folding, since it also reaches the translator `@key` description sitting right below the value.

**Out of scope.** An unknown-key inspection (the generated `MessageKey` union plus `svelte-check` already make a typo a
compile error), any write path into the catalog, and non-English locales.

**Tests.** `BasePlatformTestCase` folding fixtures over `.ts`, `.svelte`, and `.md` files, headless. Under the
no-dependency design below they're all the same kind of test, because none of them need a language plugin present.

## No language dependencies

**We do not depend on the JavaScript, TypeScript, or Svelte plugins, and we don't parse those languages either.**

The trick is that `FoldingBuilder` never forces you to touch the target language's PSI. You register
`<lang.foldingBuilder language="JavaScript">` (and `Svelte`, and `Markdown`) with the language ID as a **string** in
`plugin.xml`; `buildFoldRegions` hands you a generic `PsiElement` root and the `Document`, and a fold region is just a
`TextRange` plus a placeholder. So the implementation is a regex over the document text, and the platform still owns
everything that's annoying to own: when to rebuild, what survives an edit, expansion state, ⌘+ / ⌘⇧+, persistence. Same
for the changelog links: `PsiReferenceContributor` patterns can match on generic `PsiElement` text, and `WebReference`
is core platform.

What this buys:

- **No heavy artifact.** The build compiles against the platform only, which for tier 2 is David's local IDEA install,
  so there is no multi-gigabyte IDE download in the loop at all.
- **The Svelte-PSI unknown disappears.** It was the one thing in this spec that could have cost a day.
- **It runs in every JetBrains IDE**, Community and RustRover included, which also makes the headless test tier cheap
  and complete.

What it costs, and why that's fine here:

- **No syntax awareness.** A `tString('x')` sitting inside a comment or a string literal folds too. In a codebase where
  those keys only ever appear as real calls, this is theoretical.
- **A regex, not a parser.** Nested quotes or a multi-line call spanning lines can be missed. Every real call site in
  the repo is single-line and single-quoted, and a missed fold is a fold that just doesn't appear.

**The precise, boring alternative** is the PSI route: depend on the JavaScript plugin, walk real call expressions, and
let the Svelte plugin's injection do the work. It's more correct and it is NOT expensive at runtime (the JS plugin is
already loaded in IDEA Ultimate whether or not we exist, and our own artifact is a few hundred KB either way; the real
cost is a ~2 GB IDE artifact in the build, avoidable by building against the local install). Keep it in the back pocket
for a feature that genuinely needs types or resolution. Nothing in v1 does.

**M0 verifies three things about this design**, because they're cheap to check and expensive to assume: that a
`lang.foldingBuilder` registered for a language whose plugin isn't installed degrades quietly rather than logging an
error on every startup, what the Svelte language ID string actually is, and that our regions coexist with the language's
own folding pass instead of fighting it.

## Milestones

- **M0, spike.** Gradle scaffold that builds and loads, the tier 1 and tier 2 loops proven on a do-nothing feature, and
  the three checks from § No language dependencies. Output is a paragraph in `DETAILS.md`. Nothing else gets built
  first, because everything else assumes those answers.
- **M1, changelog normalization.** The whole of § Normalizing the changelog to 7 characters, landing before the plugin
  reads the file. Independently useful and independently revertable; it touches the repo, not the plugin.
- **M2, core.** `CmdrProjectService`, `cmdr-plugin.json` loading, the settings panel with feature toggles, and the
  `plugin.xml` skeleton. Verified by a feature that does nothing except log that it's enabled in Cmdr and disabled in a
  scratch project.
- **M3, commit-hash links.** The full feature plus its headless tests. First useful build.
- **M4, i18n folding.** `.ts` call sites first, then key properties, then `.svelte` and `<Trans>`. All one mechanism
  under the no-dependency design, so the split is about test coverage, not risk.
- **M5, ⌘-click key to catalog.** Reuses M4's index.
- **M6, wiring.** `README.md`, the `CLAUDE.md` + `DETAILS.md` pair, and the docs links from § Docs wiring.

M0 through M3 is the standalone-valuable slice: a normalized changelog plus working commit links, with the i18n half
still ahead.

## `cmdr-plugin.json`

Doubles as the project marker and the feature config.

```json
{
  "changelog": {
    "files": ["CHANGELOG.md"],
    "commitUrl": "https://github.com/vdavid/cmdr/commit/{hash}",
    "trailingGroupPattern": "\\(([0-9a-f]{7}(?:,\\s*[0-9a-f]{7})*)\\)$"
  },
  "i18n": {
    "catalogGlob": "apps/desktop/src/lib/intl/messages/en/*.json",
    "functions": ["t", "tString", "getMessage"],
    "componentAttributes": [{ "component": "Trans", "attribute": "key" }],
    "keyProperties": ["labelKey", "descriptionKey", "titleKey", "cardKey"],
    "languages": ["JavaScript", "TypeScript", "Svelte"]
  }
}
```

## The feedback loop

An agent has to be able to see this working without David driving an IDE. Three tiers, in the order they get reached
for.

**Tier 1, headless fixture tests. The primary loop.** `./gradlew test` runs `BasePlatformTestCase` fixtures against a
real, in-process IntelliJ Platform: no window, no display, no license, seconds per run. Folding regions, references, and
annotations are all directly assertable (`myFixture.assertPreviewText`, the folding-region model, `WebReference`
targets), so both features are testable at the level that matters. This is where the loop lives, and it's the reason the
M0 spike de-risks so much: once the PSI shape is known, everything after it is red-green.

**Tier 2, a real IDE the agent launches and screenshots.** `./gradlew runIde` starts a sandboxed IDE (its own config and
plugin dirs, so it can't touch the running one) with a seeded project directory, opened on a fixture file. The agent
runs it in the background, waits for the window, takes a `screencapture -l <window-id>` PNG, and reads it. Slow, roughly
a minute per cycle, and it needs the desktop session, so it's the confirm step and never the iteration step: "the fold
really renders and is really colored", once per feature, not per edit.

**Tier 3, scripted UI runs.** The Starter framework (`TestFrameworkType.Starter` behind the `testIdeUi` task, JUnit 5)
drives a real IDE from test code and collects output including screenshots. It exists and it works, but it is heavier
and flakier than tier 2's one-shot screenshot. Only worth it if we end up with a UI surface complicated enough to
regress, which two reading aids are not.

**Which IDE the loop runs against.** The no-dependency design makes this a cheap question: tier 1 needs only the
platform, so Community is a perfectly good and much smaller test target for both features, `.svelte` fixtures included
(they're plain text to us). Tier 2 points at the local **IntelliJ IDEA 2026.2 EAP** install, because that's the IDE
David actually reads code in, so the screenshot shows what he'd see, with the real Svelte plugin installed and syntax
highlighting on. Building against a local installation also means no IDE artifact download and no licensing question.

## Docs wiring

- `tools/intellij-plugin/README.md`: human-facing build-and-sideload instructions.
- `tools/intellij-plugin/CLAUDE.md` + `DETAILS.md`: required as a pair by `claude-md-details-sibling`. `CLAUDE.md` gets
  the module map and the two guardrails (scoped `.mise.toml`, config-driven or it silently dies); `DETAILS.md` gets the
  M0 PSI findings and the decisions here.
- `AGENTS.md` § File structure has no `tools/` line today, and `tools/privatesize-poc/README.md` looks unreferenced by
  anything. M5 adds the `tools/` line and links both, which is also what `docs-reachable` needs. Do not reach for the
  intentionally-unreachable allowlist; connect them.
- `docs/architecture.md` needs no entry: it maps the product, and this ships to nobody.

## Verified facts

- IntelliJ Platform Gradle Plugin latest is **2.18.1**, published 2026-07-10 (Gradle Plugin Portal, checked 2026-08-03).
  Re-check at M0 rather than trusting this line.
- The installed IDE is **IntelliJ IDEA 2026.2 EAP** (`/Applications`, 2026-08-03). M0 pins `sinceBuild` off the real
  build number; an EAP target means a `sinceBuild` bump is likely at the 2026.3 upgrade.
- The Svelte plugin is `dev.blachut.svelte.lang` ([marketplace](https://plugins.jetbrains.com/plugin/12375-svelte)).
- Commit-ref lengths in `CHANGELOG.md`: of 1,383 unique refs, 909 are 8 characters, 425 are 7, 93 are 6 (counted
  2026-08-03 by matching trailing groups per source line, so wrapped groups are undercounted; the ratio is what
  matters). All 1,383 resolve and all abbreviate uniquely at 7. The repo is 4,559 commits and ~99,000 in-pack objects.
- The Starter UI testing framework is real and current (`TestFrameworkType.Starter`, driven by the `testIdeUi` task,
  JUnit 5 only), per the IntelliJ Platform Gradle Plugin docs, checked 2026-08-03.

## Answered (2026-08-03)

1. **Fold the whole call**, with stock ⌘+ / ⌘⇧+ as the escape hatch back to the real code.
2. **No truncation.** The full message text, however long.
3. **`CHANGELOG.md` only.** Specs and `docs/notes/` cite commits too; they don't get links.
4. **Nothing else in v1.** Two features, then stop.
