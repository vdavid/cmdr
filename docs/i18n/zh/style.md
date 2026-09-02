# Chinese (zh) translation style guide

Working notes for translating Cmdr into Chinese. Read `../README.md` for how this fits the translation process, and the
app-wide `docs/style-guide.md` for the English voice these notes carry into Chinese.

Chinese is a tier-1 well-localized language: Apple (Finder), Microsoft, Google, Spotify, and Netflix all ship both
script variants, so triangulation evidence is strong. Sources mined for this guide: macOS Finder/AppKit strings in zh-CN
(Simplified), zh-TW and zh-HK (Traditional), plus the Microsoft zh-Hans and zh-Hant terminology and style guides, and
the GNOME Nautilus / Xfce Thunar zh-CN/zh-TW catalogs.

This is a living doc, and capturing is your job. When you discover a convention, gotcha, or ruling that wasn't already
written, add it here.

## Decisions to confirm with David

These are calls a translator can't make alone. The rest of this guide assumes them.

- **Which script variant(s) to ship: RESOLVED to BOTH.** This catalog is Simplified; Traditional ships separately as
  `zh-Hant`, with its own guide at `../zh-Hant/style.md`. The two are independent full translations that never inherit
  from each other (the script guard blocks it in all three layers), so **never auto-convert one into the other**: the
  vocabulary differs, not just the character shapes. See the script decision point below and `../script-decisions.md`.
  No longer open.
- **Formal vs neutral "you" (`您` vs `你`): RESOLVED to `你`, with no register carve-out** (consumer-brand evidence, and
  Apple's Simplified copy uses `你` in its licence agreement too; see Formality and `../formal-informal-decisions.md`).
  No longer open.

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
  `../formal-informal-decisions.md`.
- **`你` holds in licensing and billing too; there is no register carve-out for them.** The boundary that exists in
  Chinese runs between an AGREEMENT BODY (the clause-numbered contract a user clicks through) and the CHROME around it
  (buy, activate, renew, licence details, expiry notices), not between "legal-ish" and "ordinary" copy. Cmdr ships no
  agreement body in the catalog at all: every `licensing.*` string is chrome, so every one of them is `你`. Apple
  doesn't even split at that boundary in Simplified: its purchase chrome is `你` (102 instances, zero `您`, across
  `AppStoreKit.framework` and `App Store.app` `zh_CN.lproj`) and so is the macOS Tahoe 26 software licence agreement
  itself (353 `你`, zero `您`, `Setup Assistant.app` `zh_CN.lproj/OSXSoftwareLicense.rtf`) (verified on macOS 26.6.2,
  `plutil` / `textutil` over the live bundles, 2026-08-30).
- **Trap when re-checking this:** the one `您`-heavy legal text on a stock Mac is Feedback Assistant's `License.rtf` (93
  `您`, zero `你`), a separately drafted click-through agreement stamped `EA1920`, 2024-09-09. Its file mtime matches
  every other bundle's, so mtime won't date it; read the agreement's own footer stamp. Don't let that one file talk you
  back into a formal-register island.
- **The one real `您`: a salutation Cmdr puts in the USER's mouth.** `licensing.dialog.mailtoBody` pre-fills an email
  the user sends to Cmdr's support address, so the addressee is us, not the reader, and `您好` is the ordinary Chinese
  business-letter opener there. That's a different axis from how Cmdr addresses its user, so the formality ruling
  doesn't reach it. `zh-Hant` independently kept the same single exception.
- **Buttons and menu items: bare verb, no politener.** macOS labels actions as plain verbs: `拷贝`/`拷貝` (copy),
  `移动`/`搬移` (move), `打开`/`打開` (open), `删除`/`刪除` (delete), `取消` (cancel). This is the correct register for
  Cmdr's buttons and menus: concise and direct, polite by default because a bare verb isn't rude in Chinese.

## Decision points

### Script: Simplified vs Traditional (the big one), and which region

**RESOLVED: ship both scripts as separate catalogs.** This one is Simplified (`zh`); Traditional is `zh-Hant`, written
to a pan-Traditional consensus that serves Taiwan, Hong Kong, and Macau from one catalog. Recorded in
`../script-decisions.md`; the Traditional terminology rulings live in `../zh-Hant/style.md` and
`../zh-Hant/glossary.md`, not here. The structure and evidence below stand.

