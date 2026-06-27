# Translating Cmdr (the translator process)

The translator-facing companion to [`i18n.md`](i18n.md). `i18n.md` is the developer map: how the catalog, runtime, and
checks work. THIS guide is the process you follow to add a language or translate new strings, plus the reusable
agent-handoff block. Mechanism lives in `i18n.md` and the colocated docs; this guide points to it, never restates it.

Translation is agent-driven. Human review is the documented ideal but **not** a ship gate (see the override below). No
language-specific content lives in the repo docs; everything below talks about "the target language" and its
per-language style guide.

## Human review is not a ship gate (deliberate override of principle 6)

Principle 6 (`AGENTS.md`: anything meeting human eyes is made or closely reviewed by a human) is **explicitly overridden
for translations**, and only for translations. Recruiting trustworthy native reviewers across ~100 languages isn't
feasible for the foreseeable future, so **a locale ships machine-made**: translated by the agent, passing the checks,
overflow-checked. Human review does not block shipping.

The review infrastructure stays and is used opportunistically: `@key.reviewed: true` records a human sign-off when one
happens (and `desktop-i18n-stale` clears it when the source changes, so the flag never lies). If we get native reviewers
for a language, wonderful — we record it and quality goes up. Until then, absence of `reviewed` is the normal shipped
state, not a blocker. This override is the reason the steps below treat review as optional, not required.

Everything else in principle 6 stands — this carve-out is translations only, not a general license to skip human review
elsewhere.

## Treat every language the same — Hungarian included

Never special-case a language. In particular, **never give Hungarian preferential treatment, extra input, or a shortcut
the process doesn't give every other language**, even though David is a native Hungarian speaker. Hungarian is his
fluency gauge: he reads the shipped Hungarian to judge how well the whole language-agnostic pipeline (reference pile,
style guides, the agent process, the checks) actually performs. Any Hungarian-specific exception, hand-fed term, or
native gut-check he injects would contaminate that gauge — he'd be measuring his own corrections instead of the system.
So when a Hungarian term is unsettled, resolve it the same way you would for a language no one on the team speaks:
triangulate the reference pile, pick the best-evidenced fit, record the confidence, and flag what stays `tentative`.
Don't ask David to break the tie for Hungarian.

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
   terms: whenever you hit a convention, gotcha, decision point, or rule that wasn't already written where you looked
   for it, write it down so the next translator inherits it instead of rediscovering it. Per-language findings go in the
   style guide; a missing cross-language rule (like an ICU mechanic) goes in this guide or the template.
3. **One ICU instruction**: given once in the agent system prompt, not per string (see the block below).

## Term-choice principles

Three settled decisions about WHICH term to pick. They apply to every language; the don't-translate check enforces what
it can, but the judgment is yours.

1. **Apple feature names — localize what Apple localizes.** Some Apple feature names are translated per-OS (Quick Look →
   fr "Coup d’œil", de "Übersicht", es "Vista rápida"); others Apple keeps English in every locale (Spotlight, Mission
   Control, AirDrop, Siri, Time Machine, Finder). To decide, check `<tag>/macOS/` in the reference pile: if Apple's
   localized macOS uses a translated term, use it; if it keeps the English name, keep it. Match what the user actually
   sees in their Finder. (This is why `BRAND_WORDS` in `apps/desktop/scripts/i18n-catalog-lib.js` lists the kept-English
   names but NOT Quick Look — a translated Quick Look must not read as a dropped brand.)
2. **Prefer the macOS Finder term when macOS and Windows/Microsoft differ.** Cmdr is a macOS app, so the native-OS term
   wins over the Windows convention. For example, pt-BR delete = "Apagar" (Finder), not "Excluir" (Windows); German move
   = "Bewegen" (Finder), not "Verschieben" (Microsoft). The Microsoft terminology entry is the Windows wording, not ours
   — use it only as a tiebreak below macOS. (Same shape as the formality trap in
   [`reference-pile/how-to-mine.md`](../i18n/reference-pile/how-to-mine.md) § Source-quality traps trap 5.)
3. **Brand/product names may inflect.** In agglutinative/inflecting languages, let the brand take its natural
   inflectional suffix rather than forcing an unnatural bare form: Hungarian "Cmdrben" (in Cmdr), Swedish genitive
   "Cmdrs". Keep the brand recognizable but grammatical. Pick the suffix by how the name is PRONOUNCED, not spelled:
   vowel harmony in Hungarian/Finnish/Turkish keys off the SPOKEN form, so "Cmdr" read aloud as "commander" harmonizes
   to the vowels you'd hear, not to the bare consonant cluster. Apply your language's harmony rules to the
   pronunciation. The don't-translate check is suffix-aware (`hasBrandPresent`), so an inflected brand passes; an
   omitted one is still flagged.

