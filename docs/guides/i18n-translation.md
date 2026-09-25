# Translating Cmdr (the translator process)

The translator-facing companion to `i18n.md`. `i18n.md` is the developer map: how the catalog, runtime, and checks work.
THIS guide is the process you follow to add a language or translate new strings, plus the agent handoff (the standing
instructions themselves live in `docs/i18n/translator-instructions.md`, embedded in every brief). Mechanism lives in
`i18n.md` and the colocated docs; this guide points to it, never restates it.

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
2. **Per-language style guide and termbase**: tone, voice, formality (T/V distinction if the language has one), how
   brand words are handled in this language, and the ruling for each recurring term. NOT per-string; never repeat tone
   on every key. The style guide lives at `docs/i18n/<tag>/style.md` and opens with a `## Digest` of its must-know
   rules; the terms live in `docs/i18n/<tag>/terms.json`, keyed by the shared concepts in `docs/i18n/concepts.json`,
   with the rationale in `decisions.md` beside them. Schema and tools: `docs/i18n/termbase.md`. A translator reads none
   of these whole up front except `style.md`: `pnpm i18n:brief` assembles the digest, the rulings in play, the nearest
   shipped translations, and the relevant decisions for one batch (see § The translator-agent context). These are
   working notes, not catalog data: the app never loads them. **Treat them as living docs, and capturing is part of the
   job**: record each term you settle in `terms.json` with its sources and a confidence (see Researching terms below).
   This isn't only for terms: whenever you hit a convention, gotcha, decision point, or rule that wasn't already written
   where you looked for it, write it down so the next translator inherits it instead of rediscovering it. Per-language
   findings go in the style guide; a missing cross-language rule (like an ICU mechanic) goes to
   `docs/i18n/source-queue.md` as a proposal, and the lead promotes it into the shared docs. **Every message key you
   cite is verified**: `pnpm check i18n-citations` (`desktop-i18n-doc-citations`) requires each backticked dotted token
   in a guide or a termbase file whose first segment is a real catalog namespace to name a real English key, and it
   fails the build when one doesn't. So a rename that orphans your evidence surfaces at once instead of talking the next
   translator out of a correct fix. Naming a key that's genuinely gone (recording where a carried-over translation came
   from) needs a reasoned entry in `scripts/check/checks/desktop-i18n-doc-citations-allowlist.json`.
3. **One ICU instruction**: given once in the agent system prompt, not per string (see the block below).

## Term-choice principles

Three settled decisions about WHICH term to pick. They apply to every language; the don't-translate check enforces what
it can, but the judgment is yours.

1. **Apple feature names — localize what Apple localizes.** Some Apple feature names are translated per-OS (Quick Look →
   fr "Coup d’œil", de "Übersicht", es "Vista rápida"); others Apple keeps English in every locale (Spotlight, Mission
   Control, AirDrop, Siri, Time Machine, Finder). To decide, check `<tag>/macOS/` in the reference pile: if Apple's
   localized macOS uses a translated term, use it; if it keeps the English name, keep it. Match what the user actually
   sees in their Finder. (This is why `BRAND_WORDS` in `apps/desktop/scripts/i18n-catalog-lib.ts` lists the kept-English
   names but NOT Quick Look — a translated Quick Look must not read as a dropped brand.) It's decided per LABEL, not per
   app: Apple keeps `Finder` and `Terminal` English almost everywhere yet localizes `Get Info`, `Locked`, and
   `Sharing & Permissions` in every locale. A `@key.description` telling you to keep such a label English is not
   authority; verify it against the OS, and fix the `en` description when it's wrong. The direct key-match recipe
   against the shipped Finder bundle, plus the caveat that these labels follow the SYSTEM language while our catalog
   follows the APP language, are in `../i18n/translation-learnings.md` § "Quoting a macOS UI label".
2. **Prefer the macOS Finder term when macOS and Windows/Microsoft differ.** Cmdr is a macOS app, so the native-OS term
   wins over the Windows convention. For example, pt-BR delete = "Apagar" (Finder), not "Excluir" (Windows); German move
   = "Bewegen" (Finder), not "Verschieben" (Microsoft). The Microsoft terminology entry is the Windows wording, not ours
   — use it only as a tiebreak below macOS. (Same shape as the formality trap in `../i18n/reference-pile/how-to-mine.md`
   § "Source-quality traps", trap 5.)
