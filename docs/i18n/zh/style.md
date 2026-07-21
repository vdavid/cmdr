# Chinese (zh) translation style guide

Working notes for translating Cmdr into Chinese. Read [`README.md`](../README.md) for how this fits the translation
process, and the app-wide [`/docs/style-guide.md`](../../style-guide.md) for the English voice these notes carry into
Chinese.

Chinese is a tier-1 well-localized language: Apple (Finder), Microsoft, Google, Spotify, and Netflix all ship both
script variants, so triangulation evidence is strong. Sources mined for this guide: macOS Finder/AppKit strings in zh-CN
(Simplified), zh-TW and zh-HK (Traditional), plus the Microsoft zh-Hans and zh-Hant terminology and style guides, and
the GNOME Nautilus / Xfce Thunar zh-CN/zh-TW catalogs.

This is a living doc, and capturing is your job. When you discover a convention, gotcha, or ruling that wasn't already
written, add it here.

## Decisions to confirm with David

These are calls a translator can't make alone. The rest of this guide assumes them.

- **Which script variant(s) to ship: RESOLVED to Simplified `zh-Hans` only for now** (Traditional `zh-Hant`, Taiwan
  norm, is a fast-follow; never auto-convert, vocabulary differs). See the script decision point below and
  [`script-decisions.md`](../script-decisions.md). No longer open.
- **Formal vs neutral "you" (`您` vs `你`): RESOLVED to `你`** (consumer-brand evidence; legal/billing copy uses formal
  `您`; see Formality and [`formal-informal-decisions.md`](../formal-informal-decisions.md)). No longer open.

## Voice and tone

Cmdr's Chinese voice is friendly, concise, active, and never alarmist, matching the English. Microsoft's Chinese voice
guidance lines up with Cmdr's: "warm and relaxed, less formal, more grounded," "crisp and clear, write for scanning
first," and a deliberate preference for everyday words over stiff formal/technical vocabulary (verified against the
reference pile, `zh-Hans/microsoft-style-guides/StyleGuide.pdf`, 2026-06-20). Carry that over: short, spoken, modern
Mandarin, not bureaucratic or literary register.

Error and warning messages stay calm and actionable. Keep the English rule of avoiding the words "error" and "failed";
phrase what happened and the next step (Chinese has neutral framings like `无法…` / `無法…` "couldn't…") rather than a
loud failure word like `错误`/`失敗`.

**Demonstratives: prefer the spoken `这个` / `这项` / `这次` over the written `此` / `该`.** The catalog is dominated by
`这…` (`这个文件夹`, `这项操作`, `这个传输`); `此` reads as legal/technical register and clashes with the friendly
voice. Keep `此` only where it's already settled in a terse label (`此驱动器…` in the drive-index tooltips).

Chinese runs SHORT: a Chinese string is often half the character count of the English, so overflow is rarely the risk
(under-flow / too-sparse buttons can be). Still overflow-check, but the bigger care is that terse Chinese still reads
naturally and isn't cryptically clipped.

## Formality

- **Verdict: address the user as `你` (informal/neutral), not the formal `您`.** Chinese has a polite second-person `您`
  and a neutral `你`. Consumer brands (Apple zh-CN, WeChat, Bilibili, Xiaohongshu, Duolingo) use `你`, which fits Cmdr's
  friendly personal voice; macOS Finder/AppKit uses `你` exclusively (zero `您` across zh-CN and zh-TW; 411 and 398 `你`
  respectively, verified against the reference pile, 2026-06-20). Microsoft's house style leans `您`, but Cmdr picks
  `你`. Keep it consistent across the whole catalog; mixing reads as careless. Formality decision recorded in
  [`formal-informal-decisions.md`](../formal-informal-decisions.md).
- **Exception: legal and billing copy uses the formal `您`.** Where the copy is contractual (licensing, payment, terms),
  the formal `您` is the convention; reserve it for those strings and keep `你` everywhere else.
- **Buttons and menu items: bare verb, no politener.** macOS labels actions as plain verbs: `拷贝`/`拷貝` (copy),
  `移动`/`搬移` (move), `打开`/`打開` (open), `删除`/`刪除` (delete), `取消` (cancel). This is the correct register for
  Cmdr's buttons and menus: concise and direct, polite by default because a bare verb isn't rude in Chinese.

## Decision points

### Script: Simplified vs Traditional (the big one), and which region

