# Thai (th) translation style guide

Working notes for translating Cmdr into Thai. Read [`README.md`](README.md) for how this fits the translation process.
Thai is a tier-1 well-localized language: Apple (Finder), Microsoft, Google, Spotify, and Netflix all ship Thai, so
triangulation evidence is strong. Sources mined for this guide: macOS Finder/AppKit Thai strings, Microsoft Thai
terminology and style guide, GNOME Nautilus and Xfce Thunar Thai catalogs.

This is a living doc, and capturing is your job. When you discover a convention, gotcha, or ruling that wasn't already
written, add it here.

## Voice and tone

Cmdr's Thai voice is polite, clear, and conversational, matching Cmdr's English voice (friendly, concise, active, never
alarmist). The Microsoft Thai voice guidance lines up exactly: warm and relaxed, "less formal, more grounded," writing
for scanning first, and deliberately avoiding stiff, overly formal vocabulary. Use everyday words a Thai user knows over
formal or technical synonyms.

Error and warning messages stay calm and actionable, never alarmist. Keep the English rule of avoiding the words "error"
and "failed"; Thai has neutral equivalents (state what happened and what to do, for example phrasing around
ไม่สามารถ… "couldn't…" rather than a loud failure word).

## Formality

Thai has no T-V (formal/informal second person) distinction like European languages, but it has a strong politeness and
register system built from pronoun choice, vocabulary level, and sentence-final particles. The decisions that matter:

- **Second person: คุณ ("you").** This is the standard polite, gender-neutral, neutral-register pronoun for software
  addressing a user, used by Microsoft's Thai style guide and macOS. Don't use formal/deferential ท่าน (reads stiff and
  corporate) or any familiar/intimate pronoun. Often the pronoun is dropped entirely in UI, which is natural and fine.
- **Imperatives for buttons and menu items: bare verb, no particle, no politener.** macOS Finder labels actions as plain
  verbs (คัดลอก "Copy", ย้ายไป "Move to", เปิด "Open", ทิ้ง…ลงในถังขยะ "Move…to Trash"). This is the correct register for
  Cmdr's buttons and menus: concise, direct, polite by default in Thai because a bare verb isn't rude.
- **No sentence-final politeness particles (ครับ/ค่ะ) in standard UI.** See the decision point below; this is the single
  highest-stakes register call and macOS/Microsoft both omit them.

## Decision points

### No word spaces: segmentation, line-breaking, and truncation

- Thai writes words with no spaces between them; spaces mark phrase or sentence boundaries only. Word boundaries are
  inferred by the reader (and by software via a dictionary segmenter, for example ICU). This is the defining Thai
  layout challenge for a file manager.
- Effects to design for: line-breaking can only happen at (invisible) word boundaries, so a naive break at any character
  splits a word and reads as broken; mid-word truncation with an ellipsis can chop a word so it changes or loses
  meaning; tone marks and combining vowels above/below the base consonant must not be split from their base; the
  repetition mark ๆ and abbreviation mark ฯ must not start a line.
- Thai text runs roughly 15% longer than English on average, worsening clipping in fixed-width UI (path bars, status
  lines, column headers). The pseudolocale (`en-XA`) already stress-tests length; treat Thai as a real case of it.
- Majors: macOS and the Thai web of Apple, Microsoft, Google, Spotify, and Netflix all rely on the OS/browser ICU-based
  Thai line-breaker rather than spacing words manually. Their catalog strings contain no inserted spaces between Thai
  words.
- **Recommendation (high):** write Thai values with no spaces between words, exactly as the language is written, and rely
  on the platform's Thai-aware line-breaker (macOS/WebKit ships one). Don't hand-insert spaces or zero-width spaces to
  force breaks. Where Cmdr truncates filenames or paths, prefer truncating at the OS layer; if Cmdr does its own
  ellipsizing, that logic must be Thai-segmentation-aware, not byte- or grapheme-naive. Flag any Cmdr-side truncation or
  middle-eliding of paths as a layout item to verify with a real Thai string during the overflow check.
- **For David:** whether Cmdr's own path/filename ellipsizing is segmentation-aware is an engineering question to verify
  in the app, not a translation choice.

### Sentence-final particles ครับ / ค่ะ in UI

- In spoken and conversational written Thai, the politeness particles ครับ (male speaker) and ค่ะ/คะ (female speaker)
  end polite sentences, and they carry the speaker's gender.
- In software UI, the majors omit them: macOS Finder and AppKit Thai strings carry no politeness particles, and the tone
  stays polite through word choice alone. Adding them would (a) force a speaker gender onto the app's voice, and (b) read
  as chatty/spoken in a place that should be neutral product copy.
- **Recommendation (high):** do NOT use ครับ/ค่ะ in Cmdr UI strings (buttons, labels, menus, status, errors). Politeness
  comes from vocabulary and the คุณ pronoun, not particles. This also sidesteps the gender problem entirely.

### Thai numerals (๐-๙) vs Arabic numerals (0-9)

- Thai has its own digit glyphs (๐๑๒๓๔๕๖๗๘๙) but Arabic numerals are overwhelmingly the everyday default in modern Thai,
  especially in software, tech, and anything with counts or sizes.
- Majors: macOS Thai Finder uses Arabic numerals throughout (no Thai digits found in the UI strings); Microsoft, Google,
  Spotify, and Netflix Thai UIs likewise use Arabic numerals. Thai digits survive mainly in formal/ceremonial print and
  some government documents, not app UI.
