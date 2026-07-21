# Translation learnings (shared, cross-language)

A running log of process learnings discovered WHILE translating, so each batch inherits what the last one learned
instead of rediscovering it. The split (per `i18n-translation.md`): per-LANGUAGE findings (a term, a formality call) go
in that language's `style.md` / `glossary.md`; CROSS-language learnings (an ICU mechanic, a tooling gotcha, a pile trap)
go HERE or in the guide. When a learning becomes a hard rule, promote it into the guide or `how-to-mine.md`.

## Pipeline (how a wave-1 locale gets made)

1. **Scaffold the skeleton**: `node apps/desktop/scripts/gen-locale-skeleton.ts <tag>` writes `messages/<tag>/*.json`
   with the English values in place and each `@key.sourceHash` already stamped. Translators EDIT values in place and
   never touch the hash.
2. **Build the glossary first**: mine the pile for the recurring core terms and record them in `<tag>/style.md` +
   `glossary.md` (`chosen · sources · confidence`) BEFORE translating strings, so term choices stay consistent.
3. **Translate values in place**, file by file, following the per-language style guide + each key's `@key.description`
   (the per-string context) + the ICU rules in the guide's agent-prompt block.
4. **Check**:
   `pnpm check desktop-i18n-parity desktop-i18n-icu desktop-i18n-plural desktop-i18n-stale desktop-i18n-coverage desktop-i18n-dont-translate`.
   Parity/ICU/plural are ERROR (must pass). Coverage (WARN) lists every value still byte-identical to English — that's
   the honest "what's left untranslated" signal; drive it to only the legitimately-identical keys.
5. **Overflow-check** later against the pseudolocale (`en-XA`) per the guide.

## Catalog mechanics (verified 2026-06-21)

