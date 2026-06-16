# Custom i18n runtime + message catalog (i18n-readiness, step 2)

Give Cmdr a thin, owned i18n runtime: every user-facing string resolves from a JSON message catalog through one typed
`t()` (and a `<Trans>` for sentences with inline components), formatted with a real ICU engine for the
multiple-variable cases, scoped by semantic keys, with compile-time key safety. English-only ships today; this makes the
app translation-ready without adopting an i18n framework or a TMS.

This is step 2 of the "i18n-ready" effort. Step 1 (error prose → frontend) and step 3 (locale-aware formatter layer,
`$lib/intl`) shipped. This step builds directly on `$lib/intl`: it reads the locale via the existing `getLocale()`
chokepoint and embeds numbers/sizes/dates through the existing formatters.

## Why custom, and why this shape (the intentions, so you can adapt)

The decision history with David, captured so the implementer understands the "why" and can deviate intelligently:

- **Custom thin runtime over a framework (Paraglide / i18next / svelte-i18n / Lingui).** The hard part of i18n is the
  message FORMAT (per-locale plurals, multi-variable selection), not the lookup. We get the hard part from a focused
  library (`intl-messageformat`) and own the trivial ~80-line runtime around it. i18next is a framework whose value-add
  (namespaces, backends, language detection, a React `<Trans>`) is ~80% stuff we don't need (we bundle JSON, scope by
  key prefix) or can't use (React-only `<Trans>`). Paraglide has no English-as-key, no namespaces, no
  description/screenshot metadata in its format, and no reactive rune. svelte-i18n is dormant. Lingui has no maintained
  Svelte integration. Owning a thin runtime fits Cmdr's principles (smart backend / thin frontend, elegance, here for
  the long run, agents read all the code) and composes cleanly with the `$lib/intl` seam already shipped.
- **`intl-messageformat` (ICU MessageFormat) as the format engine.** A file manager genuinely has multi-variable
  sentences (`transfer-complete-toast.ts`: "Copied 3 files and 1 folder", "Moved 5, skipped 2"). A flat
  `{ one, other }` JSON can't express two independent plurals in one sentence with locale-dependent word order; ICU can.
  Do NOT hand-roll ICU/CLDR plural selection: that's the one piece worth a dependency.
- **`<Trans>` for components mid-sentence.** A sentence with an inline interactive element (the FDA hint's
  `<LinkButton>Open System Settings</LinkButton>`, a styled chip) needs its component re-inserted at the locale's word
  order. Every battle-tested `<Trans>` is React; in Svelte we build a ~50-line snippet-based one regardless of library
  choice, so this is owned work either way. It renders text + snippets (no `{@html}`), so it's XSS-safe by construction.
- **JSON catalogs, per feature area, with ARB-style `@key` metadata; no TMS.** JSON is native to JS, agent-friendly,
  diffable, version-controlled. A TMS earns its keep only with non-developer human translators / translation memory /
  review UI; we have none of those (agents translate, the pipeline is scripted, all in git). Descriptions and screenshot
  references live in `@key` metadata; screenshots are image files in the repo referenced by filename (one shared across
  many keys = many keys naming the same file).
- **Semantic prefix-scoped keys (`area.feature.leaf`) + a generated `MessageKey` union + a naming check.** Semantic
  keys survive copy edits (English-as-key orphans translations when copy changes) and make scoping free: the key path IS
  the scope, so the same English word can diverge per window just by having its own key. The generated union gives
  compile-time typo-proofing, autocomplete, find-usages, and dead-key / missing-key detection. A naming check enforces
  structure "even when you forget", which was David's explicit worry.

## Goal (end state)

- A `$lib/intl` runtime exposing `t(key, params?)` and `<Trans>` that resolve user-facing text from JSON catalogs under
  `messages/<locale>/`, formatted via `intl-messageformat`, reading the locale from the existing `getLocale()`.
- Keys are semantic, prefix-scoped (`settings.fsWatch.title`), with a generated `MessageKey` union type so wrong/missing
  keys are compile errors and unused catalog keys are flagged.
