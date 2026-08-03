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
5. **Not wired into `pnpm check` as a build.** A Gradle build drags a JVM into CI for zero product value. One cheap Go
   check guards the coupling instead (see § The guard check).
6. **Every feature is config-driven and inert outside Cmdr.** Detection is the presence of
   `tools/intellij-plugin/cmdr-plugin.json` under the project base dir. No project-name matching, no absolute paths, and
   a worktree checkout is recognized for free.
7. **IDEA Ultimate only.** Feature 2 depends on the bundled JavaScript plugin, which Community doesn't have. RustRover
   is out for the same reason; if the backlog ever grows a Rust-side feature, that's a separate plugin artifact from the
   same source tree.

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

**Parsing.** The rule must mirror `apps/website/src/lib/changelog.ts` exactly, because a hash the website links and the
plugin doesn't (or the reverse) is a bug report waiting to happen:

- Only the group **anchored to the end of a logical entry** counts. This is what keeps prose safe: entries routinely
  close on `(~40x speed-up!)` or `(smb2 0.8.0)`, and a hex-looking word mid-sentence is never considered.
- A logical entry is a bullet opened by `- ` / `* ` / `+ ` at column zero plus its indented continuation lines, joined
  before matching, so a group that wrapped across two source lines still matches.
- Hashes are `[0-9a-f]{6,40}`, comma-separated.

Three copies of that rule now exist (`scripts/check/checks/changelog-commit-links.go`, the website, the plugin). The
guard check below asserts the plugin's copy agrees with the website's rather than letting them drift silently.