3. **Brand/product names may inflect.** In agglutinative/inflecting languages, let the brand take its natural
   inflectional suffix rather than forcing an unnatural bare form: Hungarian "Cmdrben" (in Cmdr), Swedish genitive
   "Cmdrs". Keep the brand recognizable but grammatical. Pick the suffix by how the name is PRONOUNCED, not spelled:
   vowel harmony in Hungarian/Finnish/Turkish keys off the SPOKEN form, so "Cmdr" read aloud as "commander" harmonizes
   to the vowels you'd hear, not to the bare consonant cluster. Apply your language's harmony rules to the
   pronunciation. The don't-translate check is suffix-aware (`hasBrandPresent`), so an inflected brand passes; an
   omitted one is still flagged.

## Researching terms: the reference pile

A term with a ruling in `terms.json` is settled: use it, and reopen it only with new evidence (then edit the ruling in
place and move the old form to `avoid`). For every term WITHOUT a ruling (the brief marks it "no ruling", or it isn't a
registered concept yet), checking the reference pile is MANDATORY: mine it for the term and for similar sentences, reuse
and cite, never guess. The reference pile holds authoritative localizations keyed by language: the ~3 GB of macOS,
Microsoft, and five file managers — the explorer family (GNOME Nautilus, Xfce Thunar, KDE Dolphin) plus the orthodox
two-pane pair (Total Commander, Double Commander) — one folder per language. Read `../i18n/reference-pile/README.md` for
what's there and the authority tiers, and `../i18n/reference-pile/how-to-mine.md` for tested per-source recipes (greps,
jq, `msggrep`, `pdftotext`, `.lng`).

> [!IMPORTANT] **Where the pile is — and why a worktree can't see it.** The pile is gitignored (`_ignored/` is
> untracked), so it lives ONLY in the main clone, at **`~/projects-git/vdavid/cmdr/_ignored/i18n/<tag>/`**. It is NOT
> copied into git worktrees. Translation almost always runs from a worktree (`.claude/worktrees/<slug>/`), and there a
> relative `_ignored/i18n/` **does not exist** — checking the worktree-relative path returns "absent" and tempts you to
> translate without the pile (guessing, which this guide forbids). So ALWAYS use the absolute main-clone path above. If
> you can't assume the path, resolve the main clone with `git worktree list | head -1` (its first column is the main
> checkout) and mine `<that path>/_ignored/i18n/<tag>/`. A "no reference pile present" conclusion is almost always this
> worktree trap, not a genuinely missing pile — re-check the main-clone absolute path before deciding it's gone.

For each term or convention: triangulate across every source the language has, pick the most native-sounding fit for
Cmdr's voice, then record it as a `terms.json` entry (`chosen`, `sources`, `confidence`, plus `accept` / `forms` /
`avoid` as needed; a concept with no `concepts.json` entry gets one first). Weight by authority: macOS first, then
Microsoft, then the file-manager corpora (community-translated, so below the first-party vendors for general terms).
Confidence is `confirmed` (a human signed off), `high` (authoritative sources agree), or `tentative` (sources conflict
or none had it). A choice only David can make goes in the style guide's decisions-to-confirm section, and a doubt only a
native reviewer can settle goes in `review-queue.md`, rather than burying either — but for Hungarian, resolve by
evidence and don't park it for David (see § Treat every language the same).

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

## An `*Aria` key must contain its visible label (WCAG 2.5.3)

When a `*Aria` key is the accessible name of a control whose visible label is another key (`…queue` / `…queueAria`,
`…background` / `…backgroundAria`), the accessible name has to CONTAIN the visible label's words, verbatim and in order.
Voice-control users say what they can see, so a name that paraphrases the label leaves them unable to press the button.
Case may differ (English itself only manages "Background" ⊂ "Keep this running in the background"); word order and
wording may not.

Inflection is what breaks this, silently and per-language, which is why it's stated here once rather than rediscovered
nine times:

- A German label wants a case the aria sentence doesn't (`In den Hintergrund` is not inside `… im Hintergrund …`).
- A Hungarian/Finnish/Turkish case suffix does the same (`Háttérbe` vs `a háttérben`), and a coincidental PREFIX match
  doesn't count: whole-word voice matching won't honor it.
