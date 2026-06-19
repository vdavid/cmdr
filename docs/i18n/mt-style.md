# Maltese (mt) translation style guide

Working notes for translating Cmdr into Maltese. Read [`README.md`](README.md) for how this fits the translation
process. Maltese is a Semitic language written in **Latin script** with special letters (ċ, ġ, ħ, ż, and the digraphs
għ and ie); it is an EU official language. Localization depth is THIN outside Microsoft and EU institutions: Microsoft
is the primary reference (see Decision points).

## Voice and tone

Cmdr's Maltese voice mirrors its English one: friendly, concise, active, and never alarmist. Microsoft Maltese software
copy is plain and direct, so this register is the native default.

- Address the user with the informal second-person singular (int / int- verb endings), lowercase. Maltese software does
  NOT use a polite-plural register the way Slavic or Romance locales do; the singular is the universal UI default (see
  Formality). This is the opposite of the Macedonian/Slavic guides, so don't reach for a "polite plural" instinct here.
- Keep verbs in instructional sentences and in UI actions in the singular imperative (iftaħ, ikkopja, fittex), matching
  the int address.
- Stay calm and actionable in error messages, and keep the English rule of avoiding "error" and "failed". Maltese has no
  neutral one-word "failed"; rewrite around what happened and what to do (for example "Il-folder ma nstabx" = the folder
  wasn't found), not a literal "the operation failed".
- Drop English filler that carries no meaning: don't render "successfully" (state the outcome instead), and avoid "jekk
  jogħġbok" ("please") in terse UI actions, where it reads stiff.
- Sentence case only, first word and proper nouns. This aligns with Cmdr's sentence-case rule.

## Formality

- **Second person: informal singular int, always.** This is the single most important register decision, and it differs
  from the Slavic guides. Maltese has int (singular) and intom (plural), but it lacks a strongly grammaticalized T/V
  politeness split: intom is a plural, not a routine politeness register, and software does not address a single user as
  a plural. Microsoft Maltese addresses the user in the singular. Confidence: high.
- **UI commands, buttons, and menu items use the singular imperative.** iftaħ (open), ikkopja (copy), waqqaf / ħassar
  (delete), ikkanċella (cancel), ibdel l-isem (rename), fittex (search). Confidence: high.
- **Prefer neutral structures over spelling out "you/your".** Maltese expresses possession with the article or a
  suffixed pronoun and uses overt "you/your" far less than English. Don't transcribe every English "your". Confidence:
  high.

## Decision points

The genuinely tricky calls, with how the majors handle each, a recommended default, and a confidence level.

- **English/Italian/Maltese code-switching is THE core decision, and Microsoft keeps English loans with Maltese
  morphology.** Maltese natively blends Semitic roots, Italian nouns, and English loans in one sentence, and Microsoft's
  Maltese terminology leans hard into phonetically-naturalized English for the computing domain rather than coining
  Semitic neologisms:
  - file: **fajl** (English loan, Maltese spelling), not a native coinage.
  - folder: **fowlder** (English loan, Maltese spelling). Italian "cartella" is NOT used by Microsoft.
  - copy / paste / save: **ikkopja / ippejstja / issejvja** (English verbs given Maltese conjugation).
  - drive: **drajv**; volume: **volum**; destination: **destinazzjoni** (Italian-pattern loan).
  - Native Semitic verbs survive where they're well established: open **iftaħ/fetaħ**, delete **ħassar**, search
    **fittex/tiftix**, move **mexxi**, window **tieqa**, cut **aqta'**.
  Recommendation: follow Microsoft. Use the naturalized English loan where Microsoft does (fajl, fowlder, drajv,
  ikkopja, ippejstja, issejvja); use the native verb where Microsoft does (iftaħ, ħassar, fittex). Do NOT substitute
  Italian terms (cartella) or invent purist Semitic coinages: neither matches user expectations. This is the highest-
  value consistency call, so lock each term in the glossary as it comes up. Confidence: high for the pattern, medium per
  individual term (Microsoft's own terminology is not 100% consistent, e.g. "Settings" sometimes stays English).

- **Special characters ċ ġ ħ ż and the digraphs għ, ie are load-bearing, not decoration.** They are distinct letters
  that change meaning (ż vs z, ħ vs h, ċ vs k/c), so a stripped-ASCII rendering is wrong, not just ugly. The encoding/
  typing pitfall: these need a Maltese keyboard layout or compose input, and "għ" is two characters that act as one
  letter. Recommendation: always store and display the correct UTF-8 letters; never ASCII-fold (fajl is fine, but ħassar
  must keep its ħ). Verify each reviewed string visually for dropped diacritics, the most common silent corruption.
  Confidence: high.

- **Major-product availability is thin; lean on Microsoft and EU.** Apple does NOT ship a Maltese UI (Maltese is not a
  macOS/iOS system language), so there is no Apple Finder precedent to match, and no macOS "Trash" Maltese anchor.
  Microsoft DOES localize Maltese (Windows/Office, a published terminology base and style guide) and is the primary
  reference. EU institutions localize Maltese fully (it's an official EU language) but their register is legal/formal,
  not consumer-UI, so borrow terminology from them cautiously and tone never. Google is partial (Translate, some Android/
  Search surfaces) with uneven coverage. Spotify and Netflix do not ship a Maltese interface. Recommendation: treat
  Microsoft terminology as primary; don't reach for Finder or Apple conventions, and don't adopt EU legalese register.
  Confidence: high.

- **Gender and number agreement bites in dynamic strings.** Maltese nouns carry grammatical gender (masculine/feminine),
  and adjectives, the definite article's assimilation, and past-tense verbs agree with the noun's gender and number.
  This hits two places: (1) plural count messages, where the branch must agree with the counted noun (fajl is masculine,
  folder feminine) - write each plural branch as a full natural phrase, never swap only the numeral; (2) past-tense
  outcome strings ("X was moved"), where the verb ending depends on the subject noun's gender. Recommendation: write
  count and outcome strings as complete phrases per branch; dedicate one human review pass to gender agreement.
  Confidence: high.

- **Definite article assimilation ("sun letters").** The article "il-" assimilates to certain following consonants
  (id-disk, is-settings, ix-xena, ir-root) and elides before vowels (l-isem). This is mechanical Maltese spelling, but a
  translator splicing a noun after a hardcoded "il-" will get it wrong. Recommendation: write the article into the same
  branch as its noun, never assemble "il-" + {placeholder}; flag any fragment key that would force runtime article
  splicing. Confidence: high.

- **Quotation marks and punctuation.** Maltese typically follows the surrounding-European convention; Microsoft Maltese
  uses straight or curly double quotes around names and titles. There is no strong native guillemet tradition.
  Recommendation: use plain double quotes "…" around quoted names in copy unless a reviewed string establishes
  otherwise; keep punctuation calm (no English exclamation marks in messages). Confidence: medium.

## Flag for David (owner calls)

- **"pane" term.** Cmdr is pane-centric, so lock one term. Microsoft Maltese uses **kwadru** for a UI pane (it also uses
  "panew" as a naturalized loan in some entries). Recommendation: kwadru, or keep the English-loan "panew" for a more
  modern feel. Confidence: low; David picks.
- **"trash" term.** Cmdr's concept is the macOS Trash, and there is no Apple Maltese to match. Microsoft uses **Barmil**
  (literally "barrel", their Recycle Bin term). Recommendation: Barmil, or the plain English-loan "trash" if it should
  read closer to macOS. Confidence: low; David picks.
- **"tab" term.** Microsoft keeps **tab** (the English loan) for a UI tab. Recommendation: tab. Confidence: medium.
- **Overall loan-vs-native dial.** The whole catalog can lean more "naturalized English" (matches Microsoft, feels
  modern to younger users) or more "native Semitic" (feels more formal/purist). Microsoft's pile is the former.
  Recommendation: match Microsoft (naturalized English). This is a one-time tone call worth David confirming, since it
  colors every term. Confidence: medium.

## Terminology and glossary

Core terms drawn from Microsoft Maltese terminology. Extend as strings come up. Per-term confidence is medium (see the
code-switching decision point); lock each as it's used.

| English term | Maltese | Notes |
| ------------ | ------- | ----- |
| file | fajl | masculine; English loan, not Italian |
| folder | fowlder | feminine; English loan, not "cartella" |
| copy | ikkopja | imperative; English-derived verb |
| move | mexxi | imperative; native verb |
| delete | ħassar | imperative; native verb (keep the ħ) |
| paste | ippejstja | imperative; English-derived verb |
| cut | aqta' | imperative; note the closing apostrophe (a real letter mark) |
| rename | ibdel l-isem | "change the name" |
| open | iftaħ | imperative; native (keep the ħ) |
| save | issejvja | imperative; English-derived verb |
| cancel | ikkanċella | imperative (keep the ċ) |
| search | fittex | verb; the noun is "tiftix" / "tiftixa" |
| trash | Barmil | owner call; Microsoft's Recycle Bin term |
| pane | kwadru | owner call; "panew" also attested |
| tab | tab | English loan |
| window | tieqa | native |
| volume | volum | the storage-volume sense |
| drive | drajv | English loan |
| settings | settings | Microsoft often keeps English; "issettjar" is attested |
| destination folder | fowlder tad-destinazzjoni | note article assimilation: tad- |
| file name | isem tal-fajl | |
| name | isem | |
| view | dehra | the noun |

## Brand and do-not-translate

Keep these verbatim (product or platform names, not words to translate): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust,
Svelte, Quick Look. The same list (plus the system placeholder tokens) is enforced by the `desktop-i18n-dont-translate`
check; see the curated list in `apps/desktop/scripts/i18n-catalog-lib.js`. Acronyms (SMB, MTP, URL) stay Latin.

## Plurals

CLDR plural categories for `mt`: **`one`, `two`, `few`, `many`, and `other`** (FIVE categories, confirmed via
`new Intl.PluralRules('mt').resolvedOptions().pluralCategories`). This is one of the heaviest plural burdens of any
locale and a real translation cost: EVERY plural count message needs five branches, not two. The boundaries (verified
against CLDR):
- **one**: n = 1
- **two**: n = 2
- **few**: n = 0, or n mod 100 in 3..10 (so 3-10, 103-110, ...)
- **many**: n mod 100 in 11..19 (so 11-19, 111-119, ...)
- **other**: everything else (20-100 not ending in the above, 101, 102, ...)

Write each branch as a full natural phrase and mind gender agreement with the counted noun (fajl masculine, fowlder
feminine): the noun's form changes across these categories, not just the numeral, so never template only the number into
a fixed noun. The `desktop-i18n-plural` check requires every category this locale needs. Budget extra review time here:
this is the single largest correctness risk for Maltese.

## Notes and decisions

- Register: informal singular int throughout; singular imperatives for actions. NOT a polite plural (differs from the
  Slavic guides).
- Special letters ċ ġ ħ ż and digraphs għ, ie are distinct letters; never ASCII-fold. Visual diacritic check every
  reviewed string.
- Definite article "il-" assimilates to sun letters and elides before vowels; write the article inside the noun's
  branch, never splice "il-" + {placeholder}.
- Gender agreement (adjectives, past-tense verbs, article assimilation) is the highest-frequency correctness risk after
  plurals; dedicate a review pass.
- Loanword strategy follows Microsoft: naturalized English for the computing domain (fajl, fowlder, ikkopja), native
  Semitic verbs where established (iftaħ, ħassar, fittex), no Italian substitutes, no purist coinages.
- Error messages: calm, natural, no exclamation marks, never the words "error" or "failed".
- Sorting and numbers: rely on `Intl` ('mt') at runtime; never ASCII-strip Maltese letters in stored or displayed
  values.
