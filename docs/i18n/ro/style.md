# Romanian (ro) translation style guide

Working notes for translating Cmdr into Romanian. Read [`README.md`](../README.md) for how this fits the translation
process.

Romanian is well-resourced in the pile: macOS Finder/AppKit, full Microsoft terminology + style guide, and GNOME
Nautilus + Xfce Thunar. Triangulation is easy; lean on macOS Finder first.

## Voice and tone

Friendly, concise, active, never alarmist. Romanian tech UI is fairly direct and a little formal by default; keep it
warm without slang. Error messages stay calm and actionable. macOS Finder is the closest tone reference.

## Formality

**Verdict: informal `tu`, not `dumneavoastră`.** Consumer brands (IKEA, Spotify, Netflix, and peers; IKEA-RO leads with
informal `tu`/`introdu`) address Romanian users informally, which fits Cmdr's friendly personal voice. Formality
decision recorded in [`formal-informal-decisions.md`](../formal-informal-decisions.md).

- "Copiere", "Mutare", "Redenumire", "Ștergere" as button/menu noun-infinitive labels, OR the imperative "Copiază",
  "Mută" form. macOS Finder and Microsoft both lean on the infinitive/supine noun form ("Copiere", "Lipire") for menu
  commands, which reads neutral and sits fine under a `tu` register. Prefer that for labels.
- When running text addresses the user, use the familiar **tu** register (via the singular verb form), matching the
  consumer-brand norm. Never the polite **dumneavoastră** in product UI.

## Decision points

### Diacritics: comma-below vs cedilla (ș/ț vs ş/ţ)

The single most common Romanian localization bug. Romanian uses **ș** and **ț** (S/T with comma below, U+0218/U+021A,
U+0219/U+021B), NOT the visually similar Turkish **ş**/**ţ** (cedilla). Old fonts/codepages forced the cedilla forms;
correct modern Romanian requires the comma-below glyphs.

- Apple (macOS) and Microsoft both ship the correct **comma-below** ș/ț in current Romanian.
- Recommendation: always use comma-below ș (U+0219) and ț (U+021B), and â/î/ă correctly. Verify the source strings don't
  carry legacy cedilla characters; normalize if they do. Confidence: high. This is a correctness rule, not a style call.

### "â" vs "î" spelling (the 1993 reform)

Romanian writes the same sound as **î** word-initially/finally and **â** word-internally (e.g. "România", "în",
"încărcare", "câmp"). The post-1993 Romanian Academy spelling is the standard.

- Apple and Microsoft both follow the post-1993 Academy norm.
- Recommendation: follow the Academy spelling (â internal, î at edges, "sunt" not "sînt"). Confidence: high.

### Regional variant: Romania vs Moldova

Romanian is also official in Moldova (`ro-MD`), historically written in Cyrillic but now Latin script; the spoken/written
standard is effectively the same Academy Romanian with minor lexical differences.

- Apple, Microsoft, Google, Spotify, and Netflix all ship a single "Română" locale, not a separate Moldovan one.
- Recommendation: target standard `ro` (Romania), no separate ro-MD. Confidence: high.

### Gender and inclusive language

Romanian is grammatically gendered (masculine/feminine/neuter, with adjective agreement). Cmdr rarely addresses the user
with a gendered adjective; structure around infinitives and nouns to avoid it. Where an adjective must agree with the
user, the conventional unmarked masculine is standard; there is no widely adopted neutral-morphology convention in
Romanian product UI (Apple/Microsoft/Google/Spotify/Netflix all use conventional agreement). Recommendation: rephrase to
avoid gendered user-adjectives; otherwise unmarked masculine. Confidence: high.

## Terminology and glossary

Defer the full glossary; triangulate macOS Finder (highest) + Microsoft terminology + Nautilus/Thunar as terms arise.

| English term | Romanian | Notes |
| ------------ | -------- | ----- |
| file | fișier | comma-below ș |
| folder | dosar / folder | confirm Finder; "folder" is also widely used |
| trash | Coș (de gunoi) | Finder term to confirm |
| copy | Copiere / Copiază | infinitive-noun preferred in menus |
| pane | panou | confirm |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `ro`: `one`, `few`, `other`. Romanian has a distinct **few** category (numbers ending 01-19 above
the unit, and 0), so e.g. "0 fișiere", "2 fișiere", "20 de fișiere" select differently. Plural messages must write a
`few` branch, and note the "de" preposition appears before nouns with 20+ counts ("20 **de** fișiere" vs "2 fișiere") -
ICU can't insert "de" for you, so bake it into the `few`/`other` branch text correctly.

## Notes and decisions

- Quotation marks: Romanian uses „..." (low-9 opening, high-9 closing) for primary quotes, «...» for nested. Match where
  quotes appear.
- Decimal comma, period/space thousands; let `Intl` format.
- The "de" insertion rule above is the subtlest Romanian grammar trap for counts.

## Decisions to confirm with David

- None blocking. "dosar" vs "folder" for folder is the one term worth a native check.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and
add to it as you settle terms, each sourced from the reference pile (`_ignored/i18n/ro/`; recipes in
`_ignored/i18n/how-to-mine.md`). Never guess a term.
