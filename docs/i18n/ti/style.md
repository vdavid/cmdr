# Tigrinya (ti) translation style guide

Working notes for translating Cmdr into Tigrinya. Read `../README.md` for how this fits the translation process, and
`docs/guides/i18n-translation.md` for the agent-handoff block and ICU rules.

## Priority signal (read first)

Tigrinya is a very-low-priority, very-thin-evidence locale. The reference pile has Microsoft terminology ONLY: no Apple
(Apple ships no Tigrinya UI), no GNOME Nautilus, no Xfce Thunar. So there is no native file-manager catalog to
triangulate against; almost every file-manager term rests on a single source (Microsoft) or on translator judgment.
Treat the whole glossary as `tentative` until a native Tigrinya reviewer signs off. Don't ship this locale without that
review (principle 6 matters more here than anywhere). Microsoft's catalog targets Tigrinya-Ethiopia (`ti` tagged
`geographicalUsage=ETH`), so it carries an Ethiopia tilt we partly inherit (see Regional variant below).

## Voice and tone

- Match Cmdr's English voice: friendly, concise, active, calm, never alarmist. Error messages stay calm and actionable
  and avoid words meaning "error" or "failed" (Microsoft renders "error" as ጌጋ; structure messages to describe what
  happened and the next step instead of leading with it).
- Tigrinya tech writing leans on a mix of native Ge'ez-script coinages and English loanwords rendered in Ge'ez script.
  Keep it natural and unfussy, not bureaucratic or heavily Amharic-influenced.
- This is a draft register: the goal is clear, plain Tigrinya a Tigray/Eritrean user reads without friction, not
  scholarly or liturgical phrasing.

## Formality

- Tigrinya distinguishes familiar and respectful (honorific) second-person address, and the respectful form is built
  with plural verb agreement. Software convention (per Microsoft's Tigrinya UI) addresses the user with the plain
  second-person form, NOT honorific-plural, to keep buttons and labels short. Use the plain form by default.
- UI actions (buttons, menu items, commands) are imperatives. The default imperative is masculine singular, the unmarked
  UI form (see the gendered-address decision point below). Evidence: Microsoft uses masculine-singular imperatives
  throughout (open ክፈት, cut ቁረጽ, select ምረጽ, paste ለጥፍ, refresh ኣሐድስ).
- Confirm with David: plain vs honorific register, and the masculine-singular default, are the two formality calls a
  native reviewer should ratify.

## Decision points

### Script: Ge'ez / Ethiopic abugida (the #1 thing)

- Tigrinya is written in the Ge'ez (Ethiopic) script, an abugida (each glyph is a consonant + vowel syllable), written
  left-to-right (NOT RTL, NOT Latin). Unicode block U+1200–U+137F plus extensions.
- The abugida has NO case: there is no uppercase/lowercase. So any English-side casing convention (Title Case, ALL CAPS)
  has no Tigrinya equivalent. Don't try to reproduce emphasis through casing. Sentence-case guidance from the English
  style guide is a non-issue here; translate the words, drop the casing concept.
- Rendering and fonts: the script needs a font with full Ethiopic coverage to avoid tofu (missing-glyph boxes). macOS
  ships Ethiopic support system-wide (Kefa font, since macOS 10.6, 2010), so Cmdr inherits correct rendering on macOS
  without bundling a font; Noto Sans Ethiopic is the canonical free fallback if one is ever needed (verified via web
  research, 2026-06-20). Since Cmdr respects the system font, verify Ge'ez glyphs render in Cmdr's actual UI font during
  the overflow check, not only in a browser.
- Text length/width: Ethiopic glyphs are syllabic, so a word is often FEWER characters than its English source but each
  glyph is visually denser and may need more line height and a slightly larger effective cap height. Don't assume the
  English pixel width. Run the layout/overflow check (pseudolocale `en-XA` is the long stand-in) and watch for vertical
  clipping as much as horizontal.
- Recommendation: Ge'ez script, LTR, system font on macOS, no bundled font. Confidence: high.

### Gendered and numbered second-person address (the central grammar trap)

- Tigrinya is Semitic: verbs agree with the subject's gender AND number, including the second person. The imperative
  (the mood every UI action button uses) has four distinct forms: masculine singular, feminine singular, masculine
  plural, feminine plural (for example "open" differs by addressee gender and number). So "addressing the user" forces a
  gender-and-number choice that English never makes.
- Cmdr doesn't know the user's gender, so any single imperative form mis-genders some users. The pragmatic UI convention
  (Microsoft Tigrinya) is to use the masculine-singular imperative as the neutral default (open ክፈት, cut ቁረጽ, select
  ምረጽ). This is the same compromise other gendered languages make for software, not an ideal but the established norm.
- A genuinely gender-neutral alternative exists: rephrase actions as verbal nouns / infinitives instead of imperatives
  (for example "copy" as the action-noun ቅዳሕ rather than the command "copy!"). Microsoft itself does this for some menu
  labels (copy ቅዳሕ, search ምድላይ, edit ምዕራይ are noun/infinitive forms, not imperatives). Verbal-noun labels sidestep the
  gender choice entirely and read fine on buttons and menus.
- Recommendation: prefer the verbal-noun / infinitive label form where it reads naturally (it's gender-neutral and
  matches some Microsoft menu labels); fall back to the masculine-singular imperative only where a true command tone is
  needed. Confidence: tentative. This is a David + native-reviewer call: pick ONE consistent strategy across the catalog
  (all verbal-noun, or imperative-with-masculine-default) rather than mixing ad hoc. Flag for David.
- Never inflect a button label based on a runtime-detected user gender: Cmdr has no such signal, and faking one is worse
  than the neutral default.

### Regional variant: ti-ER (Eritrea) vs ti-ET (Ethiopia/Tigray)

- Tigrinya is spoken in Eritrea and in Ethiopia's Tigray region. Differences are minor (some vocabulary, loanword
  sources, a few spelling habits), not mutually unintelligible. Both write Ge'ez, both CLDR-plural identically.
