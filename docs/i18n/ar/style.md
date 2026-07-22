# Arabic (ar) translation style guide

Working notes for translating Cmdr into Arabic. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Arabic.

Arabic is the highest-effort target language in the set: it is the only RTL script here, it has full bidirectional
(bidi) layout concerns, six CLDR plural categories, and a real formal-register and gender split. Read the Decision
points before translating; several of them change how a sentence is built, not just the words.

## Voice and tone

Friendly, concise, active, calm. Cmdr's Arabic should read like Modern Standard Arabic (MSA / فصحى) as a real macOS app
speaks it: natural, not stiff or classical, but never colloquial/dialect. macOS Arabic is the model: it is plain MSA,
warm, and never alarmist. The Microsoft Arabic style guide explicitly tells localizers to avoid an unnecessarily formal
tone and to use everyday words (verified in `ar/microsoft-style-guides/StyleGuide.pdf`, pdftotext + grep, 2026-06-19).

Error messages stay calm and actionable and never label themselves with "خطأ" (error) or "فشل" (failed) as a bare
heading: state what happened and the next step. Note that macOS does use "تعذر …" ("could not …", e.g. "تعذر إنشاء
المجلد.") freely, that calm "could not" framing fits Cmdr's rule and is preferred over "خطأ"/"فشل".

## Formality

Arabic UI does not have a T/V pronoun split the way European languages do, but it does have a politeness-and-directness
decision in how actions and instructions are phrased. The settled choice is **plain, direct MSA, second person where
needed**. Two mechanics carry it (see Decision points → action verb form, which is the bigger call):

- **Address the user with the imperative for instructions**, not the softened "يرجى …" (please …) on every line. macOS
  does use "يرجى" for genuine asks ("يرجى تقديم اسم جديد للعنصر." = please provide a new name), so keep "يرجى" for
  polite requests, but don't pad every sentence with it.
- **Avoid the second-person plural/honorific everywhere.** Standard singular address is correct for a personal app.

## Decision points

Formality register is set above (plain MSA). These are the Arabic-specific calls, several of which are structural.

- **Script and direction: RTL with contextual joining, no variant choice, but bidi is the work.** Arabic is written
  right-to-left in a single connected script (letters change shape by position). There is no script variant to pick
  (unlike zh or sr). The real cost is **bidirectional layout**: the whole UI mirrors (panes, toolbars, chevrons, the
  two-pane left/right relationship, progress direction), and any Latin runs embedded in Arabic text (a file path, a
  brand word like `Cmdr`/`SMB`, a number, a URL) flip locally inside an RTL paragraph. Apple (macOS/iOS), Microsoft
  (Windows), and Google all ship fully mirrored RTL Arabic UIs; this is table stakes, not optional.
  - Recommendation: full RTL mirroring, and treat every string with an embedded `{path}`, `{name}`, brand token, or
    number as a bidi case, wrap/isolate inserts so a path or count can't visually scramble the sentence. The English
    catalog's uncontrolled-insert rule (a `{path}` can be anything) compounds here: an LTR path dropped into an RTL
    sentence needs bidi isolation to render correctly. Confidence: high (Apple/Microsoft RTL products verified via the
    pile's macOS Arabic strings; bidi-isolation specifics unverified against Cmdr's renderer, flag for layout testing).

- **Action verb form: masdar (verbal noun) vs imperative, sources split.** This is the single biggest phrasing decision
  and the majors disagree:
  - **macOS Arabic uses the masdar (verbal noun)** for action labels and descriptions, consistently: "Erase" → "مسح",
    "Choose" → "اختيار", "Copies items …" → "نسخ العناصر …", "Moves items to the Trash" → "نقل العناصر إلى سلة
    المهملات", "Cancel" → "إلغاء" (all verified in `ar/macOS/Finder/Localizable.json`, 2026-06-19). So a button reads as
    "نسخ" (copying/copy) rather than the command "اِنسخ".
  - **Microsoft Arabic style guide recommends the imperative** and specifically says to use the plain imperative ("افتح
    التطبيق") instead of the قم بـ + infinitive construction ("قم بفتح التطبيق") (verified in
    `ar/microsoft-style-guides/StyleGuide.pdf`, 2026-06-19).
  - Recommendation: follow **macOS, masdar for button/menu labels** ("نسخ", "نقل", "حذف", "إعادة تسمية", "إلغاء"), since
    Cmdr is a macOS app and consistency with Finder is what a Mac user expects; reserve the imperative for in-sentence
    instructions to the user. Never use the قم بـ construction either way (both Apple and Microsoft avoid it).
    Confidence: high (both sources verified; the split is a genuine Apple-vs-Windows convention worth recording).

- **Numerals: Eastern Arabic (٠١٢٣) vs Western (0123).** macOS Arabic renders digits as **Eastern Arabic numerals** in
  running UI text: "بعد ٣٠ يوم" (after 30 days), "٦٤٫٠ ميغابايت من ١٫٣٣ غيغابايت", and dates/counts throughout (verified
  in `ar/macOS/Finder/Localizable.json`, 2026-06-19). Western digits are common on the web and in much of the
  Gulf/Levant tech UI, so usage genuinely varies by region and product.
  - Recommendation: let the **`Intl`/formatter layer decide**, don't hardcode either digit set in catalog strings - keep
    `{count}` a plain placeholder and let locale-aware number formatting render the digits. Match macOS (Eastern Arabic)
    if a default must be chosen, but this is a David call because it's region-sensitive. Confidence: tentative (macOS
    behavior verified; the right default for Cmdr's audience is a judgment call). Flag for David.

- **Regional variant: ship one MSA `ar`, no country variant.** MSA is the shared written standard across all
  Arabic-speaking countries; dialects (Egyptian, Gulf, Levantine, Maghrebi) diverge heavily but are spoken/colloquial,
  not used for software UI. Apple and Microsoft both ship a single MSA Arabic for products (no `ar-EG` vs `ar-SA` UI
  split for the core OS). Recommendation: one `ar` in MSA. Confidence: high (consistent with the single macOS Arabic in
  the pile; pan-Arab-MSA convention corroborated by web localization sources, unverified beyond that).

- **Gender: keep the user ungendered; prefer gender-neutral phrasing.** Arabic is heavily gendered (verbs, adjectives,
  and pronouns inflect for the addressee's gender), so a second-person instruction can leak an assumed gender. The
  Microsoft Arabic style guide has a dedicated "Avoid gender bias" section: use gender-neutral alternatives and
  collective nouns instead of masculine-default forms (verified in `ar/microsoft-style-guides/StyleGuide.pdf`,
  2026-06-19). macOS sidesteps it by leaning on the masdar (a verbal noun carries no addressee gender) and impersonal
  phrasing.
  - Recommendation: prefer the **masdar and impersonal constructions** (which the macOS-matching choice above already
    gives us) precisely because they avoid gendering the user; where direct address is unavoidable, use neutral phrasing
    rather than defaulting to masculine. Confidence: high.

- **Length: Arabic can run shorter OR longer than English, and joining matters.** Arabic words are often compact, but
  connected script and full diacritic-free spelling make width unpredictable; line height and the connected baseline
  also differ. Recommendation: overflow-check against the pseudolocale AND specifically test Arabic for RTL clipping and
  bidi reordering, not just length. Confidence: high (general RTL-typography guidance; unverified against Cmdr's exact
  layout).

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order: macOS (Tier 1) → Microsoft (Tier 2) → GNOME/Xfce
(Tier 3). All terms below are taken from macOS Arabic Finder strings unless noted (verified in
`ar/macOS/Finder/Localizable.json`, 2026-06-19).

- file → ملف · macOS Finder · high
- folder → مجلد · macOS Finder ("Folder" → "مجلد", "Could not create the folder." → "تعذر إنشاء المجلد.") · high
- trash → سلة المهملات · macOS Finder (consistent) · high
  - Note: macOS also shows "سلة المحذوفات" ("Bin") and "العناصر المحذوفة" ("Deleted Items") in places; prefer "سلة
    المهملات" for the Trash, matching the dominant Finder usage.
- copy → نسخ (masdar) · macOS · high
- move → نقل · macOS ("Move ${entities} to ${destinationFolder}" → "نقل ${entities} إلى ${destinationFolder}") · high
- move to trash → نقل إلى سلة المهملات · macOS ("Moves items to the Trash" → "نقل العناصر إلى سلة المهملات") · high
- rename → إعادة تسمية · macOS ("Rename ${target} to ${newName}" → "إعادة تسمية ${target} باسم ${newName}") · high
- delete / erase → حذف / مسح · macOS ("Erase" → "مسح"; "Delete Immediately" → uses حذف) · high
  - Note: "مسح" is erase/wipe (disk), "حذف" is delete (item). Keep the distinction.
- cancel → إلغاء · macOS · high
- OK → موافق · macOS · high
- item(s) → عنصر / العناصر · macOS ("Items" → "العناصر") · high
- document → مستند · macOS · high
- computer → الكمبيوتر · macOS · high
- network → الشبكة · macOS · high
- server → الخادم · macOS ("Connected servers" → "الخوادم المتصلة", "اتصال بالخادم…") · high
- search → بحث · macOS ("Search This Mac" → "بحث في هذا الـ Mac") · high
- eject → إخراج · macOS Finder menu ("إخراج") · high
- compress → ضغط · macOS · high
- overwrite → الكتابة فوق · macOS ("Overwrite at Destination" → "الكتابة فوق الوجهة") · high
- new folder → مجلد جديد · macOS ("مجلد جديد") · high
- tab → علامة تبويب · macOS ("علامة تبويب جديدة"; "open in tabs" → "في علامات تبويب") · high
- sidebar → الشريط الجانبي · macOS ("إخفاء الشريط الجانبي") · high
- pane → بحاجة لقرار · no clean Finder source · tentative
  - macOS uses "الشريط الجانبي" for the sidebar and "نافذة" for window; a two-pane file list pane has no single
    canonical term. Candidate: "جزء" (part/pane) or "لوحة". Flag for David.
- listing → قائمة الملفات · no direct source · tentative
  - "listing" (the file list in a pane) has no canonical macOS term; "قائمة" (list) reads naturally. Confirm with David.
- volume → وحدة تخزين · macOS uses وحدة التخزين for storage volume · tentative
  - Verify against macOS volume strings; do not use a word that means audio volume.

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim, in Latin script, left-to-right within the RTL text: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte,
Quick Look, plus the `{system_settings}`-style tokens. Enforced by `desktop-i18n-dont-translate` (list in
`apps/desktop/scripts/i18n-catalog-lib.ts`). Each such token is a bidi island inside an RTL sentence, see the script
decision point; structure the sentence so the LTR token reads correctly. macOS itself keeps these Latin ("اتصال
بالخادم…" but "SMB"/"Mac" stay Latin), so this matches user expectation.

## Plurals

CLDR categories: **`zero`, `one`, `two`, `few`, `many`, `other`**, all six (verified with `new Intl.PluralRules('ar')`
and corroborated by the GNOME Arabic catalog header `nplurals=6` in `ar/gnome-nautilus/nautilus.po`, 2026-06-19). This
is the widest plural set in the language pool. Every plural message MUST write all six branches that Arabic needs; the
`desktop-i18n-plural` check enforces coverage.

- Arabic plural agreement is intricate: the noun form and the verb both change with the count (dual for two, a special
  form for 3–10, etc.). Get the noun form right inside each branch, don't just swap the number.
- The counted noun's grammatical case also shifts with the number; phrase each branch as natural Arabic, not a template.

## Notes and decisions

- **Direction and bidi are the dominant concern**, re-read the script decision point. RTL mirroring plus per-insert bidi
  isolation is the bulk of the Arabic-specific work.
- **Punctuation mirrors too**: Arabic uses the Arabic comma "،" and Arabic question mark "؟" (mirrored), and macOS
  Arabic uses them ("هل تريد مسح \"%@\"؟"). Prefer Arabic punctuation, not the Latin `,`/`?`.
- **Numbers and dates come from the formatter layer**, never hardcode digits or separators (see the numerals decision
  point; macOS uses the Eastern Arabic decimal separator "٫" and thousands handling via formatting).
- **ICU mechanics** (catalog-level, easy to miss): double every apostrophe in a value (`'` becomes `''`; ICU treats a
  lone `'` as an escape and silently swallows text), and keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  the agent-handoff block in `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Decisions to confirm with David

- **Numerals default** (tentative): Eastern Arabic (macOS) vs Western digits. Region-sensitive; recommend
  formatter-driven, but the default is David's call.
- **pane → جزء / لوحة** (tentative): no canonical Finder term for a file-list pane.
- **listing → قائمة الملفات** (tentative): no canonical source.
- **volume → وحدة تخزين** (tentative): verify against macOS volume strings; avoid the audio sense.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/ar/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
