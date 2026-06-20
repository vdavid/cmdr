# Translating Cmdr (the translator process)

The translator-facing companion to [`i18n.md`](i18n.md). `i18n.md` is the developer map: how the catalog, runtime, and
checks work. THIS guide is the process you follow to add a language or translate new strings, plus the reusable
agent-handoff block. Mechanism lives in `i18n.md` and the colocated docs; this guide points to it, never restates it.

Translation is agent-driven and human-reviewed (principle 6: anything meeting human eyes is made or closely reviewed by
a human, since agent translation is a draft, not a ship). No language-specific content lives in the repo docs;
everything below talks about "the target language" and its per-language style guide.

## Three inputs, kept separate

Set a translator (human or agent) up for excellence with three inputs, never mixed:

1. **Per-string context**: the `@key.description` + `placeholders` (+ optional `screenshot`/`screenshotNote`). Authored
   for every key; surface, trigger, constraints, do-not-translate tokens, plain-language placeholder meanings. See
   `apps/desktop/src/lib/intl/messages/DETAILS.md` § `@key` metadata schema.
2. **Per-language style guide**: tone, voice, formality (T/V distinction if the language has one), terminology and
   glossary, how brand words are handled in this language. NOT per-string; never repeat tone on every key. It lives at
   `docs/i18n/<tag>/style.md` (start from [`/docs/i18n/_template/style.md`](../i18n/_template/style.md); see
   [`/docs/i18n/README.md`](../i18n/README.md)). These are working notes, not catalog data: the app never loads them.
   **Treat it as a living doc, and capturing is part of the job**: read it before translating AND extend it as you go,
   recording each glossary choice with its sources and a confidence (see Researching terms below). This isn't only for
   terms: whenever you hit a convention, gotcha, decision point, or rule that wasn't already written where you looked for
   it, write it down so the next translator inherits it instead of rediscovering it. Per-language findings go in the
   style guide; a missing cross-language rule (like an ICU mechanic) goes in this guide or the template.
3. **One ICU instruction**: given once in the agent system prompt, not per string (see the block below).

## Researching terms: the reference pile

Checking the reference pile is MANDATORY for every term: mine it for the term and for similar sentences, reuse and cite, never guess. The reference pile holds authoritative localizations
keyed by language: the ~3 GB of macOS, Microsoft, and GNOME/Xfce data sits gitignored at `_ignored/i18n/<tag>/` (one
folder per language), and the docs explaining it are tracked in the repo. Read
[`reference-pile/README.md`](../i18n/reference-pile/README.md) for what's there and the authority tiers, and
[`reference-pile/how-to-mine.md`](../i18n/reference-pile/how-to-mine.md) for tested per-source recipes (greps, jq,
`msggrep`, `pdftotext`).

For each term or convention: triangulate across every source the language has (macOS is highest authority, then
Microsoft, then GNOME/Xfce), pick the most native-sounding fit for Cmdr's voice, then record it in the style guide's
glossary as **chosen · sources · confidence**. Confidence is `confirmed` (a human signed off), `high` (authoritative
sources agree), or `tentative` (sources conflict or none had it). Push every `tentative` term, and any unresolved
formality/voice call, into the style guide's "Decisions to confirm with David" section rather than burying it.

## Gender and inclusive language