- A non-`en` catalog file is interleaved `"key": "value"` + `"@key": { "sourceHash": "<7 hex>" }`, keys in `en` source
  order, NO `description` (that's `en`-only per-string context). The pseudolocale generator (`gen-pseudolocale.ts`) is
  the canonical shape; the skeleton generator emits the same shape with English values.
- `sourceHash(englishValue)` = first 7 hex of SHA-256 (in `i18n-catalog-lib.ts`). The skeleton stamps it; the stale
  check (`desktop-i18n-stale`) compares it. Editing only the VALUE keeps the hash valid (it hashes the English source,
  which didn't change).
- ICU vs raw split: every `errors.*` key renders RAW (normal apostrophes, literal `<…>`, `{token}` as a literal
  replacement target, markdown passed through). Every other key is ICU (double apostrophes `''`, real `<tag>`, ICU
  plural/select). `isRawKey()` is the single source of that split. The agent-prompt block in the guide states both.

## Source-quality traps

The full, durable list lives in [`reference-pile/how-to-mine.md`](reference-pile/how-to-mine.md) § Source-quality traps
(sibling/variant splits, no-macOS languages, English-valued Siri-intent files in macOS bundles, Microsoft's wrong-sense
first hit, per-language formality, catalog-tag ≠ pile-folder). Read that section before mining any language.

## Reviewing keys a feature added in passing

Keys added mid-feature by an implementation agent (rather than by a translator following this process) fail in a
characteristic way, found across all nine locales in the 2026-07 quality pass. The defect is almost never a
mistranslation: the string is fluent and the checks pass. It is drift away from what the locale had already settled.

- **The strongest evidence is the target locale's own catalog, not the pile.** A mid-feature agent pattern-matches its
  neighbouring keys, so it can't see that the same feature's Settings pane already ships a different word for the same
  concept. Before mining, grep the locale for the feature's other surfaces: a term the app already ships for the SAME
  concept outranks a pile term for a merely similar one. The pile settles what a term is; the catalog settles which of
  two correct renderings this app uses. (Found this way: de `indexieren`→`indizieren`, hu `kihagyva`→`kizárva`, sv
  `foton`→`bilder`, vi `ảnh`→`hình ảnh`, zh `图片`→`图像`, nl `hernoemen`→`naam wijzigen`, pt `análise`→`varredura`.)
- **A frequency count finds the fork instantly.** `grep -oh '"<variant>"' <tag>/*.json | sort | uniq -c` over the locale
  dir shows the settled term at 15–60 hits and the drifted one at exactly the size of the new batch. Cheaper than
  re-deriving any key.
- **A new feature imports one wrong head term everywhere at once.** The drift is per-FEATURE, not per-key, because the
  feature is named after one verb. Settle the head term first, then translate the family.
- **A batch also drifts against itself.** Sibling keys ended up with two renderings of their own core noun (pt
  `alteração de nome` vs `renomeação`). Grepping a new batch for two renderings of its own head noun finds the unsourced
  one fast.

## Defect classes the checks cannot see

Parity, ICU, plural, stale, coverage, and don't-translate are structural. These passed on 100% of the below, so the
style guide and a human-shaped review are the only defense:

- **Wrong regional variant.** A fully pt-PT string is structurally perfect. Pluricentric languages need a concrete
  grep-list of variant tells in their style guide (vocabulary plus one grammatical construction), not just a recorded
  variant decision: the decision doesn't reach an implementation agent's defaults. Applies to pt/pt-BR, es/es-419,
  zh-Hans/Hant, fr/fr-CA, nl/nl-BE.
- **Formality regression.** A single `tu` string sat in an otherwise fully-`vous` fr catalog. Any T/V language needs
  newly-added keys spot-checked against [`formal-informal-decisions.md`](formal-informal-decisions.md).
- **Typographic U+2019 leaking from the English source** into a locale whose catalog is otherwise ASCII. It isn't an ICU
  escape, so nothing flags it.
- **Locale number typography**, above all the space before `%` (de, fr, sv all require it). Tell translators to grep
  their own catalog for `%` before finishing; these slip in one key at a time.
- **An elided verb in an uninflected language.** vi rendered "may or may not be covered" as "may have or not" and it
  still read as fluent text. Only bites on transparency surfaces, where the elided word was the whole point.

## Copy-shape contracts worth stating once

Three recurring string families whose SHAPE is the thing that drifts, so state the contract rather than "keep them
parallel":

- **A warning badge is a state label, not an action.** English hides this because "(overwrite!)" is noun/verb ambiguous;
  any language with a distinct imperative produces a badge that instructs the user to do the very thing the row is
  blocking. Translate a badge family together and keep it one part of speech (`(cycle)` / `(extension)` / `(overwrite!)`
  is noun-noun-noun, not noun-noun-verb).
- **A doing/done tool pair needs a grammatical contract.** In pro-drop languages a bare finite past reads as "_he/she_
  did it" (es "Preparó"), so the done arm must be impersonal, passive, or participial. In case-marking languages state
  it explicitly (de: "done = doing minus the auxiliary, subject in the nominative"), or an agent pattern-matches into a
  bare infinitive that changes the mood entirely.
- **When adding a key to an existing parallel FAMILY, match the family's recorded shape, not the adjacent line.** fr had
  the pattern in its glossary and the new pair still broke it, because the agent copied its neighbour.

## Reference-pile notes

- **macOS AppKit's save-changes dialog is the Tier-1 source for any "review pending changes before applying" surface.**
  `Review Changes…` / `Review Unsaved` exist in every localized macOS, so a whole rename-review family has first-party
  evidence in every language. Worth knowing because "review" has no Finder hit and Microsoft's TBX first hit
  (評審/evaluation sense) is wrong for this surface.
- **Verify a language's pile inventory by `ls`, don't trust a style guide or a task brief.** Both the vi style guide and
  the 2026-07 batch brief under-listed the available sources; vi and sv both do have `total-commander/`. A brief that
  says "no orthodox source for this language" is a claim to check, not a constraint to design around.
- **`remove` vs `delete` is a systematic trap.** macOS and Microsoft render both with one verb in many languages, so
  Tier-1 evidence actively pushes toward a verb meaning "delete" on a button that doesn't delete. Check this pair
  explicitly per locale instead of following the pile (es `Quitar`, fr `Retirer`, vi `Gỡ` all diverge deliberately).

## Orchestration gotchas (for whoever automates this)

- **Give each parallel agent its own scratch directory.** With nine language agents running concurrently, helper scripts
  written to a shared `/tmp` path overwrote each other mid-run: one agent's `xref.sh` became another's and started
  returning German for Dutch queries. Use per-agent scratchpad paths, and sanity-check that a mining helper's output is
  actually in your language.
- **Don't pass the batch spec via the Workflow `args` global — hardcode it in the script.** A first batch-1 run received
  `args` as a JSON STRING (not a parsed array); the `args && args.length` guard let the string through, the loop
  iterated over its CHARACTERS, and every iteration spawned units with `tag = undefined` — ~848 agents, ~36M tokens, all
  wasted (the translators safely refused to invent an `undefined` locale, so nothing corrupted). Hardcode the tag list,
  and fail-fast: assert every tag is known AND its locale dir exists BEFORE spawning any agent, so a bad value can't fan
  out.

## Per-batch notes

### Batch 1 — de, fr, es

**Result (all complete, verified 2026-06-21):** parity/icu/plural/stale all clean for de/fr/es; coverage residuals are
100% legit short tokens (cloud-provider brand names, units, loanwords, placeholder-only) — a phrase scan found ZERO
missed sentences. Spot-check passed both paths (ICU `''`; raw `errors.*` normal `'`; fr `vous`, es `tú`; es used the
prescribed gender-neutral "Te damos la bienvenida"). Cost: 24 unit-agents, ~4M tokens, ≤3 concurrent. Two reusable wins:
the **shared `glossary.md` is the cross-file coordination point** (concurrent unit-agents read + append + reconcile term
clashes mid-run), and **the `many` CLDR plural category is the most common slip** for fr/es (English has only
`one`/`other`; add a `many` branch to every ICU plural for Romance/Slavic locales).

Pilot (de: `feedback.json` + `crashReporter.json`, 2026-06-21) validated the pipeline. Learnings:

- **Read the parallel `en/<file>.json` for each key's `@key.description`** — the skeleton carries no descriptions. This
  is the per-string context; skipping it loses screenshot/placeholder notes. Mandatory.
- **Term home is `style.md`, not `glossary.md`.** The wave-1 guides carry their sourced glossary inline in `style.md`;
  `glossary.md` is a near-empty stub. Read `style.md` first; ADD newly-settled terms to `glossary.md` as you go.
- **Match the English source faithfully; flag inconsistencies, don't silently fix them.** The en catalog has minor
  inconsistencies (e.g. `Sending…` single-char ellipsis vs `Sending...` three dots across files). Preserve each value's
  exact form; note the inconsistency for David rather than normalizing it.
- **Cross-file term consistency is a real risk** when files are translated independently: a string referencing a UI
  section by name (e.g. "Change in Settings > Updates") must match how that section is translated in `settings.json`.
  Capture UI section names in the glossary so independent translators agree. Flag any forward reference.
- **ICU tag/placeholder preservation works** when stated explicitly: `<github>…</github>`, `<call>…</call>`, and
  `{email}`/`{maxText}` all survived. The parity + icu checks catch any slip.
- **Pure-placeholder values stay identical to English** (e.g. `{currentText} / {maxText}`) and correctly remain in the
  coverage "identical" list — that's not an untranslated miss, it's nothing-to-translate. Expect a few legit identicals.
- **Validate with the direct node scripts**, not (only) `pnpm check`:
  `node scripts/i18n-check-{parity,icu,plural,stale,coverage}.js` from `apps/desktop`. The Go check runner's cache can
  serve a stale "no non-en locales" result on the first run right after a new locale dir appears; the node scripts
  always resolve the live catalog.

### Quality pass — 54 mid-feature keys, all nine locales

The bulk-rename review, image-index scope, and Ask Cmdr tool-label keys were added by implementation agents mid-feature
and never went through this process. Re-reviewed against the guide, the style guides, and the pile: **168 of 486 values
changed** (de 19, es 11, fr 15, hu 12, nl 26, pt 24, sv 21, vi 21, zh 19); the rest were confirmed and kept
byte-for-byte. All six i18n checks passed BEFORE the pass as well as after, which is exactly why the § "Defect classes
the checks cannot see" list above exists: every finding was invisible to tooling.

Shape of the findings, in rough order of frequency: term drift against the same feature's other surface in the same
locale (every language), copy-shape breaks in the badge and doing/done families (six languages), and one regional
variant contamination (pt shipped European Portuguese in about a third of the batch, the only pt-PT leak in the whole
30-file catalog). Only two values across all nine locales were legitimately identical to English (fr `(cycle)` and
`(extension)`), both already carrying a `sameAsSourceJustification`, both rewritten to cite a real source.