- Per-string descriptions and screenshot references live in `@key` ARB-style metadata, stripped before the runtime ever
  sees them.
- Reactivity: a locale change re-renders `t()`/`<Trans>` usages in markup via a Svelte rune. (No in-app locale picker
  ships now; the seam supports one later.)
- Base-locale (`en`) rendered output is unchanged for current users (this is readiness, not a copy change).
- A lint (`cmdr/no-raw-user-facing-string`, or an extension of the existing `cmdr/no-raw-locale-format`) prevents new
  hardcoded user-facing strings in migrated areas.
- The hardest existing case (`transfer-complete-toast.ts`) is migrated as the pilot, proving the whole loop.

## Non-goals (hold the line)

- **No real translations.** No `de.json` etc. The app stays English-only. We ship the machinery + the base catalog. The
  agent-driven translation pipeline is a later, separate effort. **Principle 6 ("humans review anything meeting human
  eyes") note:** the base `en` catalog is a parity-protected MOVE of already-human-authored copy, so it's fine. But the
  future agent-translated locales DO meet human eyes, so that later pipeline must include human review per principle 6 —
  "agents translate, scripted pipeline" is not a license to ship unreviewed machine copy. Flag this so the assumption
  doesn't bake in a principle breach.
- **No TMS, no translation SaaS.** Catalogs + screenshots live in git.
- **No locale-switch UI.** The locale comes from the OS via `getLocale()`. Build the reactive seam, not a picker.
- **Don't re-do step 3.** Numbers/sizes/dates keep formatting through `$lib/intl` + `format-utils`; `t()` embeds the
  already-formatted strings as params. Don't reformat inside messages with ICU `number`/`date` skeletons (keep
  formatting in one place, `$lib/intl`).
- **Don't boil the ocean in one commit.** The full ~1,000-string migration is tranched by area and lands incrementally;
  each tranche is independently shippable and flips the lint on for that area.
- **Don't touch the backend.** Rust stays word-free (step 1 already moved error prose to the FE).

## Background: verified current state

- **`$lib/intl` exists** (`locale.ts` = `getLocale()` + `_setLocaleForTests`; `number-format.ts` = memoized
  `Intl.NumberFormat`, `formatInteger`, `getGroupSeparator`). Locale is read live, uncached; formatters are memoized.
  The runtime built here lives alongside these.
- **Error prose is already centralized** in the frontend (`$lib/errors` factories from step 1), rendered as markdown via
  `snarkdown` + `{@html}` with a TS escaper for interpolated params. These are strings produced by composition logic
  (reason → message, provider table, `system_strings` pane names). They are a migration target (their literal English →
  catalog keys) but their composition logic and snarkdown rendering pipeline stay. See the errors-tranche note.
- **Settings strings are centralized** in `settings-registry.ts` (label + description per setting). They migrate as a
  block (registry holds keys, resolved through `t()`).
- **The hard multi-variable case is `transfer-complete-toast.ts`**: builds "Copied N files and M folders", "Moved N,
  skipped M (already at the target)", with verb conjugation (`Copied`/`Moved`/`copied`/`moved`), was/were agreement, and
  `parts.join(' and ')` fragment concatenation. This is exactly what ICU `select` + `plural` replaces, and the pilot.
- **Rough string count: ~800–1,200** (command palette ~100–150; settings ~160–240; errors ~300; menus ~30–40; the
  long tail across ~202 components ~300–500). The extraction dry-run (M1) produces the real number; two big buckets
  (settings, errors) are already consolidated.
- **A `cmdr/no-raw-locale-format` ESLint rule already exists** (from step 3); the new no-hardcoded-string lint extends
  that pattern (`apps/desktop/eslint-plugins/`).

## Design decisions

### Decision 1: the runtime (`messages.svelte.ts`)

