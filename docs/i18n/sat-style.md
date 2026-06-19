# Santali (sat) translation style guide

Working notes for translating Cmdr into Santali. Read [`README.md`](README.md) for how this fits the translation process.

LOW priority. Santali is an Austroasiatic (Munda) language, an official language of India (~7M speakers, mainly
Jharkhand, West Bengal, Odisha). The pile has ONLY a Microsoft style guide for `sat-Olck` (Ol Chiki script); no macOS
(Apple ships no Santali UI), no terminology glossary, no GNOME/Xfce. Thin precedent, but Santali has a strong modern
software push (Android system language from ~13, Microsoft style guide).

## Voice and tone

Friendly, concise, active, never alarmist. Microsoft's `sat-Olck` style guide is the only software-tone anchor. Native
review required. Confidence: tentative.

## Decision points

### Script: Ol Chiki (the defining choice)

Santali is written in multiple scripts historically (Devanagari, Bengali, Odia, Latin), but the OFFICIAL and identity-
significant script is **Ol Chiki** (ol-chiki), purpose-built for Santali in 1925, Unicode block U+1C50–U+1C7F.

- The tag `sat-Olck` (in the pile) pins this to Ol Chiki. Microsoft's style guide is Ol Chiki.
- Ol Chiki is a true alphabet (not an abugida), LTR.
- Recommendation: target **Ol Chiki (`sat-Olck`)**, it's the official script and carries strong cultural identity for
  Santali speakers; using Devanagari/Latin would read as second-class. Confidence: high.
- Font caveat: Ol Chiki needs a font that covers U+1C50–U+1C7F; verify the app's font stack renders it (system font may
  not). This is an app-readiness check, like RTL is for Pashto. Flag for David.

### Resourcing / scope

No Apple OS reference and no terminology glossary, only a Microsoft style guide. Recommendation: low priority for launch
absent specific demand; if pursued, the Microsoft `sat-Olck` style guide is the sole anchor and a native reviewer is
mandatory. Confidence: high that it's low priority.

### Coining technical vocabulary

Like other newly-digitized languages, Santali lacks settled native vocabulary for "file/folder/pane/tab"; the Microsoft
style guide is the only precedent for how to handle this (coin vs borrow). Recommendation: follow the style guide's
approach; don't invent independently. Confidence: tentative.

### Numerals

Ol Chiki has its own digit set (U+1C50–U+1C59), but Western digits are also common. Let `Intl` with `sat`/`sat-Olck`
decide; confirm audience expectation with a reviewer. Confidence: tentative.

### Gender

Santali (Munda) has no grammatical gender in the European sense, so user-gender-agreement problems mostly don't apply.
Confidence: tentative (verify with reviewer).

## Terminology and glossary

None in the pile beyond the style guide; defer to a native reviewer using the Microsoft `sat-Olck` style guide.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look. Enforced by `desktop-i18n-dont-translate`.

## Plurals

CLDR categories for `sat`: `one`, `two`, `other`. Note the distinct **two** (dual) category, Santali grammatically marks
a dual number, so count messages must write a `two` branch in addition to one/other. This is unusual (most languages lack
`two`) and easy to miss.

## Decisions to confirm with David

- Is Santali in scope? Recommend low priority for launch (no Apple reference, needs native reviewer).
- If pursued: target Ol Chiki (`sat-Olck`), and verify the app's font stack renders the Ol Chiki Unicode block, an
  app-readiness check parallel to RTL. The `two`/dual plural category is the translation gotcha.