- **Two written standards, not mutually substitutable.** Simplified Chinese (`zh-Hans`) is the standard in Mainland
  China and Singapore; Traditional Chinese (`zh-Hant`) is standard in Taiwan, Hong Kong, and Macau. They differ in
  character shapes AND, importantly, in vocabulary and term choices (not a font swap). Serving Simplified to a Taiwan
  user, or vice versa, is a recognized localization miss (a Hong Kong `zh-HK` browser locale wrongly falling back to
  `zh-CN` is a documented bug class). `high`.
- **Within Traditional, Taiwan and Hong Kong diverge on real terms**, so one `zh-Hant` catalog serving both had to rule
  on each split. Those rulings are `zh-Hant`'s to make and live in `../zh-Hant/style.md`; don't restate them here.
- **Majors:** Apple ships zh-Hans (China), zh-Hant (Taiwan), and a distinct zh-HK; Microsoft ships zh-Hans and zh-Hant
  terminology + style guides; Google, Spotify, and Netflix all offer separate Simplified and Traditional (unverified for
  the latter three, web-evidenced, not in the pile). Everyone treats them as two locales, never one.
- **Tag convention:** use script subtags `zh-Hans` / `zh-Hant`, not region tags, as the base catalogs (region only if a
  zh-HK or zh-SG override is later needed). This matches Cmdr's base-preferred BCP-47 convention and the reference
  pile's own sibling-folder layout (`zh-Hans`, `zh-Hant`, `zh-CN`, `zh-TW`, `zh-HK`).
- **Shipped shape:** `zh` (Simplified) and `zh-Hant` (Traditional) both ship, as independent full translations. `zh-HK`
  stays a later optional overlay of `zh-Hant`, wanted only if Hong Kong readers ask for the handful of terms `zh-Hant`
  decided the Taiwan way.
- **Don't auto-convert one into the other.** Simplified↔Traditional is NOT a safe character-by-character mapping:
  one-to-many mappings (e.g. 干/乾/幹 all simplify to 干) and divergent term choices mean a naive conversion produces
  wrong words. Each variant is its own translation pass, cross-checked against that variant's macOS source.

### Tech-term strategy: established native term, Apple as top authority

- Chinese has mature, universally-understood native IT vocabulary, so prefer the established Chinese term over an
  English loan or a transliteration. macOS is the highest-authority source (what a user literally sees in Finder); use
  it to break ties, with Microsoft and GNOME as cross-checks.
- Simplified and Traditional differ in TERMS, not just character shapes (Trash is `废纸篓` here but `垃圾桶` there; save
  is `保存` vs `儲存`; search is `搜索` vs `搜尋`; settings is `设置` vs `設定`). Keep this catalog self-consistent
  against its own zh-CN macOS source; the Traditional side of every such pair is `zh-Hant`'s call, recorded in
  `../zh-Hant/glossary.md`.

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
- **Pre-formatted placeholders are often Latin, so space them on both sides.** Several placeholders arrive already
  formatted and unlocalized (`{duration}` = `45s` / `2m 30s` / `1h 5m`, sizes, speeds), so they land as Latin text mid
  sentence: write `已有 {duration} 没有进度`, not `已有{duration}没有进度`. The whole catalog does this
  (`剩余约 {duration}`, `{countText} 个文件`).

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Sources are read to decide the term, never copied verbatim
(Apple/Microsoft copyrighted; GNOME GPL). Top source is macOS zh-CN; Microsoft and GNOME cross-check. Evidence verified
against the reference pile (`_ignored/i18n/zh-CN`) on 2026-06-20.

**Simplified only.** The Traditional rendering of each term is a separate decision with its own evidence and its own
Taiwan-vs-Hong-Kong rulings, and it lives in `../zh-Hant/glossary.md`. A second column here would be a copy that rots:
several Traditional terms are deliberately NOT Apple's zh-TW word.

| English term  | Simplified (zh) | Notes                                                                        |
| ------------- | --------------- | ---------------------------------------------------------------------------- |
| file          | 文件            | macOS. `high`.                                                               |
| folder        | 文件夹          | macOS. `high`.                                                               |
| copy          | 拷贝            | macOS Finder. Imperative on buttons. `high`.                                 |
| move          | 移动            | macOS. `high`.                                                               |
| delete        | 删除            | macOS. `high`.                                                               |
| open          | 打开            | macOS. `high`.                                                               |
| cancel        | 取消            | macOS. `high`.                                                               |
| Trash         | 废纸篓          | macOS. A real term split from Traditional, not just character shape. `high`. |
| eject         | 推出            | macOS. `high`.                                                               |
| search        | 搜索            | macOS. `high`.                                                               |
| settings      | 设置            | macOS. `high`.                                                               |
| volume (disk) | 宗卷            | macOS (mounted-disk sense, NOT audio loudness `音量`). `high`.               |
| tab           | 标签页          | macOS. `high`.                                                               |
| new folder    | 新建文件夹      | macOS. `high`.                                                               |

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