**RESOLVED: ship Simplified `zh-Hans` only for now** (Traditional `zh-Hant`, Taiwan norm, is a fast-follow; never
auto-convert, the vocabulary differs). Recorded in [`script-decisions.md`](../script-decisions.md). The structure and
evidence below stand.

- **Two written standards, not mutually substitutable.** Simplified Chinese (`zh-Hans`) is the standard in Mainland
  China and Singapore; Traditional Chinese (`zh-Hant`) is standard in Taiwan, Hong Kong, and Macau. They differ in
  character shapes AND, importantly, in vocabulary and term choices (not a font swap). Serving Simplified to a Taiwan
  user, or vice versa, is a recognized localization miss (a Hong Kong `zh-HK` browser locale wrongly falling back to
  `zh-CN` is a documented bug class). `high`.
- **Within Traditional, Taiwan vs Hong Kong diverge on real terms.** Mined from macOS: folder is `檔案夾` in zh-TW but
  `資料夾` in zh-HK (100% consistent split: 233 vs 0 and 0 vs 228 occurrences, verified against the reference pile,
  2026-06-20). So `zh-Hant` written to the Taiwan norm is the mainstream Traditional default, but a Hong Kong user will
  notice term differences. Ship one `zh-Hant` to the Taiwan norm unless David wants a separate `zh-HK`.
- **Majors:** Apple ships zh-Hans (China), zh-Hant (Taiwan), and a distinct zh-HK; Microsoft ships zh-Hans and zh-Hant
  terminology + style guides; Google, Spotify, and Netflix all offer separate Simplified and Traditional (unverified for
  the latter three, web-evidenced, not in the pile). Everyone treats them as two locales, never one.
- **Tag convention:** use script subtags `zh-Hans` / `zh-Hant`, not region tags, as the base catalogs (region only if a
  zh-HK or zh-SG override is later needed). This matches Cmdr's base-preferred BCP-47 convention and the reference
  pile's own sibling-folder layout (`zh-Hans`, `zh-Hant`, `zh-CN`, `zh-TW`, `zh-HK`).
- **Recommendation:** ship `zh-Hans` (Simplified, Taiwan-norm-independent) first; add `zh-Hant` written to the Taiwan
  norm as a fast follow; treat `zh-HK` as a later optional override. `high` on the structure; the scope/priority is the
  David call flagged above.
- **Don't auto-convert one into the other.** Simplified↔Traditional is NOT a safe character-by-character mapping:
  one-to-many mappings (e.g. 干/乾/幹 all simplify to 干) and divergent term choices mean a naive conversion produces
  wrong words. Each variant is its own translation pass, cross-checked against that variant's macOS source.

### Tech-term strategy: established native term, Apple as top authority

- Chinese has mature, universally-understood native IT vocabulary, so prefer the established Chinese term over an
  English loan or a transliteration. macOS is the highest-authority source (what a user literally sees in Finder); use
  it to break ties, with Microsoft and GNOME as cross-checks.