## Researching terms: the reference pile

Checking the reference pile is MANDATORY for every term: mine it for the term and for similar sentences, reuse and cite,
never guess. The reference pile holds authoritative localizations keyed by language: the ~3 GB of macOS, Microsoft, and
five file managers — the explorer family (GNOME Nautilus, Xfce Thunar, KDE Dolphin) plus the orthodox two-pane pair
(Total Commander, Double Commander) — one folder per language. Read
[`reference-pile/README.md`](../i18n/reference-pile/README.md) for what's there and the authority tiers, and
[`reference-pile/how-to-mine.md`](../i18n/reference-pile/how-to-mine.md) for tested per-source recipes (greps, jq,
`msggrep`, `pdftotext`, `.lng`).

> [!IMPORTANT] **Where the pile is — and why a worktree can't see it.** The pile is gitignored (`_ignored/` is
> untracked), so it lives ONLY in the main clone, at **`~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/`**. It is NOT
> copied into git worktrees. Translation almost always runs from a worktree (`.claude/worktrees/<slug>/`), and there a
> relative `_ignored/i18n/` **does not exist** — checking the worktree-relative path returns "absent" and tempts you to
> translate without the pile (guessing, which this guide forbids). So ALWAYS use the absolute main-clone path above. If
> you can't assume the path, resolve the main clone with `git worktree list | head -1` (its first column is the main
> checkout) and mine `<that path>/_ignored/i18n/<tag>/`. A "no reference pile present" conclusion is almost always this
> worktree trap, not a genuinely missing pile — re-check the main-clone absolute path before deciding it's gone.

For each term or convention: triangulate across every source the language has, pick the most native-sounding fit for
Cmdr's voice, then record it in the style guide's glossary as **chosen · sources · confidence**. Weight by authority:
macOS first, then Microsoft, then the file-manager corpora (community-translated, so below the first-party vendors for
general terms). Confidence is `confirmed` (a human signed off), `high` (authoritative sources agree), or `tentative`
(sources conflict or none had it). Record open terms in the style guide's open-decisions section rather than burying
them — but for Hungarian, resolve by evidence and don't park it for David (see § Treat every language the same).

### Mining the file-manager sources: four gotchas

These are reusable across every language — they're how to read the five file-manager catalogs without being misled:

1. **Match the source to Cmdr's UI family.** The orthodox two-pane pair (TC, DC) is Cmdr's design lineage and the only
   source for the concepts Finder lacks — pane, file list, command line, the button bar. The explorer family (Nautilus,
   Thunar, Dolphin) owns general file operations and has the broadest language coverage. A term lifted from the wrong
   family can mislead, so pick by which UI shares Cmdr's surface for that concept.
2. **A source may name a DIFFERENT concept, not just a different word.** The orthodox managers' "directory hotlist", for
   instance, is a related-but-distinct feature, not a translation of Cmdr's "bookmark". When the nearest match names a
   different feature, record the mismatch and keep looking — don't adopt its term as if it were yours.