- **原生菜单跟随 Finder 的用词，而不是目录里的旧用词。**
  macOS 有对应项时以它为准（`显示`、`上层文件夹`、`个人`、`显示简介`）。唯一有意保留的分歧是 Apple 的「访达」，Cmdr 仍写
  `Finder`。证据与例外见 `glossary.md` 里的「原生菜单」一节。
- **Click is `点按`, not `点击`.** macOS `zh-CN` uses `点按` exclusively (0 occurrences of `点击` across Finder, AppKit,
  and SystemSettings, verified against the reference pile, 2026-08-21), and onboarding already writes
  `点按下方的 <strong>…</strong>`. The catalog still has ~10 stragglers on `点击`; write `点按` in new strings and
  converge the rest opportunistically.
- **Toast strings that follow a colon carry the wrapper's verb, so don't repeat it.** Several error values are dropped
  into a wrapper key (`无法推出 {volumeName}：…`, `无法断开连接：…`) and read as the sentence AFTER the colon. Write
  them to continue that sentence, and pick a different construction for the second clause (`没法断开它` instead of a
  second `无法断开连接`). Examples and the per-term evidence: `glossary.md` § Eject / disconnect error copy.
- **Quote a menu name in running text, don't write a menu path.** In prose, wrap the menu's name in `“…”` and follow it
  with `菜单`: `请从“帮助”菜单发送一份新报告。` (the shape `settings.updates.errorReports.description` already uses).
  The bold `帮助 > 发送反馈…` path shape is reserved for the step-by-step onboarding instructions. Menu names must match
  `menu.*` exactly (Help = `帮助`), so a copy edit can't drift the two apart.
- **`查看` is "look at the contents", `显示` is the View menu.** macOS `zh-CN` splits them, and so does the catalog:
  `menu.bar.view` = `显示` (change the view), while reading a report, a file, or an info panel is `查看`. Picking the
  wrong one makes a button sound like a view-mode switch. Evidence: `glossary.md` § amending a sent error report.
- **No letter case; the sentence-case rule is moot for Chinese text.** Han characters are unicameral. Just keep Latin
  brand words (Cmdr, macOS) as-is.
- **Each script is its own pass.** Never machine-convert Simplified↔Traditional (one-to-many mappings + divergent
  terms); cross-check each variant against its own macOS source.
- **Undo/skip reason lists share one sentence shape: `保留了 X：原因。`** Every family that undoes something and then
  reports what it deliberately left alone (`askCmdr.renameUndo.skipReason.*`, `fileOperations.cancelRollback.reason.*`)
  renders its bulleted reasons this way, with a bare `{name}` (no `“…”`, unlike prose strings) and a full-width colon.
  Break the shape only for a reason that is NOT a deliberate choice (the `failed.*` arms, where the drive turned the
  undo down). Some of these keys carry byte-identical English across families, so `desktop-i18n-term-consistency`
  requires identical values; per-key evidence and the pairs that are locked together: `glossary.md`
  § 回滚结束后的提示条.
- **Quotation marks:** this catalog quotes filenames with `“…”`, following macOS zh-CN. Traditional uses corner brackets
  instead, which is one more reason a converted catalog reads wrong; its rule is in `../zh-Hant/style.md`.
- **Ask Cmdr tool-line labels are a `正在…` / `已…` pair, with `查看` for reading contents.** Every
  `askCmdr.tool.*.doing` opens with `正在` and its `.done` twin with `已`, on the same verb phrase (`正在查看文件内容` /
  `已查看文件内容`). Reading what's inside something is `查看…内容` (or `读取` for the photo facts tool); keep `显示`
  for the View menu. Per-term evidence: `glossary.md` § Ask Cmdr inspect-file consent.
- **A photo's place is `拍摄地点`, never `位置`.** `位置` is reserved for a file-system location in this catalog
  (`目标位置`, `原来的位置`); Photos.app calls photo places `地点`. Camera EXIF is `相机信息` (Photos.app `无相机信息`).

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Chinese rarely needs apostrophes, but any in a loanword or English fragment must be doubled.
- Keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/zh-CN/`; recipes in
`docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