- The main Simplified-vs-Traditional term differences beyond character shape (verified against the reference pile,
  2026-06-20): Trash is `废纸篓` (Simplified) but `垃圾桶` (Traditional); copy is `拷贝`/`拷貝`; move is `移动`
  (Simplified) vs `搬移` (Traditional, Apple's preferred); search is `搜索` (Simplified) vs `搜尋` (Traditional).
  Settings is `设置` (Simplified) vs `設定` (Traditional). Keep each variant's terms self-consistent against its own
  macOS source.

### Gender and inclusive language: inherently neutral

- Chinese has no grammatical gender on nouns or verbs, and no verb agreement. The written third-person
  pronouns 他/她/它 (he/she/it) differ only in writing and sound identical; UI rarely needs them because Cmdr addresses
  the user in second person (`你`/`您`, ungendered) and refers to files/items as things. `high`. No special handling
  needed; keep strings second-person or item-referring and gender never arises.

### Numerals, punctuation, and spacing

- **Use Arabic numerals (0-9)** for counts, sizes, and percentages, as macOS Chinese and all majors do; `Intl` produces
  these by default. Chinese numerals (一二三) are for prose/formal contexts, not UI counts. `high`.
- **Full-width CJK punctuation.** Chinese uses full-width punctuation: `，` `。` `：` `；` `？` `！`, and the
  corner-bracket quotes `「…」` (Traditional) or guillemet-style `“…”` plus `《…》` for titles. macOS Finder quotes
  filenames with `“…”` in Simplified and `「…」` in Traditional. Use full-width marks in Chinese running text; keep
  ASCII punctuation only inside brand words and code-like tokens. `high`.
- **No spaces between Chinese characters**, but insert a thin/normal space between Chinese text and adjacent Latin brand
  words or numbers where it aids readability (common house style; follow what reads cleanly against the
  `{placeholder}`).

## Terminology and glossary

Format per term: `chosen (Simplified / Traditional) · sources · confidence`. Sources are read to decide the term, never
copied verbatim (Apple/Microsoft copyrighted; GNOME GPL). Top source is macOS; Microsoft and GNOME cross-check. Evidence
verified against the reference pile (`_ignored/i18n/zh-CN`, `zh-TW`, `zh-HK`) on 2026-06-20.

| English term  | Simplified (zh-Hans) | Traditional (zh-Hant)     | Notes                                                                                       |
| ------------- | -------------------- | ------------------------- | ------------------------------------------------------------------------------------------- |
| file          | 文件                 | 檔案                      | macOS. Note: Simplified `文件` = file; Traditional uses `檔案`. `high`.                     |
| folder        | 文件夹               | 檔案夾 (TW) / 資料夾 (HK) | macOS. TW vs HK split is real; ship TW norm for zh-Hant. `high`.                            |
| copy          | 拷贝                 | 拷貝                      | macOS Finder. Imperative on buttons. `high`.                                                |
| move          | 移动                 | 搬移                      | macOS (Apple prefers `搬移` in Traditional). `high`.                                        |
| delete        | 删除                 | 刪除                      | macOS. `high`.                                                                              |
| open          | 打开                 | 打開                      | macOS. `high`.                                                                              |
| cancel        | 取消                 | 取消                      | macOS. Same both scripts. `high`.                                                           |
| Trash         | 废纸篓               | 垃圾桶                    | macOS. Real term split (not just character shape). `high`.                                  |
| eject         | 推出                 | 退出                      | macOS (`推出` Simplified, `退出` Traditional). Verify against Cmdr's eject context. `high`. |
| search        | 搜索                 | 搜尋                      | macOS. `high`.                                                                              |
| settings      | 设置                 | 設定                      | macOS. `high`.                                                                              |
| volume (disk) | 宗卷                 | 卷宗                      | macOS (mounted-disk sense, NOT audio loudness `音量`). `high`.                              |
| tab           | 标签页               | 標籤頁                    | macOS. `high`.                                                                              |
| new folder    | 新建文件夹           | 新增檔案夾                | macOS. `high`.                                                                              |

Pane, listing, transfer, bookmark, viewer: triangulate during the first pass and record here with sources + confidence.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`.

## Plurals

CLDR category: **`other` only** (verified with `new Intl.PluralRules('zh')` and `'zh-Hant'`, 2026-06-20). Chinese has no
grammatical number on nouns; one form covers one and many, and counting uses measure words (classifiers), not plural
inflection.

- Every ICU plural message needs only the `other` branch for both scripts. `desktop-i18n-plural` requires the categories
  the language needs; for Chinese that's just `other`.
- Write the `other` branch to read naturally for any count, including 1. Counted nouns usually want a measure word: a
  natural counted string is `{count} 个项目` (Simplified) / `{count} 個項目` (Traditional) "{count} items" rather than
  pluralizing the noun. Mind the measure word per noun.

## Notes and decisions

- **No letter case; the sentence-case rule is moot for Chinese text.** Han characters are unicameral. Just keep Latin
  brand words (Cmdr, macOS) as-is.
- **Each script is its own pass.** Never machine-convert Simplified↔Traditional (one-to-many mappings + divergent
  terms); cross-check each variant against its own macOS source.
- **Quotation marks:** Simplified uses `“…”`; Traditional uses `「…」` (and `『…』` nested). Follow the variant's macOS
  Finder pattern when quoting filenames.

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Chinese rarely needs apostrophes, but any in a loanword or English fragment must be doubled.
- Keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  [`../guides/i18n-translation.md`](../../guides/i18n-translation.md) and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in [glossary.md](glossary.md). Read it before translating and add to it as
you settle terms, each sourced from the reference pile (`_ignored/i18n/zh/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