`t(key, params?)`: resolve `catalog[activeLocale][key]`, fall back to `catalog.en[key]`, fall back to the key string
(so a missing key is visible, never a crash). Compile the resolved ICU string with `intl-messageformat` (cached by
`(locale, key)`), `.format(params)`. Always route through `intl-messageformat` even for plain `{name}` interpolation, so
there's ONE code path (it handles trivial interpolation too; compiled instances are cached, so the parse cost is paid
once per key per locale). Plurals/selects are handled by the engine via `Intl.PluralRules` internally; we never
hand-roll selection.

The runtime ALSO exposes a raw accessor `getMessage(key): string` (same lookup + fallback chain) that returns the
catalog value WITHOUT ICU parsing, for callers that do their own composition and must not hit ICU's brace/apostrophe
grammar — specifically the error pipeline (`compose.ts` + `expandSystemStrings` + snarkdown). Most code uses `t()`; the
error pipeline uses `getMessage()`. Reactivity note: `getMessage()` reads the version rune like `t()`, but the error
pipeline calls it inside plain-`.ts` compose functions (a non-reactive context), so error copy is effectively a
SNAPSHOT taken at compose time, not live-reactive to a locale change. That's intentional and correct (errors are
transient, re-composed on the next failure) — same transient-snapshot semantics as the caveat above; don't expect error
copy to re-render in place on a locale switch.

Reactivity (precise, this is load-bearing): a module-level locale-version `$state` lives here (hence `.svelte.ts`). It is
a reactivity SIGNAL, not a second locale source: `getLocale()` (in `locale.ts`) stays the single source of truth for the
locale value. `t()` MUST read the version `$state` UNCONDITIONALLY at the top of every call, BEFORE any compiled-message
cache lookup, then call `getLocale()` for the value. If the cache is consulted before the rune is read, the reactive
dependency isn't tracked and `{t('key')}` won't re-run on a locale change. The proven pattern in this repo is
`system-strings.svelte.ts` (markup reads a `$state` property during render); follow it. Note `state_referenced_locally`
is in the suppressed-warnings list, so the compiler will NOT warn you if you read the rune wrong; the
`messages.svelte.test.ts` reactivity test is the only guard.

`setLocale(locale)` is the seam (no in-app picker ships now; tests drive it): it writes the locale into `locale.ts`'s
override (the same single source `_setLocaleForTests` uses, so the `$lib/intl` formatters pick up the change too) AND
bumps the version `$state`. There are NOT two competing locale sources: the rune only forces re-render; the value always
comes from `getLocale()`.

Caveat: `t()` is reactive only inside a reactive context (markup / `$derived`); a `t()` called once in a plain `.ts`
computation is a snapshot, which is the right semantics for transient strings (mirrors how `transfer-error-messages.ts`
snapshots a shortcut binding). No SSR/prerender concern: the app is a pure SPA (`+layout.ts` has `ssr = false`,
`adapter-static` with an `index.html` fallback), so route components are never server- or build-rendered. The only
runtime requirement is that `getLocale()` touches no `window` (already true).

### Decision 2: `intl-messageformat` as the ICU engine

