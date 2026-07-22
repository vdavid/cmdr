# Portuguese (pt) translation style guide

Working notes for translating Cmdr into Portuguese. Read `../README.md` for how this fits the translation process.

The base `pt` tag is a decision in itself: Portuguese splits hard between Brazil (`pt-BR`) and Europe/Portugal
(`pt-PT`). See the script/variant decision point below before treating `pt` as one language.

## Voice and tone

Friendly, concise, active, never alarmist, matching Cmdr's English voice. Portuguese tech UI (both variants) reads
warmer than German or Russian: contractions of preposition + article are everywhere (`no`, `na`, `do`, `da`), and that's
correct register, not slang. Error messages stay calm and actionable; avoid dramatic words. Apple's pt-BR Finder is the
closest reference for the tone Cmdr wants.

## Formality

Use the implied-subject imperative for UI actions, which sidesteps the tu/você split entirely. Portuguese verb
imperatives in UI almost never name the subject:

- "Copiar", "Mover", "Renomear", "Apagar" / "Excluir" for buttons and menu items (infinitive-as-imperative, the macOS
  and Microsoft convention for both variants).
- When the UI must address the user in running text (onboarding, confirmations), use **você** (pt-BR) or the implied
  third-person verb form (pt-PT), never **tu**. Both Apple and Microsoft use the você/implied register, never the
  tu-conjugated familiar form, in product UI.
- Avoid the explicit second-person pronoun where the verb alone carries the meaning ("Seus arquivos foram movidos" reads
  fine; prefer active "Movemos seus arquivos" where Cmdr's English uses active voice).

## Decision points

### Variant: pt-BR vs pt-PT vs a shared base

The single biggest Portuguese decision. The two variants differ in vocabulary, spelling, and some grammar, enough that a
single text reads "foreign" to one side. How the majors handle it:

- Apple ships **both** pt-BR (Brazil) and pt-PT (Portugal) as separate Finder/macOS locales; the reference pile has
  both.
- Microsoft ships **both** pt-BR and pt-PT terminology and full style guides.
- Google, Spotify, Netflix all offer **both** "Português (Brasil)" and "Português (Portugal)" as distinct UI locales.
- The industry norm is therefore two locales, not one blended Portuguese.

Concrete vocabulary splits that matter for a file manager:

- "file": pt-BR **arquivo** vs pt-PT **ficheiro**. This is the highest-frequency divergence in the whole catalog and
  alone makes a shared text wrong for one side.
- "delete / trash": pt-BR leans **Excluir** + **Lixeira** (trash); pt-PT leans **Eliminar** / **Apagar** + **Lixo** /
  **Reciclagem**.
- "folder": both use **pasta** (shared, safe).
- "screen / monitor", "mouse", "username", and many UI nouns also diverge.

Spelling: the 1990 Orthographic Agreement narrowed but did not erase the gap (e.g. accentuation and some consonant
clusters still differ in practice and in vendor style guides).

**Settled: `pt` ships BRAZILIAN Portuguese (pt-BR).** Recorded in `../language-selection-decisions.md` (pt = wave 1,
ships as pt-BR; pt-PT = wave 2) and in the reference pile's own `_ignored/i18n/pt/_see-also.txt`. Never ship one blended
"Portuguese", and never mine the bare `_ignored/i18n/pt/` folder: that one is EUROPEAN, and using it is the documented
variant trap. Mine `_ignored/i18n/pt-BR/`.

**pt-PT tells worth grepping for before you ship a batch** (each one is a real regression found in a shipped batch):

- `ficheiro`/`ficheiros` → pt-BR **arquivo(s)**.
- The `estar a` + infinitive progressive (`está a indexar`, `A indexar`) → pt-BR **gerund** (`está indexando`,
  `Indexando`).
- `consoante` (= "according to") → **conforme** / **pelo quanto**.
- Proclitic object pronouns before an infinitive (`para a preparar`) → pt-BR enclisis on the infinitive
  (`para prepará-la`).
- `Rever` → **Revisar**. `alterar o nome` → **renomear**.
- A dropped `você` where the verb form alone is ambiguous (`Excluiu esta pasta…` → `Você excluiu esta pasta…`).

### Spelling reform compliance

Within whichever variant you pick, follow the post-1990 Orthographic Agreement spelling (e.g. drop the silent consonants
the reform removed). Apple and Microsoft both ship reform-compliant strings. Recommendation: post-reform spelling
throughout. Confidence: high.

### Gender and inclusive language

Portuguese is grammatically gendered (o/a, -o/-a adjective agreement). Cmdr's UI rarely addresses the user with a
gendered adjective, so this is mostly avoidable by structuring around nouns and infinitives. Where an adjective would
agree with the user ("tem certeza?"), Portuguese conventionally uses the masculine as the unmarked default; the
"x"/"@"/"e" neutral forms (e.g. "todes") are activist register, NOT used by Apple, Microsoft, Google, Spotify, or
Netflix in product UI. Recommendation: avoid gendered user-adjectives by rephrasing; where unavoidable, use the
conventional unmarked masculine, matching every major. Don't use "x"/"@"/"-e" neutral morphology. Confidence: high.

### "Apagar" vs "Excluir" vs "Eliminar" for delete/trash

A file manager hits delete constantly, and the verb choice is variant-coded AND semantically loaded (permanent delete vs
move-to-trash), so it's the most likely consistency bug. **Locked in the glossary** (pt-BR): delete → **Apagar**, delete
permanently → **Apagar permanentemente**, the trash action → **Mover para o Lixo** (trash noun = **Lixo**, the macOS
Finder Tier-1 value). Cmdr is a macOS app, so Finder's own "Apagar" beats the Windows-influenced "Excluir" (term-choice
principle 2). "Excluir" survives only in its non-delete senses: query-scope exclude, and the AI-model deletion in
`ai.json`. See the glossary's reconciliation note.

## Terminology and glossary

Defer the full glossary until the variant is chosen (every row depends on it). Triangulate pt-BR/pt-PT macOS Finder
(highest authority, both in the pile) + Microsoft terminology + GNOME/Xfce.

| English term | Portuguese (pt-BR default) | Notes                                                    |
| ------------ | -------------------------- | -------------------------------------------------------- |
| file         | arquivo                    | pt-PT: ficheiro                                          |
| folder       | pasta                      | shared                                                   |
| trash        | Lixo                       | macOS Finder Tier-1 (glossary-locked); pt-PT: Reciclagem |
| pane         | painel                     | confirm vs Finder                                        |
| tab          | aba                        | pt-BR; pt-PT: separador                                  |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by
`desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `pt`: `one`, `many`, `other` (same for pt-BR and pt-PT). Note `many` is a real, distinct category in
modern CLDR Portuguese (compact/large numbers), so plural messages must write a `many` branch, not just one/other.

## Notes and decisions

- Roster: Cmdr ships pt-BR for wave 1; pt-PT is a separate wave-2 variant (vocabulary, você/tu, spelling). See
  `../language-selection-decisions.md`.
- Quotation marks: pt-BR commonly uses curly "" (like English); pt-PT traditionally uses guillemets «». Match the chosen
  variant.
- Decimal/thousands: both use comma decimal, period (pt-PT) or period/space thousands. `Intl` handles this; don't
  hardcode.
- See the template's ICU mechanics note (double apostrophes, keep `{placeholder}`/`<tag>` verbatim).

## Decisions to confirm with David

- None open. (The `pt` = pt-BR question is settled; see the variant section above.)

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/pt/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