- **Recommendation (high):** use Arabic numerals (0-9) everywhere in Cmdr (file counts, sizes, progress, percentages).
  Cmdr's `Intl.NumberFormat('th')` already produces Arabic digits by default. Keep the English thousands-separator
  convention via `Intl`; Thai uses the same comma-grouping with Arabic digits.

### Dates and the Buddhist era

- Thailand officially uses the Buddhist Era (BE) calendar, which is 543 years ahead of the Gregorian/Common Era (so
  2026 CE = 2569 BE), with a common day/month/year order. Both calendars circulate; BE is the formal/official default,
  Gregorian appears in international and some tech contexts.
- Majors: Apple and Microsoft expose BE as the locale default for Thai system date formatting, so OS-formatted dates a
  Thai user sees elsewhere are typically BE.
- **Recommendation (tentative):** for any date Cmdr formats itself, format via `Intl.DateTimeFormat('th')` and let the
  platform decide era and digits rather than hand-building date strings, so Cmdr matches whatever the user's OS shows.
  Don't hardcode a Gregorian year into a Thai string.
- **For David:** decide whether Cmdr's file-listing dates should follow the OS Thai locale (likely BE, matching Finder)
  or stay Gregorian for consistency with the rest of the app. Recommend following the OS so it feels native, but this is
  a product call. Confirm what Cmdr's date formatter actually emits for `th` during review.

### Gender and inclusive language

- Thai nouns and verbs have no grammatical gender, and the standard UI pronoun คุณ is gender-neutral, so Thai UI is
  inherently gender-neutral as long as you avoid the gendered politeness particles (covered above). There's no he/she
  agreement to resolve.
- **Recommendation (high):** no special handling needed beyond "no ครับ/ค่ะ." Avoid gendered speaker particles and Thai
  UI stays neutral by construction.

## Terminology and glossary

Early high-confidence terms (macOS Finder + Microsoft agree). Expand during the first translation pass.

| English term | Thai | Notes |
| ------------ | ---- | ----- |
| file | ไฟล์ | macOS, Microsoft. Loanword, standard. |
| folder | โฟลเดอร์ | macOS, Microsoft. Loanword, standard. |
| Trash | ถังขยะ | macOS Finder. "Move to Trash" = ทิ้ง…ลงในถังขยะ / ย้าย…ไปยังถังขยะ. |
| copy (verb) | คัดลอก | macOS Finder. |
| move (verb) | ย้าย / ย้ายไป | macOS Finder ("move to" = ย้ายไปยัง). |
| open (verb) | เปิด | macOS Finder. |
| tab | แท็บ | macOS, Microsoft. Loanword. |
| eject | นำออก | macOS (verify against Cmdr's eject context). |
| you (2nd person) | คุณ | Polite, neutral, gender-neutral. Often dropped in UI. |

Pane, volume, listing, transfer, viewer: not yet confirmed; triangulate during the first pass and record here with
sources + confidence.

## Brand and do-not-translate

Keep these verbatim (product or platform names): Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. The
same list (plus system placeholder tokens) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.js`.

## Plurals

Thai's CLDR plural categories: **`other` only** (confirmed via
`new Intl.PluralRules('th').resolvedOptions().pluralCategories` and the Thai GNOME catalog's `nplurals=1`). Thai has no
grammatical number on nouns: the same noun form covers one and many, and counting uses classifiers, not plural
inflection.

- Every ICU plural message needs only the `other` branch for Thai. `desktop-i18n-plural` requires that the categories a
  language needs are covered; for Thai that's just `other`.
- Write the `other` branch so it reads naturally for any count, including 1. Don't try to special-case 1 with a
  separate phrasing unless the English context genuinely needs it (then use `=1` exact match, not a plural category).
- Counted nouns in Thai often want a classifier (for example รายการ "item(s)" as a generic classifier-like noun), so a
  natural counted string is "{count} รายการ" rather than pluralizing the noun. Mind this when translating
  count-bearing strings.

## Notes and decisions

- **Polite-but-not-formal is the through-line.** When two Thai phrasings tie, pick the shorter, more everyday one
  (Microsoft Thai voice and macOS both favor this). Stiff formal vocabulary is the most common Thai-translation
  mis-register.
- **Quotation marks:** Thai commonly uses the same ASCII/curly double quotes as English for quoted names; macOS Thai
  Finder quotes filenames with " " (for example คัดลอก "บางสิ่ง" ไปยัง "บางที่"). Keep whatever quote style the English
  source uses unless it reads wrong.
- **Spaces around inserts:** because Thai has no word spaces, be careful where a `{placeholder}` meets Thai text. A path
  or filename insert may need a surrounding space or quote to stay readable against adjacent Thai characters; follow
  macOS Finder's pattern (it quotes inserted names). Verify per string during review.

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Thai rarely needs apostrophes, but any in a loanword or English fragment must be doubled.
- Keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../guides/i18n-translation.md) and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Decisions to confirm with David

- **Dates: follow OS Thai locale (likely Buddhist era, matching Finder) or stay Gregorian app-wide?** Recommend
  following the OS via `Intl.DateTimeFormat('th')` so listings feel native; confirm what Cmdr's formatter emits for `th`.
- **Cmdr-side path/filename truncation must be Thai-segmentation-aware.** Engineering verification item, not a
  translation choice: confirm during the overflow check with a real Thai string that mid-name ellipsizing doesn't split
  words or detach tone marks.