- The only evidence source (Microsoft) targets Ethiopia (`geographicalUsage=ETH`). CLDR/ICU recognize `ti`, `ti-ER`, and
  `ti-ET`.
- Recommendation: target the base tag `ti` (per the project's base-preferred tag convention), drafting from the
  Ethiopia-tilted Microsoft evidence but keeping vocabulary as pan-Tigrinya / neutral as possible. Split to `ti-ER` /
  `ti-ET` only if a reviewer flags real divergence. Confidence: high for "use base `ti`"; the ER/ET vocabulary nuances
  are a reviewer call. Flag for David only if a reviewer wants a regional split.

### Numerals: Western digits, not Ge'ez numerals

- Ge'ez has its own numerals (፩ ፪ ፫ …), but they are non-positional (no zero, no decimal) and in modern Tigrinya are
  confined to religious, cultural, and some page-numbering contexts. Western Arabic digits (0–9) dominate everywhere in
  software, commerce, phone numbers, and general math (verified via web research, 2026-06-20; CLDR default digits for
  `ti` are Western).
- Recommendation: Western digits (0–9) for ALL counts, sizes, dates, and progress in the UI. Don't use Ge'ez numerals.
  Let `Intl`/CLDR format numbers; don't hand-localize digits. Confidence: high.

### Punctuation: Western default, with Ethiopic stop optional

- Ethiopic traditionally uses ፣ (comma), ። (full stop / four-dot), and a wordspace (፡) between words, but modern digital
  Tigrinya commonly uses the plain space between words and frequently uses Western punctuation, especially in software
  UI.
- Recommendation: use the plain ASCII space between words (matches modern digital norm and Cmdr's existing strings), and
  Western punctuation (`.`, `,`, `:`) by default for consistency with the rest of the catalog and with
  placeholder-bearing strings. The Ethiopic full stop ። and comma ፣ are acceptable in running sentence text if a native
  reviewer prefers a more native feel, but keep them OUT of strings that interpolate paths/counts to avoid spacing
  surprises. Confidence: tentative for the native-feel sentence text (reviewer call); high for "Western default in
  interpolated strings". Flag the native-punctuation preference for the reviewer.

## Terminology and glossary

Evidence is Microsoft-only, so every entry is at best `high` (one source) and realistically `tentative` until a native
reviewer confirms. Pattern observed: tech nouns split between Ge'ez-script transliterations of English (file ፋይል, disk
ዲስክ, program/app ፕሮግራም) and native coinages (folder ሓቛፊ, window መስኮት, server ኣገልጋሊ). Verbs are mostly native.

Watch two false-friend traps from Microsoft's general (non-file-manager) corpus:

- "size" maps to መጠን, but መጠን also came up as the translation of audio "volume" (volume መጠን ድምጺ = "amount of sound").
  For a file manager, a storage "volume" (a mounted disk) is a DIFFERENT sense; don't reuse the audio-volume term.
  Resolve the storage-volume term with the reviewer.
- "tab" returned ጽላት (a sheet/leaf sense). Confirm this reads as a UI tab, not a paper sheet, before using it.

| English term                                      | Tigrinya    | Notes                                                                       |
| ------------------------------------------------- | ----------- | --------------------------------------------------------------------------- |
| file                                              | ፋይል         | Microsoft; loanword in Ge'ez script. high                                   |
| folder                                            | ሓቛፊ         | Microsoft; native coinage. high                                             |
| open                                              | ክፈት         | Microsoft; masc-sg imperative (see gender decision). tentative              |
| copy                                              | ቅዳሕ         | Microsoft; verbal-noun form, gender-neutral. tentative                      |
| cut                                               | ቁረጽ         | Microsoft; masc-sg imperative. tentative                                    |
| paste                                             | ለጥፍ         | Microsoft; masc-sg imperative. tentative                                    |
| delete                                            | ሰርዝ         | Microsoft. tentative                                                        |
| move                                              | ኣንቀሳቕስ      | Microsoft. tentative                                                        |
| rename                                            | (none)      | Not in Microsoft pile; needs reviewer. tentative                            |
| cancel                                            | ኣይትምረጽ      | Microsoft (lit. "don't select"); confirm fit for a Cancel button. tentative |
| save                                              | ኣቐምጥ        | Microsoft. tentative                                                        |
| name                                              | ሽም          | Microsoft. high                                                             |
| size                                              | መጠን         | Microsoft; see false-friend note vs audio volume. tentative                 |
| type                                              | ዓይነት        | Microsoft. high                                                             |
| search                                            | ምድላይ        | Microsoft; verbal-noun. high                                                |
| settings                                          | ዓውደ-ኣሳልጦ    | Microsoft. tentative                                                        |
| options                                           | ኣማራጽታት      | Microsoft. high                                                             |
| window                                            | መስኮት        | Microsoft. high                                                             |
| view                                              | ትርኢት        | Microsoft. high                                                             |
| properties                                        | ባህርያት       | Microsoft. high                                                             |
| network                                           | ዝተሓላለኸ መርበብ | Microsoft. tentative                                                        |
| server                                            | ኣገልጋሊ       | Microsoft. high                                                             |
| connect                                           | ኣላግብ / ኣራኽብ | Microsoft (two forms seen); reviewer picks one. tentative                   |
| download                                          | ኣውርድ        | Microsoft. tentative                                                        |
| upload                                            | ስቐል         | Microsoft. tentative                                                        |
| refresh                                           | ኣሐድስ        | Microsoft. tentative                                                        |
| sort                                              | ጎጅል         | Microsoft. tentative                                                        |
| filter                                            | ሚሐ          | Microsoft. tentative                                                        |
| help                                              | ሓገዝ         | Microsoft. high                                                             |
| trash                                             | (none)      | Not in Microsoft pile; the file-manager Trash needs reviewer. tentative     |
| pane / tab / volume / listing / transfer / viewer | (none)      | Cmdr-specific UI nouns absent from the pile; reviewer-defined. tentative    |

## Brand and do-not-translate

Keep verbatim (product/platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte,
Quick Look, plus the `{system_settings}`-style tokens. Enforced by `desktop-i18n-dont-translate` (curated list:
`apps/desktop/scripts/i18n-catalog-lib.ts`). These stay in Latin script inside Ge'ez-script sentences; that mixing is
expected and correct.

## Plurals

- CLDR plural categories for `ti`: `one`, `other` (confirmed via
  `new Intl.PluralRules('ti').resolvedOptions().pluralCategories`, 2026-06-20). `ti-ER` and `ti-ET` resolve the same.
- Every ICU plural message must cover BOTH `one` and `other` (the `desktop-i18n-plural` check enforces it).
- Grammar note: a counted noun in Tigrinya can take a plural form and the count interacts with gender agreement on
  surrounding verbs/adjectives. Write the `one` and `other` branches as full natural phrases (don't assemble "number +
  bare noun" and hope it agrees). Keep counts as Western digits in the `{count}` placeholder.

## Notes and decisions

### Decisions to confirm with David / native reviewer

- Address strategy (THE big one): one consistent choice across the whole catalog between (a) verbal-noun / infinitive
  labels (gender-neutral, matches several Microsoft menu labels) and (b) masculine-singular imperatives (Microsoft's
  button default). Don't mix per-string. Tentative.
- Plain vs honorific-plural second-person register (recommend plain). Tentative.
- ti vs ti-ER vs ti-ET targeting (recommend base `ti` from Ethiopia-tilted evidence, kept pan-Tigrinya). High for using
  base `ti`; regional split only on reviewer request.
- Native Ethiopic punctuation (። ፣) in running sentence text vs all-Western (recommend Western default; native stop
  optional in sentence text, never in interpolated strings). Tentative.
- The whole glossary: Microsoft-only single-source, plus the storage-"volume" / audio-"volume" false friend and the
  "tab" sense, all need native confirmation. Tentative.

### ICU mechanics (catalog-level, not Tigrinya-specific, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`; ICU treats a lone `'` as an escape and silently swallows text).
  Ge'ez script rarely uses the ASCII apostrophe, but loanword transliterations and English brand fragments can, so watch
  for it.
- Keep every `{placeholder}` and `<tag>` verbatim; translate only the human-readable text between them. Full rules: the
  agent-handoff block in `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ti/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
