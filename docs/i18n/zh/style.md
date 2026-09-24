# Chinese (zh) translation style guide

Working notes for translating Cmdr into Simplified Chinese. Read `../README.md` for how this fits the translation
process, and the app-wide `docs/style-guide.md` for the English voice these notes carry into Chinese. Term rulings live
in `terms.json` (keyed by the shared `../concepts.json` plus this locale's `concepts-proposed.json`), their rationale in
`decisions.md`, and open questions in `review-queue.md`.

Chinese is a tier-1 well-localized language: Apple (Finder), Microsoft, Google, Spotify, and Netflix all ship both
script variants, so triangulation evidence is strong. Sources mined for this guide: macOS Finder/AppKit strings in zh-CN
(Simplified), zh-TW and zh-HK (Traditional), plus the Microsoft zh-Hans and zh-Hant terminology and style guides, and
the GNOME Nautilus / Xfce Thunar zh-CN/zh-TW catalogs.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Script**: this catalog is Simplified (`zh`); Traditional is the separate `zh-Hant` translation. Never convert
  between them: the vocabulary differs (`拷贝` / `拷貝` is shared, but select is `选择` here and `選取` there, Trash is
  `废纸篓` here and `垃圾桶` there).
- **Address**: `你` everywhere, licensing and billing included (macOS zh-CN has zero `您`). The one `您` is the email
  salutation Cmdr drafts for the user to send (`licensing.dialog.mailtoBody`).
- **Voice**: friendly, short, spoken modern Mandarin; calm, never alarmist. Never `失败` or `错误` as a label: "Couldn't
  X" → `无法 X`, a one-off miss → `没能 X`, a second clause after a wrapper that already said `无法` → `没法`, a stopped
  operation → `无法完成`, "Something went wrong" → `出了点问题`. `错误` survives only in the feature name `错误报告`. No
  apology in a notice that reports a deliberate choice.
- **Demonstratives**: the spoken `这个` / `这项` / `这次` over the written `此` / `该`; `此` only where a terse label
  already settled it (`此驱动器` in the drive-index family, `此设备` / `此服务器` busy tooltips).
- **Buttons and menu items**: a bare verb, no politener (`拷贝`, `移动`, `打开`, `删除`, `取消`). Progress lines `正在…`
  (`正在扫描…`, `正在连接到 {name}…`); results and state badges the perfective `已…` (`已拷贝`, `已存档`, `已暂停`),
  except a state the user didn't cause (`在附近发现`). A tool line pairs `正在…` / `已…` on one verb phrase.
- **Apple names follow macOS zh-CN**, even when a `@key` description says otherwise: `快速查看`, `钥匙串`, `程序坞`,
  `显示简介`, `上层文件夹`, `个人收藏`, `宗卷`, `推出`, `废纸篓`, `聚焦`, `磁盘工具`, `iCloud 云盘`, `“文本编辑”`,
  `“预览”`. The one deliberate exception: Finder stays Latin (`在 Finder 中显示`), never `访达`. On a phone, Android's
  own Chinese wins (`USB 调试`, `“允许”`, `点按`, the classifier `部`).
- **Traps** (details in `terms.json`):
  - copy → `拷贝` (F5 and the clipboard); `复制` is Duplicate only.
  - operation → `操作` with the classifier `项` (`这项操作`, `{count} 项操作`); the queue `操作队列`, never `传输队列`;
    transfer `传输` only for a copy or move in flight.
  - dismiss → `关闭`, never `忽略`; clear → `清除`; Cmdr tidying its own leftovers → `清理掉` / `清掉`.
  - click → `点按` (never `点击` for a single click); double-click `双击`; right-click `右键点击`.
  - Enter / Escape keys stay Latin: `按 Enter 键…`, `按 Esc 键…`, never `回车键`.
  - save → `保存`, never `存储`; review (the approval gate) → `复查`, never `检查` (that's check) or `审核`.
  - view contents → `查看`; the View menu → `显示`; See why → `看看原因`.
  - put back from the trash → `放回原处`; undoing an AI rename → `恢复…原来的名称`; roll back → `回滚`; undo → `撤销`.
  - image indexing → `图像`, never `图片`; the user's pictures → `照片` (classifier `张`); a photo's place `拍摄地点`,
    never `位置` (file-system only).
  - archive (zip) → `压缩文件`; a chat's archive → `存档`; guest → `客人`; native (macOS's own) → `内建`.
- **Punctuation**: full-width `，。：；？！（）` in Chinese text; quote UI names, file names, and arbitrary runtime
  names with `“…”`; one ellipsis form, the single `…` (U+2026), even where the English writes `...`; paired verbs hug
  their slash (`固定/取消固定服务器`), while a spaced `/` is only for numeric fractions. A settings path in a sentence
  is quoted whole, separator mirroring the English: `在“设置 > 更新与隐私”中重新开启`. A menu in prose: `“帮助”菜单`.
- **Spacing**: no space between Han characters; one ASCII space between Han text and Latin words, digits, or a
  placeholder that may arrive Latin (`{countText} 个文件`, `已有 {duration} 没有进度`, `macOS 内建的…`); never a space
  against full-width punctuation. Inline `<strong>` / `<code>` quoting a UI element or command get a space on both
  sides; an `<em>` inside a clause gets none.
- **Numbers and plurals**: Arabic numerals; the `%` sign with no space (`已完成 {percentText}%`). CLDR `zh` has only
  `other`: write one branch that reads right for 1, and always a measure word (`个文件`, `项操作`, `台服务器`, `部手机`,
  `张照片`, `项个人收藏`). A list-final placeholder takes no measure word (`其他 App`).
- **Brand and tokens**: `Cmdr`, `macOS`, `GitHub`, `SMB`, `MTP`, `ADB`, `Safari`, `NAS` verbatim; `Ask Cmdr` names only
  the chat panel, and a sentence about what the AI does says `Cmdr` (or `AI` in `suggestedOps.*`). Never hand-translate
  a `{system_settings}`-style token. The Latin `App` stays in error, cloud, and AI copy; the scope word is `应用`.
- **ICU**: double every apostrophe in ICU values; the RAW families (`errors.*`, `menu.*`, `licensing.windowTitle.*`,
  `main.instanceLock.*`) keep single ones. Keys whose English is byte-identical must stay identical in Chinese
  (`desktop-i18n-term-consistency`), and an `*Aria` key must contain its visible label verbatim.

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
`../script-decisions.md`; the Traditional terminology rulings live in `../zh-Hant/style.md` and `../zh-Hant/terms.json`
(rationale in `../zh-Hant/decisions.md`), not here. The structure and evidence below stand.

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
  `../zh-Hant/terms.json`.

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
- **A list-final placeholder item takes no measure word, and the joiner is never in the string.**
  `Intl.ListFormat('zh')` joins with `、` and a trailing `和`, adding no spaces of its own
  (`Preview、Warp、Photos和其他 App`, verified with Node 24, 2026-09-16). So a string that stands in for the tail of
  such a list (`errors.eject.otherApps` = `其他 App`) is a bare noun phrase: `其他` modifies the noun directly, and
  adding `几个` would invent a count the English doesn't have. The sentence holding `{apps}` still spaces its own side
  (`{apps} 还在使用…`), because the joined phrase ends in a Latin word.
- **Pre-formatted placeholders are often Latin, so space them on both sides.** Several placeholders arrive already
  formatted and unlocalized (`{duration}` = `45s` / `2m 30s` / `1h 5m`, sizes, speeds), so they land as Latin text mid
  sentence: write `已有 {duration} 没有进度`, not `已有{duration}没有进度`. The whole catalog does this
  (`剩余约 {duration}`, `{countText} 个文件`).
- **Ellipsis: always the single full-width `…` (U+2026), whatever the English source writes.** Chinese typography uses
  `…`, not three ASCII dots, so every status and label renders `…` (`正在发送…`, `正在加载…`, `正在取消…`). This is a
  deliberate locale-wide normalization, not byte-copying the source; the doubled literary `……` isn't used either, even
  in prose tooltips.

## Terminology

Every term ruling lives in `terms.json`, one entry per concept: the chosen form, accepted variants, usage forms, avoided
forms with the reason, confidence, and sources, plus `exceptions` for keys that rightly deviate. `pnpm i18n:brief` hands
a translator the entries a batch needs; `pnpm i18n:check-termbase` holds the catalog to them. Sources are read to decide
a term, never copied verbatim (Apple and Microsoft are copyrighted; GNOME is GPL). Top source is macOS zh-CN; Microsoft
and GNOME cross-check. The reference pile is `_ignored/i18n/zh/` in the main clone.

**Simplified only.** The Traditional rendering of each term is a separate decision with its own evidence and its own
Taiwan-vs-Hong-Kong rulings, recorded in `../zh-Hant/terms.json`. A second column here would be a copy that rots:
several Traditional terms are deliberately NOT Apple's zh-TW word.

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, ADB, Safari, Tauri, Rust, Svelte, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`. Apple feature names Apple itself localizes are NOT on it: Quick Look is
`快速查看`, Keychain `钥匙串`, the Dock `程序坞`.

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
  `Finder`。证据与例外见 `decisions.md` 里的「原生菜单」一节。
- **Click is `点按`, not `点击`.** macOS `zh-CN` uses `点按` exclusively (0 occurrences of `点击` across Finder, AppKit,
  and SystemSettings, verified against the reference pile, 2026-08-21), and onboarding writes
  `点按下方的 <strong>…</strong>`. The catalog carries no single-click `点击`, the noun included (`每一次点按…`). The
  compounds follow the Tier-3 file managers: double-click `双击`, right-click `右键点击`.
- **Toast strings that follow a colon carry the wrapper's verb, so don't repeat it.** Several error values are dropped
  into a wrapper key (`无法推出 {volumeName}：…`, `无法断开连接：…`) and read as the sentence AFTER the colon. Write
  them to continue that sentence, and pick a different construction for the second clause (`没法断开它` instead of a
  second `无法断开连接`). Examples and the per-term evidence: `decisions.md` § Eject / disconnect error copy.
- **Quote a menu name in running text, don't write a menu path.** In prose, wrap the menu's name in `“…”` and follow it
  with `菜单`: `请从“帮助”菜单发送一份新报告。` (the shape `settings.updates.errorReports.description` already uses).
  The bold `帮助 > 发送反馈…` path shape is reserved for the step-by-step onboarding instructions. Menu names must match
  `menu.*` exactly (Help = `帮助`), so a copy edit can't drift the two apart.
- **`查看` is "look at the contents", `显示` is the View menu.** macOS `zh-CN` splits them, and so does the catalog:
  `menu.bar.view` = `显示` (change the view), while reading a report, a file, or an info panel is `查看`. Picking the
  wrong one makes a button sound like a view-mode switch. Evidence: `decisions.md` § "给已发送的错误报告补充备注（`errorReporter.amend.*`、`errorReporter.amendedToast.message`、`errorReporter.autoSentToast.viewOrAddNotes`）".
- **No letter case; the sentence-case rule is moot for Chinese text.** Han characters are unicameral. Just keep Latin
  brand words (Cmdr, macOS) as-is.
- **Each script is its own pass.** Never machine-convert Simplified↔Traditional (one-to-many mappings + divergent
  terms); cross-check each variant against its own macOS source.
- **Undo/skip reason lists share one sentence shape: `保留了 X：原因。`** Every family that undoes something and then
  reports what it deliberately left alone (`askCmdr.renameUndo.skipReason.*`, `fileOperations.cancelRollback.reason.*`)
  renders its bulleted reasons this way, with a bare `{name}` (no `“…”`, unlike prose strings) and a full-width colon.
  Break the shape only for a reason that is NOT a deliberate choice (the `failed.*` arms, where the drive turned the
  undo down). Some of these keys carry byte-identical English across families, so `desktop-i18n-term-consistency`
  requires identical values; per-key evidence and the pairs that are locked together: `decisions.md`
  § 回滚结束后的提示条.
- **`Ask Cmdr` names the chat PANEL; everywhere else the subject is `Cmdr`, and in the Suggested ops dialog it's `AI`.**
  Follow the English exactly: keep `Ask Cmdr` where it's the panel's own name (its title, the View-menu item, the
  palette command, the settings section, the on/off copy) and where a sentence points at that settings section
  (`Ask Cmdr 设置`, `Ask Cmdr 部分`); write bare `Cmdr` when the sentence describes what the AI does; write `AI` in the
  four `suggestedOps.*` strings, which have to stay distinguishable from the neighbouring `Cmdr 掌握的信息`. ❗ Don't
  "restore" `Ask Cmdr` as a sentence subject: that's the exact regression a copy sweep once had to undo. Per-key
  evidence: `decisions.md` § "AI 文案改写：主语从 “Ask Cmdr” 换成 `Cmdr` / `AI`".
- **Quotation marks:** this catalog quotes filenames with `“…”`, following macOS zh-CN. Traditional uses corner brackets
  instead, which is one more reason a converted catalog reads wrong; its rule is in `../zh-Hant/style.md`.
- **Ask Cmdr tool-line labels are a `正在…` / `已…` pair, with `查看` for reading contents.** Every
  `askCmdr.tool.*.doing` opens with `正在` and its `.done` twin with `已`, on the same verb phrase (`正在查看文件内容` /
  `已查看文件内容`). Reading what's inside something is `查看…内容` (or `读取` for the photo facts tool); keep `显示`
  for the View menu. Per-term evidence: `decisions.md` § Ask Cmdr inspect-file consent.
- **Android over ADB: translate the Android-side feature names, keep the command names Latin.** A Chinese Android phone
  shows `开发者选项 > USB 调试`, so `USB debugging` is `USB 调试` and `platform tools` is `平台工具`, while the tool and
  command names (`ADB`, `adb`, `Android SDK`, `Homebrew`) stay Latin, matching Google's own zh-CN docs. Same rule as
  Apple feature names: what the user's own device shows wins. Sources: `terms.json` `usb-debugging` and `phone`.
- **A settings text field that may be left blank opens with `留空则…`.** Settled in
  `settings.askCmdr.interactiveModel.description`, reused for `settings.fileOperations.adbBinaryPath.description`.
- **A grayed-out menu item's “busy” form is the base label plus `（占用中）`.** Settled in `menu.volume.ejectBusy`
  (`推出（{name}）（占用中）`) and reused verbatim by `menu.volume.disconnectBusy`,
  `menu.volume.forgetSavedPasswordBusy`, and `menu.volume.forgetServerBusy`. Keep the base wording byte-identical to the
  enabled item (`menu.network.disconnect`, `menu.network.forgetSavedPassword`, `menu.network.forgetServer`), append the
  marker with full-width parens and no space, and never invent a second marker: the two states have to read as one item.
- **A table column heading that holds a date or time carries the time noun.** Apple's Chinese column headings are
  `上次打开日期`, `上次使用时间`, `修改日期`, so "Last used" over a date column is `上次使用时间`, not the bare
  `上次使用` (which Apple uses mid-sentence). Evidence: the `decisions.md` section on the servers hub.
- **Status labels are `已…` adjectives, unless the user didn't cause the state.** `已连接` / `已保存` / `已退出登录`
  read as one set in a Status column. A state the app merely observed takes a plain verb phrase instead (`在附近发现`),
  because `已…` would make it sound like an action that just finished.
- **Paired-verb command labels hug the slash: `固定/取消固定服务器`.** The spaced `/` in the catalog is reserved for
  numeric fractions (`{currentText} / {maxText}`).
- **`网络` is the volume-switcher GROUP, `服务器` is the row inside it.** Don't let the two drift back together; the row
  and the Keyboard-shortcuts scope both say `服务器`.
- **A photo's place is `拍摄地点`, never `位置`.** `位置` is reserved for a file-system location in this catalog
  (`目标位置`, `原来的位置`); Photos.app calls photo places `地点`. Camera EXIF is `相机信息` (Photos.app `无相机信息`).
- **“There's nothing to X” is a fixed Apple pattern: `没有要 X 的内容`.** `There's nothing to send.` →
  `没有要发送的内容。`, `There is nothing to print.` → `没有要打印的内容。` (AppIntents and Printing `.loctable`, macOS
  26.6.2 / 25G83, 2026-09-07). Reach for it whenever a string says there is nothing for the user to supply, rather than
  inventing a chattier `没什么可…的`.
- **A menu item naming an app writes the name bare: `打开 Cmdr`, never `打开“Cmdr”`.** Apple quotes a name in a menu
  item only when it substitutes an arbitrary one at runtime (Dock's `隐藏“%@”`, the catalog's own
  `menu.context.copyNamed` = `拷贝“{name}”`). When the name is fixed in the string, Apple writes it unquoted: `隐藏访达`
  / `退出访达` (Finder `MenuBar`), `隐藏Safari浏览器` / `关于Safari浏览器` (Safari `MainMenu`) (verified on macOS
  26.6.2, 25G83, 2026-09-09). Cmdr's whole `menu.app.*` and `menu.dock.*` block follows that: `关于 Cmdr`, `隐藏 Cmdr`,
  `退出 Cmdr`, `打开 Cmdr`, with a space before the Latin brand.
- **A progress heading is `正在…`, and the reconnect one is `正在重新连接到 {name}…`.** It shares the sentence shape of
  `正在连接到 {name}…`, so first connect and auto-reconnect read as two states of one thing. Evidence in `decisions.md`
  § 自动重连的面板标题.
- **An inline `<field></field>` is a text box sitting mid-sentence, so put it where Chinese wants the object and keep
  the sentence reading as one line.** `onboarding.stepBeta.checklist.email` renders an input plus its Save button inside
  the sentence, so the words before and after it have to hold together around a box:
  `留下你的邮箱 <field></field> 就能偶尔收到一点更新和提问`. Spaces on both sides of the tag, as with any inline widget.
- **Space an inline tag by what it WRAPS, not by the tag name.** A `<strong>` or `<code>` block quoting a UI element or
  a command gets a space on both sides, even between Han characters: `点按下方的 <strong>打开{systemSettings}</strong>`,
  `在列表中找到 <strong>Cmdr</strong> 并把它打开`, `启用 <code>brew install cmdr</code>`. That space is what keeps a
  bolded option name from fusing with the noun after it (`… AI</strong> 选项`). An `<em>` emphasizing a word INSIDE a
  clause takes none, because the sentence would read wrong broken apart: `如果<em>不是</em>有意的`,
  `既想要隐私<em>又</em>想要一个像样的模型`. Never space against CJK punctuation (`，。：（`) either way.
- **A `…summary` key beside a switch is a HALF-LINE and must not wrap; the long version lives in the sibling `…desc`.**
  Match the desc's terminology exactly, then cut: `占用 1 GB 空间，加快搜索，显示文件夹大小`. Chinese runs short here,
  so the room is real, but a summary that grows past ~24 Han characters starts wrapping in the real layout.

### ICU mechanics (catalog-level, easy to miss)

- Double every apostrophe in a value (`'` becomes `''`); ICU treats a lone `'` as an escape and silently swallows text.
  Chinese rarely needs apostrophes, but any in a loanword or English fragment must be doubled.
- Keep every `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in
  `docs/guides/i18n-translation.md` and `apps/desktop/src/lib/intl/messages/CLAUDE.md`.

## Termbase and decisions

Read the terms a batch needs through `pnpm i18n:brief`, and write back as you settle terms (`../termbase.md` § Writing
back): a ruling goes into `terms.json`, rationale worth keeping into a `decisions.md` section whose heading cites the
keys in backticks, and anything only a native reviewer can settle into `review-queue.md`. Source every term from the
reference pile (`_ignored/i18n/zh/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`). Never guess a term.