- A Swedish/Danish/Norwegian definite form does it too (`Bakgrund` vs `i bakgrunden`).

**The fix is always the same: pick the LABEL's form to be the one the natural aria sentence already uses, then cut the
label out of that sentence** (ideally as its opening words, the shape that survives later rewording). Don't bend the
aria around an awkward label. Two consequences worth knowing: the pair is ONE unit, so re-wording the label alone can
break containment, and a later "make these consistent" pass over the aria can break it too, so record the constraint in
the `note` of the term's `terms.json` entry, naming both keys.

`desktop-i18n-aria-label` (`pnpm check i18n-aria`) enforces it. A pair counts only when English's own `fooAria` already
contains `foo`, so there's nothing to allowlist: if it fires, the translation genuinely broke it. It's warn-only while
`de`, `es`, `pt`, and `zh` still carry known breaks.

## Deliberately-identical strings (the `sameAsSourceJustification` field)

Some keys are CORRECTLY identical to English in your language and must never be force-translated: a brand name
(`Dropbox`, `OneDrive`), a unit symbol (`GB`, `kB`), a standard name (`ISO 8601`, `PDF`, `Unicode`), a placeholder-only
string (`{width} × {height}`, `{systemSettings} > {appearance}`), or a real word the language genuinely shares with
English (German `Server`, French `Type`, Swedish `Smart`). Translating these would be a regression, not an improvement.

The `desktop-i18n-coverage` check flags every still-English value as "possibly untranslated", and it sees past the
plural shape: a counter whose branch set is your language's own but whose branch text is English
(`one {token} many {tokens} other {tokens}}`) is flagged like a byte-identical value, because the reader gets English
either way. To keep an honest, clean coverage signal — every warning is a real gap, every deliberate identical is
silenced WITH a recorded reason — record a `@key.sameAsSourceJustification` on that key in YOUR locale catalog: a short,
non-empty string saying why it's deliberately identical, sourced like any other term decision. Example, in
`messages/de/errors.json`:

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
- The bar is the SAME as a translation: only record a justification you can defend from the reference pile / termbase.
  "I couldn't be bothered" is not a justification. If a key actually needs translating, translate it — the field is for
  genuinely-identical strings only, and the goal is a clean coverage warn output WITHOUT lowering the quality bar.
- It does NOT apply to an OVERLAY catalog (`en-GB`, `pt-PT`). There, a value identical to what it overrides is dead
  weight and the fix is always to delete the key, so the field is ignored: `i18n.md` § Overlay catalogs.

Mechanism + schema: `apps/desktop/src/lib/intl/messages/DETAILS.md` § `@key` metadata schema.

## Add a new language

1. **Pick the BCP-47 tag.** A language base (`xx`) for the universal set, or a region variant (`xx-YY`) when a region
   needs overrides. The tag is a format identifier, not translatable. The base is the fallback for its variants; `en` is
   the final fallback. Convention + resolution order: `i18n.md` § Locale-format convention. If the base language already
   ships, you're writing an OVERLAY, not a translation: skip to the next section.
2. **Create the skeleton.** Run `node apps/desktop/scripts/gen-locale-skeleton.ts <tag>`: it mirrors `en/`'s files and
   keys under `messages/<tag>/` with the English values in place and each `@key.sourceHash` = the 7-char hash of the
   exact English value it was translated from (computed by `sourceHash()` in `apps/desktop/scripts/i18n-catalog-lib.ts`;
   the pseudolocale generator does exactly this and is the reference). The hash is what `desktop-i18n-stale` uses to
   know a translation is still current.
3. **Write the per-language style guide and start the termbase** (input 2 above): copy `docs/i18n/_template/` to
   `docs/i18n/<tag>/` (`style.md` with its `## Digest`, an empty `terms.json`, a `mechanics.json` stub, and the
   `decisions.md` and `review-queue.md` stubs), fill the style guide, and declare the language's typography in
   `mechanics.json` (quote pairs, apostrophes, required spacing, the hedges its grammar invites; schema in
   `docs/i18n/termbase.md`).
4. **Translate** with a translator agent (§ The translator-agent context), in batches, each from a
   `pnpm i18n:brief --lang <tag>` brief. The first batches will mostly say "no ruling": each term you settle becomes a
   `terms.json` entry the next batch inherits.
