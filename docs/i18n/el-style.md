# Greek (el) translation style guide

Working notes for translating Cmdr into Greek. Read [`README.md`](README.md) for how this fits the translation process.

This is the language base (`el`), the universal Greek (Modern Greek, monotonic). Greek is effectively a single standard;
no region variant is needed (see Decision points).

## Voice and tone

Friendly, concise, active, and never alarmist. Match the English register: warm and direct. Greek error and crash copy
stays calm and factual; the Microsoft Greek style guide explicitly steers away from old-fashioned, overly formal, or
archaic phrasing, which suits Cmdr's voice. Avoid heavy "σφάλμα"/"αποτυχία" framing where a calmer phrasing works.

## Formality

**This is the central Greek call, and it's split between the majors, flag for David.** Greek has a T-V distinction:
informal singular "εσύ" (verbs in 2nd-person singular, `-εις`) vs the polite/formal plural "εσείς" (verbs in 2nd-person
plural, `-ετε`).

- **macOS Greek uses the FORMAL plural (εσείς).** The mined Finder dialogs address the user in 2nd-person plural
  throughout: "Θέλετε…", "Αν διαγράψετε…", "Εάν μετακινήσετε…", "…με εσάς", "…συσκευές σας" (verified in `el/macOS/`,
  counted `-ετε` formal verb endings dominate the dialog text; the `-εις` hits are all nouns like "ρυθμίσεις", not
  informal verbs; 2026-06-20).
- **Microsoft Greek leans informal/conversational.** Its style guide pushes a relaxed, non-formal voice ("be informal,
  be friendly, talk like a real person"), and the general Greek-localization convention is that translators usually pick
  the formal plural but switch to informal singular for deliberately informal products.
- **Recommendation: formal plural (εσείς), confidence high, but flag for David.** A file manager addressing an unknown
  adult, on macOS, where the platform itself (Finder) uses formal plural, is the safe and native-feeling default; it
  reads respectful, not cold. The counter-case: Cmdr's English voice is warm and signs onboarding as David, which is the
  one argument for informal singular. Whichever is chosen, it MUST be consistent across the entire catalog. David
  settles whether Cmdr's Greek leans formal-native (match macOS) or informal-friendly (match its English personality).

**Imperatives for UI actions** (buttons, menu items): macOS Greek uses the **nominalized noun form**, not an imperative
verb: "Αντιγραφή" (copying), "Διαγραφή" (deletion), "Μετακίνηση" (moving), "Μετονομασία" (renaming), "Ακύρωση"
(cancellation), "Αποθήκευση" (saving), "Άνοιγμα" (opening), "Κλείσιμο" (closing) (verified in `el/macOS/AppKit/`,
2026-06-20). Use this noun-label form for buttons and menu items; reserve full 2nd-person verbs for sentences that
address the user (where the formality choice above applies).

## Decision points

- **Script and accents: monotonic, with no tonos on a standalone capital.** Modern Greek UI is monotonic (single
  acute accent `´`), never polytonic. Convention: a capital letter at the start of a word that would carry an accent
  does NOT get the tonos when standing alone or at the start of an all-caps run, but accents within a word are kept.
  Practical rule for Cmdr: avoid ALL-CAPS labels, because all-caps both strips the natural casing macOS uses and forces
  the accent-dropping question; macOS Greek button labels are sentence-case nouns ("Αντιγραφή"), not all-caps. Follow
  that. Confidence: high.
- **Final sigma (ς vs σ).** Greek lowercase sigma is written "ς" word-finally and "σ" elsewhere ("στοιχείς" is wrong;
  "στοιχείος"… the form changes by position). A translator typing Greek gets this right naturally, but watch
  programmatic transforms: never lowercase or truncate a Greek string in code in a way that turns a medial σ into a
  final ς or vice versa. Confidence: confirmed (Greek orthography).
- **Regional variant: none. One `el` base.** Greek as used in Greece and Cyprus is a single written standard for UI
  purposes; all majors ship one Greek. No `el-CY` split needed. Confidence: confirmed.
- **Gender: Greek is heavily gendered; address directly and avoid gendering the user.** Nouns, adjectives, and participles
  inflect for masculine/feminine/neuter. macOS and Microsoft Greek avoid gendering the user by using direct address and
  neutral phrasings rather than a generic gendered noun ("ο χρήστης"). There is no widely adopted gender-neutral ending
  in Greek UI (unlike Spanish's "@/x"). Recommendation: direct address (per the formality choice) and neutral nouns;
  when a participle or adjective must agree with the user, prefer rephrasing over guessing the user's gender. Confidence:
  high.
- **Length.** Greek runs moderately longer than English and uses longer compound-ish phrases; overflow-check tight
  buttons against the pseudolocale (`en-XA`). Confidence: high.

## Terminology and glossary

| English term | Greek | Notes |
| ------------ | ----- | ----- |
| Copy | Αντιγραφή | noun-label form, macOS |
| Move | Μετακίνηση | noun-label form, macOS |
| Delete | Διαγραφή | noun-label form, macOS |
| Rename | Μετονομασία | noun-label form, macOS |
| Cancel | Ακύρωση | noun-label form, macOS |
| Save | Αποθήκευση | noun-label form, macOS |
| trash | Κάδος (απορριμμάτων) | macOS Greek term for the trash ("Κάδος") |
| Settings | Ρυθμίσεις | macOS app-settings term |
| crash report | αναφορά σφάλματος | confirm against macOS/MS Greek; "σφάλμα" is standard but check the non-alarmist fit |

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate`; curated list in `apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Greek CLDR categories: `one`, `other` (verified with `new Intl.PluralRules('el')`, 2026-06-20; GNOME nautilus confirms
`nplurals=2; plural=(n != 1)`). Write both branches. Note that the noun in a counted phrase inflects for case and number;
get the agreement right inside each branch.

## Notes and decisions

- **Quotation marks**: Greek uses guillemets «…» (as seen throughout macOS Greek: «^2»), not English "…". Use «…».
- **Numbers and dates come from the formatter layer** (comma decimal, period/space thousands). Never hardcode
  separators.
- **Ellipsis**: keep the source's three literal ASCII dots to match the English catalog shape; macOS Greek's own
  "Άμεση διαγραφή…" uses a single `…`, but follow Cmdr's catalog convention.
- **ICU mechanics**: double every apostrophe in ICU values; keep every `{placeholder}` and `<tag>` verbatim. Full
  rules: [`../guides/i18n-translation.md`](../guides/i18n-translation.md).

## Decisions to confirm with David

- **Formality: formal plural (εσείς, macOS-native) vs informal singular (εσύ, matches Cmdr's warm English voice).**
  Recommended formal plural; David's call. Whatever is chosen, apply it consistently across the whole catalog.
- **crash report → "αναφορά σφάλματος"** (tentative): confirm the term reads non-alarmist enough, or pick a calmer
  phrasing.
