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

## Orchestration gotchas (for whoever automates this)

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