5. **Run the checks**:
   `pnpm check i18n-parity i18n-icu i18n-plural i18n-stale i18n-coverage i18n-dont-translate i18n-quoted-labels i18n-aria i18n-terms i18n-termbase i18n-mechanics i18n-citations`.
   Parity (placeholder/tag/token sets), ICU validity, plural coverage, translation coverage, aria containment,
   citations, and the termbase and mechanics schemas are ERROR class, so a locale can't ship half-translated; stale,
   don't-translate, term consistency, termbase drift, `decisions.md` growth, and typography findings are WARN class.
   What each catches: `i18n.md` § Enforcement (the termbase and mechanics checks: `docs/i18n/termbase.md` § Tooling).
6. **Overflow-check the layout.** Drive the app and look for clipping; the pseudolocale (`en-XA`) is the deliberately
   long stand-in for this. See `i18n.md` § Pseudolocale.
7. **Human review (optional, not a ship gate).** If a native reviewer is available, set `@key.reviewed: true` per key as
   they sign it off; the stale check clears it whenever the source changes, so review state stays honest. Skipping this
   is the normal case — see the override above.
8. **Ship.** No code change is needed to make a finished locale live: the runtime resolver and the in-app picker
   (**Settings > Appearance > Language**) are built, so dropping a `messages/<tag>/` dir makes the locale load and
   appear in the picker, with the documented `<tag>` → base → `en` fallback per key. A locale ships once it's
   translated, passes the checks, and is overflow-checked — human review is opportunistic, not a gate (see the override
   above). The runtime mechanism is in `i18n.md` § "Add a new locale".

## Add a regional variant (an overlay)

A variant whose language base already ships (`en-GB` over `en`, `pt-PT` over `pt`) is an **overlay**: it holds ONLY the
strings that genuinely differ in that region, and every other key keeps rendering the base language's translation. So
it's a much smaller job than a language, and a different one.

One thing to settle first: a variant in a DIFFERENT SCRIPT is not an overlay. Traditional Chinese (`zh-Hant`, or a
region that implies it like `zh-TW`) inherits nothing from the Simplified `zh` catalog, because a reader who needs
Traditional can't read Simplified. It's a full translation, and everything it doesn't carry renders in English. Same for
any of the nine script-split languages in `docs/i18n/script-decisions.md`. If you're unsure which you're writing, run
`pnpm check desktop-i18n-coverage`: it tells you which contract it's holding your catalog to.

1. **List what actually differs.** For `en-GB`: spelling (`colour`, `favourite`, `organise`), terminology (macOS calls
   the Trash the "Bin" in the UK), and date/measure phrasing that isn't already handled by the formatter layer. Evidence
   first, from the same reference pile as any term decision: if you can't source it, it isn't a difference. Legal and
   licensing copy needs a different source than the `.strings` catalogs: `docs/i18n/reference-pile/how-to-mine.md` §
   Legal register.