**Implementation.** A `PsiReferenceContributor` over Markdown PSI text elements, contributing a `WebReference` per hash
range (that's what gives ⌘-click plus the standard tooltip), and an `Annotator` applying the hyperlink text attribute so
the color shows without hovering. Verify the exact attribute key at implementation time; the platform has moved it
between `CodeInsightColors` and `EditorColors` across versions.

**Tests.** `BasePlatformTestCase` over a markdown fixture, headless, no JS plugin needed. Cases: single hash; multi-hash
group; a group wrapped across two source lines; a trailing `(~40x speed-up!)` that must NOT match; a hex-looking word
mid-sentence that must NOT match; a nested/indented bullet.

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

**Rendering the value.** Unescape ICU's doubled apostrophes (`Here''s` → `Here's`), leave `{countText}`-style
placeholders and `<tag>` markers as-is (seeing them is the point), collapse newlines to spaces, truncate past 80
characters with `…`. Curly quotes wrap the fold so it reads as a fold and not as a string literal in the source.

**The catalog index.** Parse `apps/desktop/src/lib/intl/messages/en/*.json`, dropping `@`-prefixed keys the way
`stripMetadata` in `messages.svelte.ts` does. English only: this is a code-reading aid, and the other locale dirs plus
the non-locale `screenshots/` dir are noise here. Cache per project, invalidated by a VFS listener on that directory,
because a `pnpm dev` session rewrites catalogs while the IDE is open.

**Bonus, same index, cheap**: ⌘-click on the key literal jumps to its line in the JSON. Arguably worth more day to day
than the folding, since it also reaches the translator `@key` description sitting right below the value.

**Out of scope.** An unknown-key inspection (the generated `MessageKey` union plus `svelte-check` already make a typo a
compile error), any write path into the catalog, and non-English locales.

**The one real risk.** All of this assumes `{tString(…)}` inside a `.svelte` template exposes ordinary JavaScript PSI to
a provider registered for `language="JavaScript"`. It probably does, since the Svelte plugin (`dev.blachut.svelte.lang`)
injects JS, but if the expressions land in a Svelte-specific injected language we're reading a third-party plugin's PSI
to find out. M0 settles it before anything is built on top.

**Fallback if the PSI shape is hostile**: a `FoldingBuilder` registered on the Svelte file type that walks leaf elements
and regex-matches their text. Folding regions only need `TextRange`s, so this always works; it's uglier and slightly
more false-positive-prone, and it's why the risk can't sink the feature.

**Tests.** `BasePlatformTestCase` folding fixtures over `.ts` files run headless and cover the call-expression and
key-property paths. The `.svelte` path needs `runIde` and human eyes; that's a known gap, called out rather than papered
over.

## Milestones

- **M0, spike (kill switch).** Gradle scaffold that builds and loads in the IDE, plus a PSI probe on a real `.svelte`
  file at a `tString` site: which language, which element type, does a JavaScript-registered extension see it. Output is
  a paragraph in `DETAILS.md` and a go/no-go on the primary approach for M3. Nothing else gets built first, because
  everything else assumes the answer.
- **M1, core.** `CmdrProjectService`, `cmdr-plugin.json` loading, the settings panel with feature toggles, and the
  `plugin.xml` skeleton. Verified by a feature that does nothing except log that it's enabled in Cmdr and disabled in a
  scratch project.
- **M2, commit-hash links.** The full feature plus its headless tests. First useful build.
- **M3, i18n folding.** `.ts` call expressions first (headless-testable), then key properties, then `.svelte` and
  `<Trans>` per M0's answer.
- **M4, ⌘-click key to catalog.** Reuses M3's index.
- **M5, wiring.** `README.md`, the `CLAUDE.md` + `DETAILS.md` pair, the guard check, and the docs links from § Docs
  wiring.

M0 through M2 is the standalone-valuable slice. M3 onward can wait if the spike says the Svelte path is a slog.

## `cmdr-plugin.json`

Doubles as the project marker and the feature config. Read by the plugin at runtime and by the guard check in CI, which
is what keeps it honest.

```json
{
  "changelog": {
    "files": ["CHANGELOG.md"],
    "commitUrl": "https://github.com/vdavid/cmdr/commit/{hash}",
    "trailingGroupPattern": "\\(([0-9a-f]{6,40}(?:,\\s*[0-9a-f]{6,40})*)\\)$"
  },
  "i18n": {
    "catalogGlob": "apps/desktop/src/lib/intl/messages/en/*.json",
    "functions": ["t", "tString", "getMessage"],
    "componentAttributes": [{ "component": "Trans", "attribute": "key" }],
    "keyProperties": ["labelKey", "descriptionKey", "titleKey", "cardKey"],
    "maxFoldLength": 80
  }
}
```

## The guard check

A new `intellij-plugin-config` entry in `scripts/check/checks/registry.go` (`App: AppOther`, `IsFast: true`, inputs
scoped to the config file, the catalog dir, and `apps/website/src/lib/changelog.ts`). No JVM: it only reads JSON and
source text. It asserts:

- `i18n.catalogGlob` matches at least one file.
- Every name in `i18n.functions` is still exported from `apps/desktop/src/lib/intl/messages.svelte.ts`.
- Every name in `i18n.keyProperties` still appears as a `MessageKey`-typed field in `src/lib/settings/types.ts`.
- `changelog.trailingGroupPattern` is byte-identical to the pattern the website uses.

Without this, renaming `tString` or moving `messages/en/` leaves the plugin silently showing nothing, which is the worst
failure mode available: no build breaks, no test fails, the feature just quietly stops.

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

## Open questions for David

1. **Fold the whole call or just the argument?** `{“It includes the app version…”}` reads best but hides which function
   was used; `tString(“It includes…”)` keeps `t` vs `tString` vs `getMessage` visible, which matters when you're
   scanning for the ICU-vs-raw distinction the intl docs care about. Spec assumes whole call.
2. **80 characters before truncating**, or longer? Several catalog strings are two full sentences.
3. **Hashes outside `CHANGELOG.md`?** Spec files and `docs/notes/` cite commits too. Easy to widen the glob, but every
   added file is more surface for a false positive.
4. **Anything else you'd want in v1?** The plugin exists to be enriched; if there's a third annoyance, M2 is the cheap
   moment to add it.
