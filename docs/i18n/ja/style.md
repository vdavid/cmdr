# Japanese (ja) translation style guide

Working notes for translating Cmdr into Japanese. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Japanese.

Japanese has no second-person T/V split like European languages, but it has a politeness-register system (keigo) that is
just as load-bearing for UI tone: getting the register wrong makes the app read as childish, cold, or oddly stiff. The
register decision is settled below; read it before translating.

## Voice and tone

Friendly, concise, active, calm. Cmdr's Japanese should read like a polished, modern macOS app: polite but not stiff,
warm but professional. macOS Japanese is the model, clean です・ます (desu/masu, "teinei" polite form), no slang, no
heavy honorific keigo. The Microsoft Japanese style guide explicitly says to avoid an unnecessarily formal/written tone
and use everyday words appropriate to the context (verified in `ja/microsoft-style-guides/StyleGuide.pdf`, pdftotext +
grep, 2026-06-19).

Error messages stay calm and actionable and never label themselves "エラー" (error) or "失敗" (failed) as a bare
heading: state what happened and the next step. macOS does this naturally, "フォルダを作成できませんでした。" ("could
not create the folder", verified in `ja/macOS/Finder/Localizable.json`, 2026-06-19) is the calm "could not …" framing
Cmdr wants, not "エラー".

## Formality

**Use です・ます (teinei, polite) register throughout.** This is settled, not a guess:

- macOS Japanese is consistently です・ます: "項目をゴミ箱に入れます" ("moves items to the Trash"),
  "項目の新しい名前を入力してください。" ("please provide a new name", polite -てください imperative),
  "フォルダを作成できませんでした。" (polite negative) (all verified in `ja/macOS/Finder/Localizable.json`, 2026-06-19).
- This is the de-facto standard for consumer software UI. Plain form (だ/である) reads as terse/blunt and is wrong for
  user-facing copy; heavy keigo (お…になる, 〜いたします) reads as over-formal/obsequious and is also
  wrong. です・ます is the warm-professional middle that matches Cmdr's friendly voice.

Mechanics:

- **Button and menu labels: bare noun-stem (連用形 / sahen noun), not a full sentence.** macOS labels are nouns:
  "キャンセル" (Cancel), "コピー" (Copy), "移動" (Move), "削除" (Delete), "名称変更" (Rename). Don't make a button
  a です・ますsentence, that's for descriptions, prompts, and status, not labels.
- **Sentences, prompts, status, and descriptions: です・ます.** "…します" / "…できませんでした" / "…してください".
- Don't force a subject pronoun ("あなた"), Japanese omits it; macOS does too.

## Decision points

Register is settled above (です・ます). These are the Japanese-specific calls.

- **Script: no variant choice; the live decision is kanji vs katakana for loanwords.** Japanese uses kanji + hiragana +
  katakana together, there is no script variant to pick (unlike zh Simplified/Traditional). The real recurring choice is
  how to render a borrowed tech term: as a katakana loanword or as a kanji compound. macOS picks per term and is the
  guide: "Folder" → "フォルダ" (katakana), "Copy" → "コピー" (katakana), but "Trash" → "ゴミ箱" (kanji+kana), "Document"
  → "書類" (kanji), "Move" → "移動" (kanji) (all verified in `ja/macOS/Finder/Localizable.json`, 2026-06-19). Apple,
  Microsoft, and Google all mix the same way.
  - Recommendation: **follow macOS term by term** (the glossary below records each choice). Don't katakana-ize a term
    macOS writes in kanji, or vice versa, consistency with Finder is what a Mac user expects. Confidence: high.
  - Note: long-vowel katakana spelling matters, macOS writes "フォルダ" and "コンピュータ" WITHOUT a trailing long-vowel
    mark "ー" (not "フォルダー"/"コンピューター"). Windows/Microsoft style adds the "ー". Match macOS (no trailing ー).
    Confidence: high (verified: "Folder" → "フォルダ", "Computer" → "コンピュータ").

- **Regional variant: one base `ja`, no country split.** Japanese is effectively single-region for software (ja-JP);
  there is no second national standard to target. Apple and Microsoft ship one Japanese. Recommendation: one `ja`.
  Confidence: high.

- **Counters (助数詞): count + counter word, not a bare number.** Japanese counts objects with a classifier suffix that
  depends on what's counted. macOS uses "件" for events/items in lists ("^0件" = N items, e.g. "^0件の参加依頼") and
  "個" for generic objects, and "項目" as the word for "item". For Cmdr's "N files / N items selected" style strings,
  this is a per-string call inside the plural/select branch.
  - Recommendation: use **件** or **個** consistent with what's being counted, matching macOS; treat the counter as part
    of writing the plural branch (Japanese has only one plural category, see Plurals, so the counter, not a plural form,
    carries the counting). When unsure which counter fits, flag the string. Confidence: high (macOS counter usage
    verified; exact counter per Cmdr string is a translate-time call).

- **Gender: not grammatically marked; nothing to handle.** Japanese verbs and adjectives don't inflect for the
  addressee's or referent's gender, and UI copy omits the subject pronoun, so there is no gendered-default trap the way
  there is in Arabic or Romance languages. Recommendation: no special handling needed. Confidence: high.

- **Spacing and line breaking: no inter-word spaces; wrapping is character-based.** Japanese text has no spaces between
  words, and line-breaking happens between characters with kinsoku (禁則) rules (certain characters can't start/end a
  line). This is a rendering concern, not a translation one, but it affects overflow: a Japanese string's wrap points
  differ from English. Recommendation: overflow-check against the layout; don't insert ASCII spaces to "help" wrapping.
  Confidence: high.

## Terminology and glossary

Format per term: `English → chosen · sources · confidence`. Tier order: macOS (Tier 1) → Microsoft (Tier 2) → GNOME/Xfce
(Tier 3). All terms below are from macOS Japanese Finder strings unless noted (verified in
`ja/macOS/Finder/Localizable.json`, 2026-06-19).

- file → ファイル · macOS Finder · high
- folder → フォルダ (no trailing ー) · macOS ("Folder" → "フォルダ", "Could not create the folder." →
  "フォルダを作成できませんでした。") · high
- trash → ゴミ箱 · macOS ("Trash" → "ゴミ箱") · high
- copy → コピー · macOS ("Copies items …" → "項目を別の場所にコピーします") · high
- move → 移動 · macOS ("Move ${entities} to ${destinationFolder}" → "${entities}を${destinationFolder}に移動") · high
- move to trash → ゴミ箱に入れる · macOS ("Moves items to the Trash" → "項目をゴミ箱に入れます") · high
- rename → 名称変更 · macOS ("Rename ${target} to ${newName}" → "${target}の名称を${newName}に変更") · high
- delete → 削除 · macOS (Finder menu 削除 usage) · high
- cancel → キャンセル · macOS ("Cancel" → "キャンセル") · high
- item → 項目 · macOS ("Items" → "項目") · high
- document → 書類 · macOS ("Document" → "書類") · high
- computer → コンピュータ (no trailing ー) · macOS ("Computer" → "コンピュータ") · high
- network → ネットワーク · macOS ("Network" → "ネットワーク") · high
- search → 検索 · macOS ("Search This Mac" → "このMacを検索") · high
- Quick Look → クイックルック · macOS ("クイックルック") · high (but keep "Quick Look" verbatim if treated as brand, see
  do-not-translate; macOS localizes it to katakana, so this is a David call: katakana "クイックルック" vs Latin "Quick
  Look". Flag.)
- viewer / preview → プレビュー · macOS preview UI · high
- server → サーバ (no trailing ー) · macOS · tentative (verify trailing-vowel spelling against macOS server strings)
- pane → ペイン · no clean Finder source · tentative
  - macOS uses "サイドバー" for the sidebar; a generic two-pane file-list pane has no canonical term. "ペイン" (katakana
    loan) or "領域". Flag for David.
- listing → 一覧 · no direct source · tentative
  - "listing" (the file list) → "一覧" reads naturally; confirm with David.
- volume → ボリューム · macOS uses ボリューム for a mounted disk volume · tentative (verify against macOS volume
  strings)

Add rows as terms come up, each with sources and a confidence.

## Brand and do-not-translate

Keep verbatim, in Latin script: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, plus the `{system_settings}`-style
tokens. Enforced by `desktop-i18n-dont-translate` (list in `apps/desktop/scripts/i18n-catalog-lib.js`). macOS keeps
these Latin in Japanese text ("このMacを検索" keeps "Mac" Latin). **Quick Look is the one wrinkle**: macOS localizes it
to katakana "クイックルック", but Cmdr's do-not-translate list keeps "Quick Look" verbatim, flag this conflict for David
(verbatim "Quick Look" vs macOS-native "クイックルック").

## Plurals

CLDR categories: **`other` only**, Japanese has no grammatical plural (verified with `new Intl.PluralRules('ja')` and
corroborated by the GNOME Japanese catalog header `nplurals=1` in `ja/gnome-nautilus/nautilus.po`, 2026-06-19). A noun
form does not change with count.

- Because there's no plural inflection, the count is carried by **the number + a counter word** (see the Counters
  decision point), not by selecting a plural branch. "1 file" and "5 files" use the same noun: "1個のファイル" /
  "5個のファイル" (or 件/項目 per context).
- A single `other` branch satisfies `desktop-i18n-plural`, but the branch text must read naturally for any count, write
  it counter-aware.

## Notes and decisions

- **Long-vowel "ー" follows macOS, not Windows**, re-read the script decision point. "フォルダ"/"コンピュータ"/"サーバ"
  without trailing ー is the macOS house style; don't "correct" to the Windows "ー" forms.
- **Punctuation: full-width Japanese marks.** Use the Japanese full stop "。" and comma "、" (not ASCII `.`/`,`), and
  full-width brackets/quotes where macOS does ("“…”" appears as full-width「…」/“…” in Finder). Match macOS.
- **No inter-word spaces; don't add ASCII spaces** for wrapping (see Spacing decision point).
- **Numbers and dates come from the formatter layer**, never hardcode separators. Japanese uses Western digits in modern
  UI (macOS does), so digits aren't a decision here.
- **ICU mechanics** (catalog-level, easy to miss): double every apostrophe in a value (`'` becomes `''`; ICU treats a
  lone `'` as an escape and silently swallows text), and keep every `{placeholder}` and `<tag>` verbatim. Full rules:
  the agent-handoff block in [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Decisions to confirm with David

- **Quick Look** (tentative): keep Latin "Quick Look" (Cmdr brand list) vs macOS-native katakana "クイックルック".
- **pane → ペイン / 領域** (tentative): no canonical Finder term for a file-list pane.
- **listing → 一覧** (tentative): no canonical source.
- **server / volume trailing-vowel spelling** (tentative): verify サーバ/ボリューム against macOS strings.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/ja/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