For gendered languages, one rule: achieve inclusivity by neutral RESTRUCTURING, never by typographic glyphs. Avoid the
German gender star/colon (`Benutzer*innen`, `Benutzer:innen`), the French midpoint (`étudiant·e·s`), Spanish/Portuguese
`-e`/`-x` (`todes`, `todxs`), the Italian schwa (`tuttə`), and Cyrillic/Hebrew/Arabic splits. Those forms break screen
readers (against Cmdr's AA+ a11y principle), are receding even at Microsoft (which dropped the German gender star), and
are politically loaded. Apple and Microsoft both prescribe restructuring instead.

Restructure by naming the object or action, not the person, which dodges both gendered moments at once:

- The role-noun (German's case): `mit 3 Benutzer*innen geteilt` → `Für 3 Personen freigegeben` (or the neutral
  participle `Benutzende`).
- User agreement (French/Spanish/Italian/Slavic): `Vous êtes connecté·e` → `Connexion établie` (the status agrees with
  the connection, not the user); `Bienvenidos`/`Bienvenides` → `Te damos la bienvenida`.

Prefer verbal-noun or imperative button labels, second person, present tense, and collective/role nouns. A file manager
is mostly commands and status, so this is almost free.

**Only restructure where it still reads naturally.** If neutral phrasing would be stilted or unidiomatic, don't ship the
awkward version: flag it as a "Decisions to confirm with David" item instead. The generic (usually masculine) form is
the documented last resort, used only when natural restructuring genuinely isn't available.

## Add a new language

1. **Pick the BCP-47 tag.** A language base (`xx`) for the universal set, or a region variant (`xx-YY`) when a region
   needs overrides. The tag is a format identifier, not translatable. The base is the fallback for its variants; `en` is
   the final fallback. Convention + resolution order: [`i18n.md`](i18n.md) § Locale-format convention.
2. **Create the skeleton.** Mirror `en/`'s files and keys under `messages/<tag>/`, and stamp each translated key's
   `@key.sourceHash` = the 7-char hash of the exact English value it was translated from (computed by `sourceHash()` in
   `apps/desktop/scripts/i18n-catalog-lib.js`; the pseudolocale generator does exactly this and is the reference). The
   hash is what `desktop-i18n-stale` uses to know a translation is still current.
3. **Write the per-language style guide** (input 2 above).
4. **Translate** with the agent-handoff block below, feeding each key its `@key` context + the style guide.
5. **Run the checks**:
   `pnpm check desktop-i18n-parity desktop-i18n-icu desktop-i18n-plural desktop-i18n-stale desktop-i18n-coverage desktop-i18n-dont-translate`.
   Parity (placeholder/tag/token sets), ICU validity, and plural coverage are ERROR class (a failure is a runtime
   break); stale, coverage, and don't-translate are WARN class. What each catches: [`i18n.md`](i18n.md) § Enforcement.
6. **Overflow-check the layout.** Drive the app and look for clipping; the pseudolocale (`en-XA`) is the deliberately
   long stand-in for this. See [`i18n.md`](i18n.md) § Pseudolocale.
7. **Human review** every string (principle 6). Set `@key.reviewed: true` per key as a human signs it off; the stale
   check clears it whenever the source changes, so review state stays honest.
8. **Ship.** One piece is still missing until the first real locale: the runtime resolver (the dir-selection layer that
   picks `messages/<tag>/` from the OS locale, plus a language selector). It's a small, contained follow-on the
   convention keeps the seam clean for; today the loaded catalog is hardcoded to `en`. See [`i18n.md`](i18n.md) § Add a
   new locale.

## New feature → add strings and translate to ALL languages

The routine maintenance loop, run for every change that adds or edits user-facing copy:

1. **Add or edit the `en` key** with a `@key.description` that meets the bar (`messages/DETAILS.md` § `@key` metadata
   schema, noting the fragment-key and pass-through-placeholder requirements). Run `pnpm intl:keys` to regenerate the
   key union.
2. **For each existing locale**, read its style guide, translate the new or changed keys, and update each touched key's
   `@key.sourceHash` to the new English value's hash.
3. **Run the checks** (same set as step 5 above). `desktop-i18n-stale` is the safety net here: editing an `en` value
   changes its hash, so EVERY locale's translation of that key reads as stale until re-translated and re-hashed. You
   can't silently leave a locale behind on a copy edit: the stale warning lists exactly which keys each locale owes.
4. **Human-review** the changed strings and set `@key.reviewed: true` again (the stale check reset it when the source
   changed).

### Write placeholder strings to be restructurable

When a user-facing string contains a placeholder (`{path}`, `{name}`, `{volumeName}`), phrase the English so a translator
can move that placeholder into a grammatically neutral slot. Many languages must change the grammar *around* a
placeholder based on its (unknown) runtime value: a Hungarian/Finnish/Turkish case suffix that has to vowel-harmonize
with `{path}`, a German/Slavic case ending, a Celtic initial mutation. They can only handle that by reordering or
reshaping the sentence (ICU allows reordering placeholders). So as the author, don't lock a placeholder into a slot that
forces agreement: avoid `the {fileName}'s owner`; prefer `Owner: {fileName}` or `This file belongs to {owner}`. A
`@key.description` that names what each placeholder holds is what lets the translator restructure safely.

This is a forward discipline only: existing English-only strings need no retrofit (nothing is translated yet, so nothing
is broken). If a future translation ever surfaces an English string that genuinely can't be restructured, fix that one
string then.

## The translator-agent context (reusable system-prompt block)

Hand an agent the block below as its system prompt, then feed it batches of keys with each key's `@key.description`,
`placeholders`, and any `screenshot`/`screenshotNote`. Replace the bracketed parts; keep the rest verbatim.

```
You are translating UI strings for Cmdr, a macOS file manager, from English into [TARGET LANGUAGE].

STYLE: Follow this per-language style guide for all tone, voice, formality, and terminology decisions:
[PASTE THE PER-LANGUAGE STYLE GUIDE]

ICU (do this for every string):
- Preserve every {placeholder}, every <tag>…</tag>, and every ICU plural/select structure EXACTLY. Translate only
  the human-readable text between them. Never rename, drop, add, or reorder a {placeholder} or <tag>.
- Reorder placeholders within a sentence only as your language's grammar needs. The set of placeholders must stay
  identical to the English.
- Write plural/select branches for the TARGET language's CLDR plural categories (zero/one/two/few/many/other as your
  language requires), not English's. ICU only selects and fills; all grammatical correctness comes from the branches
  you write.
- Double EVERY apostrophe in a value (' becomes ''). ICU treats a lone ' as an escape character.

UNCONTROLLED INSERTS: Placeholders like {message}, {reason}, and a raw {path} carry text Cmdr does not control (an OS
error string, a file path with any characters or length). Structure the sentence so it reads correctly no matter what
value lands there. Never assume gender, number, capitalization, or length of the inserted value.

FRAGMENT KEYS: Some keys are sentence fragments assembled at runtime by a named *Join key (the description names the
assembler). Translate each fragment so the assembled phrase reads naturally in your language, and mind word order: if
your language orders the parts differently, the *Join key is where the order is expressed.

ERRORS ARE RAW: Any key under errors.* does NOT use ICU. There, use NORMAL apostrophes (doesn't, not doesn''t),
keep {token} verbatim as a literal replacement target (never add ICU formatting), treat <…> as literal text, and pass
markdown (#, **, backticks) through untouched. The catalog's @key context flags these. Full note: i18n.md § Error
pipeline.

GENDER: Achieve inclusivity by neutral RESTRUCTURING, never typographic glyphs (no German *innen/:innen, no French
·e·, no Spanish/Portuguese -e/-x, no Italian schwa, no Cyrillic/Hebrew/Arabic splits — they break screen readers). Name
the object or action, not the person: verbal-noun or imperative labels, second person, present tense, collective/role
nouns, status that agrees with the object ("Connection established", not "You are connected"). Do this ONLY where the
result still reads naturally; if neutral phrasing would be stilted, flag the string for human review rather than ship an
awkward rewrite or an exposed gendered default.

REFERENCE PILE AND GLOSSARY (mandatory): before translating, mine _ignored/i18n/[TARGET LANGUAGE TAG]/ for how Apple/Microsoft/GNOME render each term and for similar sentences to model phrasing on; reuse and cite, never guess. Read and extend the language glossary at docs/i18n/[TAG]/glossary.md as you settle terms (chosen, sources, confidence). Recipes: _ignored/i18n/how-to-mine.md.

DON'T TRANSLATE: Keep brand and system tokens verbatim: Cmdr, macOS, GitHub, SMB, MTP, Quick Look, and the
{system_settings}-style tokens. The full curated list is BRAND_WORDS + SYSTEM_TOKENS in
apps/desktop/scripts/i18n-catalog-lib.js, and the desktop-i18n-dont-translate check enforces it.

OUTPUT: For each key, return only the translated value. A human reviews everything you produce before it ships, so
flag any string where the context was insufficient to translate confidently rather than guessing.
```

The two "uncontrolled inserts" and "fragment keys" paragraphs come from the catalog audit: they're the two highest
blind-translation risks once placeholders and structure are otherwise handled. They're encoded into the
description-quality bar (`messages/DETAILS.md`), so a well-described key already flags both, but stating them once in
the prompt makes the agent defensive by default.