2. **Write only those keys**, in the area files they belong to, following the layout and `@key` rules in
   `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
3. **Stamp `@key.sourceHash` from the value you override**, which is the base language's value (`pt` for `pt-PT`), not
   the English one. That's what makes a later copy edit in the base mark your fork stale.
4. **Run the same checks** as step 5 above. Coverage is the honest signal here in reverse: it lists every key that
   matches what it overrides, meaning it forks nothing and should be deleted.

**Write the overlay's style guide as you go**, at `docs/i18n/<tag>/`. For an overlay it's a record of WHAT FORKS AND
WHY, not tone or formality, and it must also list the forks you considered and deliberately SKIPPED with their evidence,
or the next contributor re-litigates them. `docs/i18n/en-GB/style.md` is the worked example, and
`docs/i18n/en-AU/style.md` shows a second overlay pointing at a sibling instead of restating it.

Don't record a `sameAsSourceJustification` on an overlay key. `gen-locale-skeleton.ts` refuses an overlay tag outright,
since it mirrors the entire `en` catalog, and `sync-locale-keys.ts` skips overlays for the same reason: a new English
key needs no work here, the variant inherits the base language's translation. Full rules, and what each check does:
`i18n.md` § Overlay catalogs.

## New feature → add strings and translate to ALL languages

The routine maintenance loop, run for every change that adds or edits user-facing copy:

1. **Add or edit the `en` key** with a `@key.description` that meets the bar (`messages/DETAILS.md` § `@key` metadata
   schema, noting the fragment-key and pass-through-placeholder requirements). Run `pnpm intl:keys` to regenerate the
   key union.
2. **Propagate the keys to every locale**: run `node apps/desktop/scripts/sync-locale-keys.ts` (all locales) — it adds
   each new `en` key as an English skeleton with the correct `@key.sourceHash`, drops keys you removed, and preserves
   existing translations. It moves KEYS only: a kept key's `@key` block, `sourceHash` included, is never rewritten, so
   syncing can't clear a staleness warning that nobody has answered. Then hand each locale's new/changed keys to a
   translator agent with a brief: `pnpm i18n:brief --lang <tag> --changed-since <ref>` (or `--keys` for the feature's
   namespace). The coverage check lists exactly what's still English.
3. **Run the checks** (same set as step 5 above). `desktop-i18n-stale` is the safety net here: editing an `en` value
   changes its hash, so EVERY locale's translation of that key reads as stale until re-translated and re-hashed. You
   can't silently leave a locale behind on a copy edit: the stale warning lists exactly which keys each locale owes.
   When you re-translate a key, update its `@key.sourceHash` in the same edit. **When an `en` edit doesn't change the
   meaning** ("is larger" → "may be larger", where every locale already used a modal form), the translations are still
   accurate and only the stored hash needs to catch up. Refresh it with
   `node apps/desktop/scripts/sync-locale-keys.ts --restamp <key>` (repeatable, applies across every locale). Check each
   locale's value first: this is the one command that clears a warning without a translation changing, so it belongs in
   its own commit with the per-locale reasoning in the message. It drops `reviewed` and `sameAsSourceJustification` on
   the keys it touches: both vouched for the old English and have to be re-earned.
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

## Auditing a finished locale for term drift

A locale translated over several passes drifts: each pass settles the terms it personally needed, so the same English
word ends up with two names and the app contradicts itself. Traditional Chinese shipped a menu item reading
`命令選擇區…` that opened a palette titled `指令面板`. Run this audit when a locale is finished, and again whenever
several passes have landed since the last one. **Doing it by reading the catalog does not work**: it's 3,263 keys.
Script it.

**Half of it is automated.** `pnpm check i18n-terms` (`desktop-i18n-term-consistency`) catches every case where two keys
share one English string and the locale renders them differently, and for an overlay it catches a half-fork. Fix what it
reports, or record the split with its reason in `apps/desktop/scripts/i18n-term-consistency-allowlist.json`. See
`i18n.md` § Term consistency.

**The other half is manual, because no check can do it.** The drift that hurts most is a term rendered two ways across
keys whose ENGLISH differs ("Show thumbnails" vs "Thumbnail size"), and deciding which English words are "terms" is
judgment. Three passes, each a short script over `loadCatalog('en')` and `loadCatalog('<tag>')`:

1. **Align short labels.** Restrict to keys whose English value is ≤ ~40 characters (labels and buttons, where a term
   maps almost 1:1 onto a translated substring; long prose legitimately paraphrases). For each English word appearing in
   ≥3 such keys, collect the substrings of the translations that correlate with it, and flag any word where two
   different substrings each cover a distinct set of keys. This is what surfaced Traditional Chinese rendering "list" as
   both `清單` and `列表`, and "operation" as `操作` everywhere except `errors.json`, which said `作業`.
2. **Probe in reverse: automated for every ruled term.** `pnpm check i18n-termbase` (`desktop-i18n-termbase`) lists
   every key whose English matches a concept while its translation carries none of the ruling's forms and isn't in the
   term's `exceptions` (`pnpm i18n:check-termbase --list` in `apps/desktop` prints them all, even under the baseline).
   Each one is either drift or a ruling that has gone stale. This is the probe that caught `theme → 佈景主題`: the
   ruling claimed it, the catalog used `主題` in all eight keys. For a term with no ruling yet, do the same probe by
   hand, then write the ruling.
3. **Probe the near-miss pairs.** For any two terms a translator could confuse, list both and read the English beside
   them. Traditional Chinese needed `複製` (copy) vs `製作副本` (duplicate), `還原` (undo) vs `復原` (roll back), `標籤`
   (tag) vs `分頁` (tab), and `金鑰` (an API key) vs `授權碼` (a licence key). Two of those had already gone wrong.

**Rank by how visible the finding is, and fix in that order.** A term in `menu.json` or `commands.json` is read on every
session and sits next to the dialog it opens; one in `errors.json` may never be seen. A divergence spanning two
high-traffic files is the worst case and the one to fix first.

**Every fix earns a termbase entry.** A conflict you resolved without recording it will be re-litigated by the next
pass, which is how these got here. Write or update the `terms.json` ruling, and when the two forms are BOTH right, say
where the boundary runs: split the concept (`browse-file-picker` vs `browse-archive`, linked by `distinct`), or list the
keys that legitimately differ in the term's `exceptions` with the reason. That sentence is the thing that stops the next
translator "fixing" it back.

## The translator-agent context

**Who translates, and in what shape.** Never the coding agent that added the strings: it writes English and the `@key`
metadata, then a separate translator agent takes over. The lever is that the translator actually reads the language's
style guide and the rulings for the terms in play; every agent already knows every language. `pnpm i18n:brief` (run in
`apps/desktop`) assembles exactly that for one batch, AND the translator's standing instructions (ICU, raw families,
aria, gender, write-back) rendered for the language, AND the shared principles (voice, names vs prose, no hedged
grammar, native typography, escalating source problems). Those live in `docs/i18n/translator-instructions.md` and
`docs/i18n/translation-principles.md` and nowhere else, so the brief is the one document an agent needs besides the full
`style.md`. Flags, sections, and the blind-run mode: `docs/i18n/termbase.md` § Tooling.

**Closing the loop.** Every translator report ends with a Source feedback section (weak descriptions, missing
screenshots, ambiguous English, proposed cross-language rules), and each item also lands in `docs/i18n/source-queue.md`.
The lead resolves the queue with David (fixing the English, the description, or the shared docs) and deletes each entry
once it's resolved. A locale's own files never carry a workaround for an English problem.

Pick the shape by batch size. Sizes are `--stats` on the migrated `nl` termbase, 2026-09-24.

- **A real batch** (a feature's strings, a review pass): one agent per language, each handed
  `pnpm i18n:brief --lang <tag> --keys <…> --out <file>` (or `--keys-file`, one key per line) and reading the full
  `style.md` beside it (~10k tokens for `nl`). The A/B batch of 30 mixed keys made a ~15k-token brief with the
  instructions and digest in it (keys ~4.6k, terms ~5k, translation memory ~2.7k, instructions ~1.2k, digest ~1.3k), so
  about 25k tokens of context before the first string.
- **One to five keys, or a small batch every language needs**: one agent can take `--lang all`. Concept senses and the
  instructions print once and each language gets a line under them; 50 `queue.*` keys ran ~30k tokens with one migrated
  locale (most of it the ten current values per key); budget ~70k once all ten have termbases. Ten full style guides
  don't fit beside it, so this agent works from the digests and opens a language's full `style.md` only when its digest
  doesn't settle a question.

The translator's loop, per batch: read the brief, then the full `docs/i18n/<tag>/style.md`, and follow the brief's
Instructions section (mine the pile for "no ruling" and "No concept yet" terms, translate, write back, run the checks).
Nothing else is required reading.

(Measured 2026-09-23: a coding agent translated four keys into ten languages inline, opened no style guide or termbase,
and passed every check anyway. The checks catch mechanics, not voice.)

**Fanning out one agent per language: if you are yourself a subagent, spawn WITHOUT the `name` parameter.** The team
roster is flat, so a named teammate can't spawn named teammates; passing `name` fails with "Teammates cannot spawn other
teammates". Only the lead session can name them. Write each language's brief to a file (`--out`) and point its agent at
the path, rather than inlining ten briefs into prompts. The whole handoff prompt is then:

```
You are translating UI strings for Cmdr, a macOS file manager, into [LANGUAGE]. Your brief is at [BRIEF PATH]: read it,
then docs/i18n/[TAG]/style.md, and follow the brief's Instructions and Principles sections. Report every string you
flagged, and end with the Source feedback section the instructions describe.
```

The instructions' "uncontrolled inserts" and "fragment keys" items come from the catalog audit: they're the two highest
blind-translation risks once placeholders and structure are otherwise handled. They're encoded into the
description-quality bar (`messages/DETAILS.md`), so a well-described key already flags both, but stating them once in
the instructions makes the agent defensive by default.