3. **A feature may be a brand name in the references, giving no generic term.** Apple's "Quick Look" and TC's "Lister"
   are product names, kept verbatim (don't-translate), so they hand you no generic word for "viewer". When every
   reference uses a brand, choose a generic term from the generic-word evidence and flag it as `tentative`.
4. **A shared ROOT across sources is signal even when the form differs.** If two references render a term with the same
   root in different forms, that root is the evidence: pick the most standard form on it and record the variant, rather
   than treating the term as unsourced and inventing from scratch.

Some terms stay `tentative` even after all of this (the sources genuinely disagree, or none names the concept Cmdr does)
— that's a real outcome to record, not a failure to dig harder.

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

## Deliberately-identical strings (the `sameAsSourceJustification` field)

Some keys are CORRECTLY identical to English in your language and must never be force-translated: a brand name
(`Dropbox`, `OneDrive`), a unit symbol (`GB`, `kB`), a standard name (`ISO 8601`, `PDF`, `Unicode`), a placeholder-only
string (`{width} × {height}`, `{systemSettings} > {appearance}`), or a real word the language genuinely shares with
English (German `Server`, French `Type`, Swedish `Smart`). Translating these would be a regression, not an improvement.

The `desktop-i18n-coverage` check flags every identical-to-English value as "possibly untranslated". To keep an honest,
clean coverage signal — every warning is a real gap, every deliberate identical is silenced WITH a recorded reason —
record a `@key.sameAsSourceJustification` on that key in YOUR locale catalog: a short, non-empty string saying why it's
deliberately identical, sourced like any other term decision. Example, in `messages/de/errors.json`:

```jsonc
{
  "errors.provider.dropbox.displayName": "Dropbox",
  "@errors.provider.dropbox.displayName": {
    "sourceHash": "1a2b3c4",
    "sameAsSourceJustification": "Brand name; kept verbatim in every locale (do-not-translate list).",
  },
}
```

Rules:

- It is a per-LOCALE judgment, so it lives in the locale catalog, never in `en`. German keeps `Server`; Spanish
  translates it to `Servidor` and gets a real value, NOT a justification. Decide per language, evidence-first.
- Repeat it per locale even for universal brands. Each translator vouches for each identical key in their own language;
  the repetition (the 30 `errors.provider.*` names justified in all 9 locales) is accepted, not deduplicated.
- It only silences the IDENTICAL signal, never MISSING. A key absent from the locale still reports.
- It is tied to the source like `reviewed`: if the English value later changes, the stale check flags the key so you
  re-confirm the justification (or translate it). Don't write a justification that would be false if English changed
  trivially.
- The bar is the SAME as a translation: only record a justification you can defend from the reference pile / glossary.
  "I couldn't be bothered" is not a justification. If a key actually needs translating, translate it — the field is for
  genuinely-identical strings only, and the goal is a clean coverage warn output WITHOUT lowering the quality bar.

Mechanism + schema:
[`/apps/desktop/src/lib/intl/messages/DETAILS.md`](../../apps/desktop/src/lib/intl/messages/DETAILS.md) § `@key`
metadata schema.

## Add a new language

1. **Pick the BCP-47 tag.** A language base (`xx`) for the universal set, or a region variant (`xx-YY`) when a region
   needs overrides. The tag is a format identifier, not translatable. The base is the fallback for its variants; `en` is
   the final fallback. Convention + resolution order: [`i18n.md`](i18n.md) § Locale-format convention.
2. **Create the skeleton.** Run `node apps/desktop/scripts/gen-locale-skeleton.js <tag>`: it mirrors `en/`'s files and
   keys under `messages/<tag>/` with the English values in place and each `@key.sourceHash` = the 7-char hash of the
   exact English value it was translated from (computed by `sourceHash()` in `apps/desktop/scripts/i18n-catalog-lib.js`;
   the pseudolocale generator does exactly this and is the reference). The hash is what `desktop-i18n-stale` uses to
   know a translation is still current.
3. **Write the per-language style guide** (input 2 above).
4. **Translate** with the agent-handoff block below, feeding each key its `@key` context + the style guide.
5. **Run the checks**:
   `pnpm check desktop-i18n-parity desktop-i18n-icu desktop-i18n-plural desktop-i18n-stale desktop-i18n-coverage desktop-i18n-dont-translate`.
   Parity (placeholder/tag/token sets), ICU validity, and plural coverage are ERROR class (a failure is a runtime
   break); stale, coverage, and don't-translate are WARN class. What each catches: [`i18n.md`](i18n.md) § Enforcement.
6. **Overflow-check the layout.** Drive the app and look for clipping; the pseudolocale (`en-XA`) is the deliberately
   long stand-in for this. See [`i18n.md`](i18n.md) § Pseudolocale.
7. **Human review (optional, not a ship gate).** If a native reviewer is available, set `@key.reviewed: true` per key as
   they sign it off; the stale check clears it whenever the source changes, so review state stays honest. Skipping this
   is the normal case — see the override above.
8. **Ship.** No code change is needed to make a finished locale live: the runtime resolver and the in-app picker
   (**Settings > Appearance > Language**) are built, so dropping a `messages/<tag>/` dir makes the locale load and
   appear in the picker, with the documented `<tag>` → base → `en` fallback per key. A locale ships once it's
   translated, passes the checks, and is overflow-checked — human review is opportunistic, not a gate (see the override
   above). See [`i18n.md`](i18n.md) § Add a new locale for the runtime mechanism.

## New feature → add strings and translate to ALL languages

The routine maintenance loop, run for every change that adds or edits user-facing copy:

1. **Add or edit the `en` key** with a `@key.description` that meets the bar (`messages/DETAILS.md` § `@key` metadata
   schema, noting the fragment-key and pass-through-placeholder requirements). Run `pnpm intl:keys` to regenerate the
   key union.
2. **Propagate the keys to every locale**: run `node apps/desktop/scripts/sync-locale-keys.js` (all locales) — it adds
   each new `en` key as an English skeleton with the correct `@key.sourceHash`, drops keys you removed, and preserves
   existing translations. Then, for each locale, read its style guide and translate the new/changed keys in place (the
   coverage check lists exactly what's still English).
3. **Run the checks** (same set as step 5 above). `desktop-i18n-stale` is the safety net here: editing an `en` value
   changes its hash, so EVERY locale's translation of that key reads as stale until re-translated and re-hashed. You
   can't silently leave a locale behind on a copy edit: the stale warning lists exactly which keys each locale owes.
4. **Human-review (optional)** the changed strings and set `@key.reviewed: true` again if a reviewer is available (the
   stale check reset it when the source changed). Not a gate — the re-translated strings ship without it.

### Write placeholder strings to be restructurable

When a user-facing string contains a placeholder (`{path}`, `{name}`, `{volumeName}`), phrase the English so a
translator can move that placeholder into a grammatically neutral slot. Many languages must change the grammar _around_
a placeholder based on its (unknown) runtime value: a Hungarian/Finnish/Turkish case suffix that has to vowel-harmonize
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

REFERENCE PILE AND GLOSSARY (mandatory): before translating, mine the reference pile for how Apple/Microsoft, the explorer file managers (GNOME Nautilus, Xfce Thunar, KDE Dolphin), and the orthodox two-pane pair (Total Commander, Double Commander) render each term and for similar sentences to model phrasing on; reuse and cite, never guess. The pile is gitignored and lives ONLY in the MAIN clone at the ABSOLUTE path ~/projects-git/vdavid/cmdr/_ignored/i18n/[TARGET LANGUAGE TAG]/ — it is NOT in your worktree, so a worktree-relative _ignored/i18n/ will look empty and that "absent" reading is the worktree trap, not a missing pile. If unsure of the path, run `git worktree list | head -1` and mine <that main-clone path>/_ignored/i18n/[TAG]/. Match the source to Cmdr's UI: for two-pane concepts the OS/explorer managers lack (pane, file list, command line), the orthodox pair is the closest lineage match. Mind the four mining gotchas in the guide's "Researching terms" section (wrong-family terms, a source naming a different concept, brand names that yield no generic term, shared-root signal). Read and extend the language glossary at docs/i18n/[TAG]/glossary.md as you settle terms (chosen, sources, confidence). Recipes: docs/i18n/reference-pile/how-to-mine.md.

DON'T TRANSLATE: Keep brand and system tokens verbatim: Cmdr, macOS, GitHub, SMB, MTP, and the {system_settings}-style
tokens. The full curated list is BRAND_WORDS + SYSTEM_TOKENS in apps/desktop/scripts/i18n-catalog-lib.js, and the
desktop-i18n-dont-translate check enforces it.

DELIBERATELY-IDENTICAL: When a value is CORRECTLY identical to English in your language (a brand, a unit symbol, a
placeholder-only string, or a word your language genuinely shares with English), DON'T force a different value — instead
record a @key.sameAsSourceJustification in YOUR locale catalog: a short, sourced, non-empty reason it's deliberately
identical. This silences the desktop-i18n-coverage "possibly untranslated" warning for that key while keeping it honest.
The bar is the same as a translation: only justify what you can defend from the reference pile / glossary. If a key
actually needs translating, translate it. It's per-locale (German keeps "Server"; Spanish writes "Servidor"), repeated
per locale even for universal brands, and silences only the IDENTICAL signal (never a MISSING key). Full rules:
docs/guides/i18n-translation.md § Deliberately-identical strings.

TERM CHOICE (three principles):
1. APPLE FEATURE NAMES — localize what Apple localizes. Some Apple feature names are translated per-OS (Quick Look ->
   "Coup d'oeil"/"Übersicht"/"Vista rápida"); others Apple keeps English everywhere (Spotlight, Mission Control,
   AirDrop, Siri, Time Machine, Finder). Decide by checking <tag>/macOS/ in the pile: use the term Apple's localized
   macOS shows, so it matches the user's Finder. (That's why Quick Look is NOT in the don't-translate list.)
2. PREFER THE macOS FINDER TERM when macOS and Windows/Microsoft differ — Cmdr is a macOS app. E.g. pt-BR delete =
   "Apagar" (Finder), not "Excluir" (Windows); German move = "Bewegen" (Finder), not "Verschieben" (Microsoft).
3. BRANDS MAY INFLECT. In inflecting languages, let the brand take its natural suffix ("Cmdrben" in Hungarian, "Cmdrs"
   in Swedish) rather than an unnatural bare form. Choose the suffix by PRONUNCIATION, not spelling (vowel harmony keys
   off the spoken form). The check is suffix-aware, so an inflected brand passes.

OUTPUT: For each key, return only the translated value. Your output may ship without human review, so don't rely on a
reviewer to catch mistakes: translate only what you're confident in, and flag any string where the context was
insufficient rather than guessing. A flag is recorded for if/when a native reviewer is available, not a guarantee one
will see it.
```

The two "uncontrolled inserts" and "fragment keys" paragraphs come from the catalog audit: they're the two highest
blind-translation risks once placeholders and structure are otherwise handled. They're encoded into the
description-quality bar (`messages/DETAILS.md`), so a well-described key already flags both, but stating them once in
the prompt makes the agent defensive by default.