Use `intl-messageformat` (FormatJS, ICU MessageFormat 1) as the format engine. Rationale: ICU MF1 is the broadly
understood standard (every TMS and most LLMs know it, which matters for the future agent-translation pipeline), it's
mature and stable, and its `formatToParts` supports rich-text tags (the mechanism `<Trans>` needs, Decision 3).
`@messageformat/core` (MessageFormat 2) is the newer standard but less tooling/LLM familiarity today; note it as a
future option, don't adopt it now. **This is the one external dependency; confirm the version is ≥14 days old per the
`use-latest-dep-versions` rule** (at time of writing `11.2.8` was ~12 days old and would be GATED; the prior `11.2.7`
was safe — verify current dates at adoption, don't trust these numbers). License is BSD-3-Clause (compatible); confirm.
Memoize compiled `IntlMessageFormat` instances keyed on `(locale, key)`.

**Two ICU hazards the migration MUST handle (they threaten the byte-identical invariant):**

- **Apostrophes and braces are ICU syntax.** ICU treats `'` as an escape char and `{`/`}` as placeholders. Cmdr copy is
  full of apostrophes ("doesn't", "can't", "you're", "already at the target"). A naive lift of existing English into ICU
  messages is NOT byte-identical: a lone `'` swallows following text. Messages must double apostrophes (`''`) or the
  extraction/migration must do it automatically. Treat this as a per-tranche parity hazard, not just transfer.
- **Don't route NON-ICU strings through `format()`.** Plain strings that contain literal `{...}` for other purposes
  (notably the error pipeline's `{system_settings}` `expandSystemStrings` tokens, and the `esc()` HTML entities) collide
  with ICU's brace/apostrophe grammar. Error strings stay on their own pipeline (see Decision 4 + the errors tranche),
  not the ICU `format()` path. Only genuinely multi-variable plural/select sentences need ICU; simple `{name}`
  interpolation does go through the engine (one code path) but carries no extra collision risk.

### Decision 3: `<Trans>` for components in a sentence

A Svelte 5 component taking a `key` plus a record of snippets, e.g.:

```svelte
<Trans key="settings.fsWatch.downloadsFdaHint" snippets={{ settingsLink }} />
```

with the catalog message `"Cmdr needs Full Disk Access. <settingsLink>Open System Settings</settingsLink>"`. Implement
via `intl-messageformat`'s rich-text support in **`format(values)`**: each `<tagName>` is supplied as a HANDLER FUNCTION
in `values` (`format({ settingsLink: (chunks) => marker })`), and the handler returns a marker object; the engine
assembles the message into an ARRAY of text strings and markers, which `<Trans>` renders as text nodes + the matching
Svelte snippet. (Correction from an earlier draft: the core `IntlMessageFormat` has **no `formatToParts`** — that lives
in the react-intl/`@formatjs/intl` layer. The mechanism is `format()` with tag-handler functions returning markers.) No
`{@html}` ⇒ XSS-safe by construction (text is text, components are real components). The spike (M0) validates this exact
`format()`-with-handlers API against Svelte snippets; if it's awkward, the fallback is a tiny custom placeholder
splitter (`{settingsLink}` tokens), ~30 lines.

**Boundary with the error markdown path:** the error system keeps its `snarkdown` + `{@html}` + param-escaper pipeline
(its messages may carry markdown like bold provider names). Error literals migrate INTO the catalog as `errors.*` keys
(plain strings, possibly containing markdown), but they keep being rendered through the existing error pipeline, not
`<Trans>`. `<Trans>` is only for the handful of UI sentences with inline INTERACTIVE components. Don't conflate the two.

### Decision 4: catalog format and layout

JSON, per feature area, under `messages/<locale>/`, plus `common.json` for genuinely shared strings:

```
messages/
  en/
    common.json      # Cancel, Save, Open System Settings, … (truly shared)
    settings.json    # settings.*  (migrated from settings-registry.ts)
    errors.json      # errors.*    (migrated from $lib/errors)
    transfer.json    # transfer.*  (the multi-count toasts — pilot)
    search.json  viewer.json  menu.json  commands.json  onboarding.json  …  (~10–12 area files)
  screenshots/       # PNGs referenced by @key metadata; one file may serve many keys
```

Key prefix ↔ filename map 1:1 (`settings.fsWatch.title` → `settings.json`). Per-area-central (not one big file, not
fully colocated next to components) is the chosen middle: clean diffs, an agent editing one feature touches one file,
and a translate-a-locale job globs ~12 predictable files. (Fully colocated fragments are the more on-brand-for-Cmdr
alternative but multiply per-locale files and complicate the build merge; flagged as an open decision below.)

Message value is either a plain ICU string or, for plurals/selects, an ICU string with `{count, plural, …}` /
`{type, select, …}` inline (NOT a `{one, other}` object — the ICU engine parses the inline form). ARB-style metadata
lives in sibling `@key` entries, stripped at load/build:

```jsonc
{
  "transfer.moved": "{folders, plural, =0 {} other {{folders, number} folders}}",  // illustrative; real ICU in M0
  "@transfer.moved": { "description": "Toast after copy/move. {files}/{folders} are top-level counts.",
                       "screenshot": "transfer-complete-toast.png" }
}
```

### Decision 5: keys, type safety, structure enforcement

Semantic, prefix-scoped, lowerCamel leaf: `area.feature.leaf` (`settings.fsWatch.clearIndex`, `common.openSystemSettings`).
A codegen step (in the check pipeline) reads `messages/en/*.json` and emits `$lib/intl/keys.gen.ts`:

```ts
export type MessageKey = 'settings.fsWatch.title' | 'common.openSystemSettings' | 'transfer.moved' | /* … */;
```

`t(key: MessageKey, …)` and `<Trans key: MessageKey>` ⇒ wrong/missing keys are compile errors with autocomplete and
find-usages. The codegen ALSO reports: keys used in code but absent from the catalog (build failure), and catalog keys
never used in code (dead-string warning). A naming check (Go check or ESLint) enforces the `^[a-z][a-zA-Z0-9]*(\.[a-z][a-zA-Z0-9]*)+$`
shape and that the first segment is a known area, so structure can't drift. Optional later upgrade: generate per-key
PARAM types (so `t('transfer.moved')` without `{files}` errors) — Paraglide-style; defer unless cheap.

Scoping rule: window/area divergence is achieved by distinct keys (the prefix), never a positional "window" argument.
Shared strings live in `common.*`; the moment one site needs a different translation, it gets its own area key.

### Decision 6: base-unchanged invariant + reactivity seam

`en` is the base/source locale. Every migrated string's base-locale rendered output must equal the pre-migration English
(parity net per area). The reactive rune is built now but exercised only by tests (no picker ships). Test-seam rule (don't mix these up):
the REACTIVITY test must drive locale via `setLocale()` (which bumps the version rune AND writes `locale.ts`'s override);
driving it via `locale.ts`'s `_setLocaleForTests` only changes the value, not the rune, so a re-render would never fire
and the test would lie. `_setLocaleForTests` is for non-reactive value-snapshot tests only.

## Milestone 0 — Spike on the transfer toasts (de-risk before building)

Prove the whole loop on the hardest case before committing to the full design. In the worktree, throwaway-or-keep:

- Add `intl-messageformat`; write `transfer.*` ICU messages reproducing EVERY branch of `composeTransferCompleteToast`
  (trash, delete, copy/move × {files+folders, files-only}, skipped suffixes, was/were, all-skipped collapse). This is
  the acid test that ICU `select`+`plural` can express the current wording. Two known restructurings the caller must
  do (don't expect raw counts to suffice): (a) the omit-zero-part "N files and M folders" join can't be expressed from
  `{files}`/`{folders}` alone (ICU branches are independent, they can't see each other's emptiness without a dangling
  " and " / stray space) — pass a discriminator param (`kind: 'both' | 'filesOnly' | 'foldersOnly'`) and `select` on it;
  (b) embed counts by passing `$lib/intl`-preformatted count STRINGS as params (keeps formatting single-sourced),
  NOT ICU's inline `{n, number}`. Double apostrophes (`''`) in every message or parity breaks (see Decision 2).
- Stand up the minimal runtime (`t()` with the read-rune-before-cache reactivity invariant + compiled-MF cache) and the
  `<Trans>` proof against ONE component-in-sentence case (the FDA hint), validating the `format()`-with-tag-handlers →
  Svelte snippet mapping (NOT `formatToParts`, which the core lacks — see Decision 3).
- Generate the `MessageKey` union for just these keys; confirm the typed call sites + a deliberately-wrong key fails the
  typecheck.

**Exit criteria (the spike answers these):** (a) ICU expresses all transfer-toast branches at en-US parity; (b) `<Trans>`
renders an inline `<LinkButton>` correctly via snippets; (c) the generated-key typecheck catches a bad key; (d) the
runtime is reactive in markup. If (a) reveals ICU can't cleanly express a branch, STOP and report — that reshapes
Decision 2. Capture findings in the plan before M1.

Tests: TDD the runtime resolution/fallback (`messages.svelte.test.ts`, red→green) and a parity test mirroring
`transfer-complete-toast.test.ts` asserting the ICU output equals the current composer's output for a branch matrix.

## Milestone 1 — Runtime, codegen, checks, extraction dry-run

Generalize the spike into the real infrastructure.

- Finalize `messages.svelte.ts` (`t`, `getMessage`, `setLocale`, reactivity, fallback chain, compiled-MF cache) +
  `Trans.svelte` + the `@key`-metadata stripper (load path) + the codegen (`keys.gen.ts` + missing/dead-key report) +
  the naming check.
- **Scope the no-raw-string lint to a CLOSED set of sink positions, not "any string literal."** Like the existing
  `no-raw-locale-format` (which keys on `.toLocaleString`/`new Intl.*`), flag literals only in known user-facing sinks:
  specific component props (`title`, `label`, `placeholder`, `aria-label`), `addToast(...)` arguments, and JSX text
  nodes in `.svelte` markup. Do NOT attempt "any user-facing string" detection: log lines, IPC keys, CSS classes,
  `data-*`, command ids, role values, and test strings make that open-ended and false-positive-ridden. Start enforced on
  `transfer`; widen the area allowlist per migrated tranche.
- Add a `keys.gen.ts` freshness check mirroring `desktop-bindings-fresh.go` (regenerate-and-diff; fail if stale), and
  add `keys.gen.ts` to the `file-length` `exempt` list. Generated file, never hand-edited.
- Wire the catalog loader for the bundled SPA (static imports of `messages/en/*.json`, merged into one map; lazy
  per-locale dynamic import is a later concern — only `en` exists now).
- **Extraction dry-run:** a script that scans `.svelte`/`.ts` for user-facing string literals (JSX text nodes, common
  attributes: `title`/`label`/`placeholder`/`aria-label`, `addToast(...)`, etc.) and emits a candidate list grouped by
  area, with a count. This gives the REAL string total and surfaces every multi-variable/rich-text case that needs ICU
  or `<Trans>`. Output is a working doc (not committed catalog), used to plan M2 tranches. Log what the heuristic
  necessarily misses (dynamic strings, concatenations) so coverage isn't overstated.
- Docs: `$lib/intl/CLAUDE.md` (add the runtime/`<Trans>`/key-safety must-knows: the read-rune-before-cache invariant,
  the `''` apostrophe rule, the error pipeline uses `getMessage()` not ICU) + `DETAILS.md` (the full design, intentions,
  the error-pipeline boundary, the ICU-vs-`$lib/intl` formatting split). Give `messages/` its own `CLAUDE.md` +
  `DETAILS.md` pair (NOT just a README): agents will edit catalog files there, so the must-knows (key shape, `''`
  apostrophe escaping, `@key` metadata, screenshots-by-filename, never hand-edit `keys.gen.ts`) should autoload on touch,
  and the `claude-md-details-sibling` check requires the pair anyway.

Tests: TDD the codegen (given a catalog, emits the right union + flags a missing/dead key) and the metadata stripper.
Lint/check tests for the naming rule and no-raw-string rule (mirror the existing `no-raw-locale-format.test.js`).

## Milestone 2 — Migrate, by area, in tranches

Each tranche: move that area's literals into `messages/en/<area>.json` (with `@key` descriptions; screenshots optional
now), replace call sites with `t()`/`<Trans>`, regenerate keys, flip the no-raw-string lint on for that area, prove base
parity. Independently shippable. Suggested order (smallest/most-consolidated first):

1. `transfer` (done in M0, finalize).
2. `settings` (registry already centralized — registry stores keys, resolved via `t()`).
3. `errors` (special, read `src/lib/errors/` first): the step-1 module is `listing-error-messages.ts`,
   `git-error-messages.ts`, `provider-error-messages.ts`, `compose.ts` (`esc` + `expandSystemStrings`),
   `markdown-escape.ts`, guarded by `friendly-error-parity.test.ts` against a FROZEN golden fixture
   (`__fixtures__/friendly_error_golden.json`) plus `friendly-error-style.test.ts`. Migration: the literal English moves
   into `errors.*` catalog keys, but the composition logic, `esc()` param-escaping (the XSS boundary), `expandSystemStrings`
   (`{system_settings}` tokens), and the snarkdown/`{@html}` render pipeline ALL stay. Error strings are resolved by
   `getMessage()` as PLAIN catalog lookups and do NOT go through ICU `format()` — their `{system_settings}` tokens and `esc` HTML
   entities collide with ICU's brace/apostrophe grammar (see Decision 2). The `friendly-error-parity.test.ts` must stay
   green and the golden fixture must NOT be regenerated (the errors `CLAUDE.md` forbids it). This tranche is the trickiest
   precisely because of these two non-ICU constraints; budget for it.
4. `commands` / command palette (~77, label + description).
5. `menu`, then the long tail by feature directory (search, viewer, onboarding, file-operations dialogs, …).

Per tranche tests: an area parity test (rendered base-locale output unchanged) + updating that area's existing tests to
the new call shape. Don't migrate fixed-format/technical strings (debug panels, log lines, dev-only catalogs) — those
aren't user-facing; the extraction dry-run flags them, the lint exempts them.

**Surfaces beyond markup props — enumerate so none are missed:** the document/window `<title>`s, imperatively-set
`aria-label`s (not just markup attributes), toast strings (the `composeTransferCompleteToast`-style composers return the
`t()` result; the `addToast` call site just displays it — both sides migrate, but the string is BORN in the composer),
and `Intl.Segmenter`/role values which are NOT user copy (leave). **Native menu labels are a real open question
(see Open decisions):** the macOS menu is built in Rust (`muda`), so its English labels live in Rust. "Don't touch the
backend / Rust stays word-free" was about ERROR PROSE (step 1), not menu labels — those are an un-migrated surface. This
step does NOT migrate native menu labels; resolving them (FE passes a label map at menu-build time, or a Rust-side
catalog) is deferred and called out below.

**`pluralize.ts` lifecycle:** ICU plurals replace `pluralize()` in migrated areas, but `pluralize.ts` and the
`pluralize-noun` Go check stay live until the LAST tranche removes the last caller (the migration runs for many
commits). Note: the `pluralize-noun` check scans source, not catalog JSON, so ICU plurals inside catalog files aren't
covered by it — their correctness is covered by the per-area parity tests instead. Remove `pluralize.ts` + retire the
check only in M3 once no caller remains.

## Milestone 3 — Enforcement complete

When the tranche list is exhausted: the no-raw-string lint's area allowlist is empty (it covers all areas it CAN cover
— see the honesty caveat next), the dead-key codegen warning is clean, and `$lib/intl`/`messages/` docs are final.
"i18n-ready" finish line: machinery + base catalog + enforcement in place; real translations are a later effort.

**Honesty caveat:** the no-raw-string lint is a closed-set heuristic (specific sinks, Decision/M1), so "allowlist empty"
proves every KNOWN sink position is covered, NOT that zero user-facing strings escaped. A string in an unrecognized
position can slip through. Don't over-claim completeness; the lint is a strong ratchet, not a proof. A periodic manual
sweep (or re-running the M1 extraction dry-run) catches the long tail.

## Checks to run

Per milestone: `pnpm check --fast` while iterating; full `pnpm check` at each milestone close (never pipe its output —
`no-tail-checker`). Specifically exercise: `svelte-check`/typecheck (the generated union is load-bearing),
`desktop-svelte-eslint` (new lints), `oxfmt`, `svelte-tests`, and the new codegen/extraction scripts. Confirm
`bindings`-style generated files aren't hand-edited. Fix or justify every warning (`no-ignored-warnings`).

## Tests strategy summary

- **TDD (real red→green):** the runtime resolution + fallback chain, the codegen (union + missing/dead-key detection),
  the `@key` stripper, the naming + no-raw-string lints. These are pure logic where regressions are silent — test-first.
- **Parity nets (write the assertion against current output BEFORE migrating each area):** transfer toasts (M0), then
  per-area in M2; the step-1 error parity snapshot must stay green through the errors tranche.
- **Component test:** `<Trans>` renders text + an inline interactive snippet in order, and remains XSS-safe (a message
  with `<script>`-looking text renders as literal text).
- Written-after: docs, the extraction dry-run (a one-shot analysis script, not a guarded behavior).

## Parallelization

Mostly sequential (M0 → M1 gates the design; M1 infra gates M2). Within M2, area tranches are independent and COULD run
in parallel worktrees, but they share `keys.gen.ts` and the lint allowlist, so parallel tranches race on those two
files — only parallelize if each agent owns regenerating + merging those, or serialize the regen step. Given we're not
in a hurry, sequential tranches are the safe default.

## Files in scope (verify before editing)

- New: `apps/desktop/src/lib/intl/messages.svelte.ts`, `Trans.svelte`, `keys.gen.ts` (generated), the codegen script,
  the metadata stripper, `messages/en/*.json`, `messages/screenshots/`, `messages/README.md`.
- New checks: a naming check + `cmdr/no-raw-user-facing-string` (under `apps/desktop/eslint-plugins/`, pattern of
  `no-raw-locale-format.js`), plus codegen wiring into `scripts/check/`.
- Edited (infra): `$lib/intl/CLAUDE.md` + `DETAILS.md`; `package.json` (`intl-messageformat`); the Vite/build glue if
  the catalog needs a load/merge step; `eslint.config.js`.
- Edited (M2 migration, per tranche): `transfer-complete-toast.ts` (+ test), `settings-registry.ts`, `$lib/errors/*`,
  the command registry, and feature components area by area.

## Definition of done (for the infra milestones; M2 is incremental)

- `t()`/`<Trans>` resolve from `messages/en/*.json` via `intl-messageformat`, reading `getLocale()`, reactive in markup.
- `MessageKey` union generated; a wrong key is a typecheck error; missing/dead keys are reported.
- Naming check + no-raw-string lint live (allowlist shrinking per migrated area).
- `transfer-complete-toast.ts` migrated; its base-locale output is byte-identical to today (parity test green).
- `pnpm check` green; docs (`$lib/intl` CLAUDE/DETAILS, `messages/README.md`) updated.
- The extraction dry-run has produced the real string count + the multi-variable/rich-text inventory for M2 planning.

## Open decisions for David (resolve before/at M0)

1. **ICU engine:** `intl-messageformat` (ICU MF1, recommended — tooling/LLM familiarity) vs `@messageformat/core` (MF2,
   newer). Recommend MF1 now.
2. **Catalog layout:** per-area-central under `messages/<locale>/` (recommended) vs fully colocated fragments next to
   each feature dir (more on-brand for Cmdr's colocation principle, more build/merge complexity).
3. **Embedded numbers:** pass `$lib/intl`-preformatted count strings into ICU messages (keeps formatting single-sourced,
   recommended) vs use ICU's inline `{n, number}` (lets translators control placement, but splits formatting ownership).
4. **Per-key param typing:** generate it now (more codegen, catches missing params) vs defer (key-union only). Recommend
   defer to a later upgrade.
5. **Native menu labels (Rust-built via `muda`):** out of scope for this step, but how to localize them eventually —
   FE passes a label map to the menu builder at build/locale-change time (keeps words on the FE, matches the overall
   principle) vs a Rust-side catalog. Recommend the FE-label-map approach when it's tackled; for now, explicitly
   deferred. (Flagged because it's the one user-facing surface that isn't FE-owned.)
