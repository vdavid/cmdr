# Vietnamese (vi) translation style guide

Working notes for translating Cmdr into Vietnamese. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice these notes carry into Vietnamese. Term rulings live in
`terms.json` (keyed by the shared `../concepts.json`), their rationale in `decisions.md`, and open questions in
`review-queue.md`.

Vietnamese is well-resourced: the pile (`_ignored/i18n/vi/`) has macOS Finder/AppKit/SystemSettings, MS terminology +
style guide, GNOME Nautilus, Xfce Thunar, KDE Dolphin, and Total Commander, so both UI families are covered. Most terms
reach `high`. Evidence verified against the pile on 2026-06-20, source list re-checked 2026-07-21.

## Digest

The must-know rules; the rest of this file elaborates them.

- **Address**: `bạn`, the neutral software pronoun (macOS, the MS vi style guide), and drop it wherever the sentence
  reads fine without it. Buttons and menu items are the bare verb with no pronoun (`Sao chép`, `Hủy`, `Mở`). David
  speaking in the first person is `mình` (onboarding step 3, `openBeta`); the USER speaking, as on a radio option, is
  `tôi` (`Có, tôi muốn AI`).
- **Voice**: friendly, concise, calm. Error copy states the problem and the next step and never uses `lỗi` or `thất bại`
  as a label: "Couldn't X" → `Không thể X` / `Không X được`; a gentle failed status → `Chưa hoàn tất được`; an outcome
  not yet proven → `Chưa xác nhận được`; "Something went wrong" → `Có gì đó không ổn`; "Here's what to try:" →
  `Bạn có thể thử:`. A flat present state takes `không`, "not yet" takes `chưa`; a thing that WILL happen (the Dock
  redraws next login, a retry will save) takes `chưa`, never a verdict of failure.
- **Diacritics**: always full, NFC. Never strip marks to save space. Tone placement is modern: `hủy`, `xóa`, `khóa`,
  `hòa` (never `huỷ`, `xoá`, `khoá`).
- **Capitalization**: sentence case in every title, label, and button; proper nouns keep theirs (`Thùng rác` when a
  string names the Trash location, `thùng rác` inside an action).
- **Menu-bar and Apple names follow the vi macOS**: `Tệp`, `Sửa`, `Xem`, `Đi`, `Cửa sổ`, `Trợ giúp`; Get Info →
  `Lấy thông tin`, Locked → `Đã khóa`, Quick Look → `Xem nhanh`, Keychain → `chuỗi khóa` (app `Truy cập chuỗi khóa`),
  Applications → `thư mục Ứng dụng`, System Settings → `Cài đặt hệ thống`, Disk Utility → `Tiện ích ổ đĩa`, First Aid →
  `Sửa nhanh`, Activity Monitor → `Giám sát hoạt động`, Preview → `Xem trước`, Force Quit → `Bắt buộc Thoát`, the Full
  Disk Access pane → `Quyền truy cập đầy đủ vào ổ đĩa`. Kept English: Finder, Dock, Terminal, Spotlight, Mission
  Control, TextEdit, Safari, `Ask Cmdr`. Localize whatever Apple localizes, whatever a `@key` description says; on a
  phone, Android's own vi wins (`Gỡ lỗi qua USB`, `Cho phép`, `nhấn vào`).
- **Punctuation**: follow the quoting style of the file you're in (most of the catalog mirrors EN's straight `"`; the
  curly `“…”` is for prose with no quoting neighbours, and a quoted Apple UI name keeps Apple's curly quotes). Keep the
  EN ellipsis glyph per key (`…` or `...`); no space before `%`; a Settings path is `Cài đặt › <mục>` except where EN
  writes `>`. No comma before `và` / `hoặc` in new lists. ICU values double a straight apostrophe; RAW families
  (`errors.*`, `menu.*`) don't.
- **Plurals**: CLDR `other` only. One `other` arm, the noun uninflected (`{countText} tệp`); ❌ never an English-shaped
  `one` / `=1` arm. `=0 {…}` is fine where the zero case says something different. A counted noun takes no `các`.
- **Placeholders**: `{name}`, `{path}`, `{volumeName}`, `{app}` stand bare, with no classifier before them (the value
  may already contain it). Don't point back at an uncontrolled placeholder with `nó` when two subjects are in play;
  repeat the noun (`chỉ mục của điện thoại`).
- **Sibling strings**: where EN varies one sentence across a set, translate the shared tail once and reuse it byte for
  byte; two keys with the same EN must read the same (`i18n-terms` checks it). One surface has one vi name even when EN
  has two (`bộ chọn ổ đĩa` for volume chooser and switcher).
- **Top traps** (details in `terms.json`):
  - show → `hiển thị`, never `hiện` (reads as "current"); show up → `xuất hiện`.
  - size → `kích cỡ`, never `kích thước` (`Cỡ chữ` for text size); download → `tải về`, never `tải xuống`.
  - search → `tìm kiếm` wherever EN says search; find / discover → `tìm`.
  - tab → `tab`; Finder tag → `thẻ`. Tag colors are Apple's `Lam`, `Lục`, `Tía`, never `Xanh dương` / `Xanh lá` / `Tím`.
  - delete → `xóa` (bytes gone, incl. Remove download); remove from a list → `gỡ bỏ`; Forget → `Quên`; dismiss →
    `Bỏ qua`.
  - eject → `tháo`; disconnect (deliberate) → `ngắt kết nối`; a dropped connection → `Đã mất kết nối`; unplug → `rút`
    (only when EN really says pull the cable).
  - operation → `thao tác` (queue `Hàng đợi thao tác`, log `Nhật ký thao tác`); transfer → `lần truyền`, narrow
    copy-or-move only; `di chuyển` is the Move operation alone, a loose "moving files" is `chuyển tệp`.
  - rollback and undo → `hoàn tác`; Finish rolling back → `Tiếp tục hoàn tác`; put back from the trash → `đưa trở lại`;
    old names back → `đặt lại tên cũ`.
  - crash report → `báo cáo sự cố`, error report → `báo cáo trục trặc`, bare "report" → `báo cáo`; `sự cố` alone is
    "problem" and never claims Cmdr quit; quit unexpectedly → `thoát bất ngờ`.
  - Back in the folder history → `Trở lại`; back to a screen or step → `Quay lại`.
  - click → `bấm` (never `nhấp`), press a key → `nhấn`; double-click → `bấm đúp`.
  - reach a SERVER → `không kết nối được tới`; reach a path or drive → `không thể truy cập`; respond (a machine) →
    `phản hồi`, answer (the user) → `trả lời`.
  - share → `mục chia sẻ`; symlink → `liên kết mềm`; extension → `đuôi tệp`; archive (zip) → `tệp nén`; disk image →
    `ảnh đĩa`, never bare `ảnh`; image (the feature) → `hình ảnh`, photos in a count → `ảnh`.
  - in the background → `ở chế độ nền` in a sentence, `Chạy nền` on a control; ❌ never `ngầm`.
  - `Ask Cmdr` names the panel only; prose about what the assistant does says `Cmdr`, about the model says `AI`.

## Voice and tone

Friendly, concise, active, calm, never alarmist, matching Cmdr's English voice. The Vietnamese Microsoft voice is
explicitly modern, "shorter and everyday words… concise and direct", avoiding "old-fashioned, too formal or archaic"
phrasing (MS style guide, verified 2026-06-20), a clean fit for Cmdr. Error messages stay calm and actionable: phrase
the problem and the next step, and don't use "lỗi" (error) or "thất bại" (failed) as a bare status label the way English
avoids "error"/"failed".

## Formality

- **No T-V split, so no formal/informal register choice.** Politeness in Vietnamese comes from pronoun choice and
  softening particles, not a grammatical formality tier. Since the app uses neutral `bạn` (or no pronoun), there's no
  per-sentence register decision like Polish/Slovak.
- **Action labels (buttons, menu items): bare verb, no pronoun.** macOS Vietnamese shows plain verbs: "Sao chép" (Copy),
  "Dán" (Paste), "Cắt" (Cut), "Mở" (Open), "Xóa" (Delete), "Hủy" (Cancel), "Di chuyển" (Move), "Lưu" (Save), "Tìm kiếm"
  (Search) (macOS AppKit, verified 2026-06-20). Vietnamese verbs don't conjugate, so the label is the verb. No
  imperative-vs-infinitive question (the language has neither inflection).
- **Sentences to the user: `bạn` or no subject, optionally softened.** "Bạn có chắc muốn xóa các tệp này?" (Are you sure
  you want to delete these files?) or the leaner "Xóa các tệp này?". The MS examples favor `bạn` in running guidance
  ("Bạn nên thường xuyên sao lưu tệp", verified 2026-06-20). Keep it short; Vietnamese readers skim.

## Decision points

- **Diacritics: mandatory, and the top technical risk (high).** Vietnamese is Latin script but uses stacked tone +
  vowel-quality marks (ẵ, ệ, ử, ợ…). Two failure modes to defend against:
  - **Font/rendering "tofu".** Many fonts lack the precomposed glyphs and render boxes. Cmdr respects the system font on
    macOS, which covers Vietnamese, but verify rendering of stacked marks during overflow/layout check; this is an
    app-rendering question, not just a translation one.
  - **Never strip diacritics to "save space" or dodge encoding.** Unmarked Vietnamese is ambiguous and can change
    meaning entirely (a missing mark flips the word). Always store and ship fully marked NFC Unicode. This is the single
    most important Vietnamese rule. (web sources, unverified, but universally stated.) Confidence: high.
- **Script: Latin (chữ Quốc ngữ), no decision.** Modern Vietnamese is Latin-based only; the historical chữ Nôm is not
  used. No script choice. Confidence: high.
- **Regional variant: one written standard (high).** Northern (Hanoi) and Southern (Saigon) Vietnamese differ in
  pronunciation and some vocabulary, but the WRITTEN standard is effectively unified; software ships one `vi`, no
  pt-BR/pt-PT-style matrix. A few lexical pairs differ (e.g. some everyday nouns), but UI/file-manager terms are shared.
  Don't build a variant matrix. Confidence: high.
- **Gender / inclusive language: a non-issue (high).** Vietnamese is analytic with no grammatical gender and no gendered
  verb/adjective agreement. `bạn` is gender-neutral. Nothing to engineer around. Confidence: high.
- **Capitalization: sentence case everywhere (high).** Vietnamese capitalizes only the first word and proper nouns in
  titles, labels, and buttons. English title case is wrong ("Hiển thị tệp ẩn", not "Hiển Thị Tệp Ẩn"). Matches Cmdr's
  sentence-case rule. Confidence: high.
- **Text expansion: plan for ~20-25% growth (high).** Vietnamese is isolating, so it spells out with separate words
  rather than affixes, and UI strings run longer than English. Overflow-check buttons and labels against the
  pseudolocale (`en-XA`). (web sources, unverified on exact %.) Confidence: high on the direction.

## Terminology

Every term ruling lives in `terms.json`, keyed by the concept IDs in `../concepts.json` (plus this locale's
`concepts-proposed.json` until the lead merges it): `chosen`, accepted forms, usage notes, forms to avoid with the
reason, a confidence (`confirmed` / `high` / `tentative`), and sources. Tier order is macOS (Tier 1) → Microsoft
(Tier 2) → the file-manager catalogs (Tier 3); a vendor's own vi UI (Apple, Android) beats a `@key` description, and
catalog consistency beats a pile-ideal form that would fork a term mid-catalog. Rationale worth more than a line sits in
`decisions.md` under a heading that cites its keys, and the term's `decision` field names that heading. Never guess a
term: mine the reference pile first (`../reference-pile/how-to-mine.md`), and sweep the installed OS when the pile is
silent (see the note below).

## Brand and do-not-translate

Keep verbatim: Cmdr, macOS, GitHub, SMB, MTP, Tauri, Rust, Svelte, Quick Look, plus the `{system_settings}`-style
tokens. The curated list (BRAND_WORDS + SYSTEM_TOKENS) is enforced by `desktop-i18n-dont-translate`; see
`apps/desktop/scripts/i18n-catalog-lib.ts`. macOS UI names Cmdr opens into should match what a Vietnamese macOS shows
("Thùng rác", "Cài đặt").

## Plurals

CLDR categories for `vi`: `other` only (verified with `new Intl.PluralRules('vi')`; GNOME's nplurals=1 agrees).
Vietnamese has no grammatical number, so one form covers all counts.

- **other**: every count. "{count} tệp" works for 0, 1, and 1,000,000, the noun never inflects.
- The `desktop-i18n-plural` check only requires `other` here, but still write the count into the string naturally; don't
  hardcode an English "1 file / N files" split. There is no singular/plural noun change to make.

## Notes and decisions

- **Menu gốc theo cách dùng từ của Finder, không theo catalog.** Chỗ nào macOS có tương ứng thì lấy của macOS
  (`Thư mục chứa`, `Nhà`, `Trở lại`, `Kích cỡ`, `tab`). Chi tiết: `decisions.md` § Menu gốc.
- **Numbers and dates come from the formatter layer.** Vietnamese uses a comma decimal and a period (or space) thousands
  separator (1.000 or 1 000); `formatNumber()`/`formatByteSize()` produce these from the locale. Never hardcode
  separators.
- **Spacing: words are space-separated like English**, but a Vietnamese "word" is often two syllables ("thư mục"); don't
  break inside a compound when wrapping. The renderer handles this; just don't manually insert breaks.
- **ICU mechanics** (catalog-level): double every apostrophe in a value (`'` becomes `''`) and keep every
  `{placeholder}` and `<tag>` verbatim. Full rules: the agent-handoff block in `docs/guides/i18n-translation.md` and
  `apps/desktop/src/lib/intl/messages/CLAUDE.md`.
- **The vi Total Commander files are lossily double-encoded; decode before mining them.**
  `_ignored/i18n/vi/total-commander/WCMD.LNG.utf8` and `WCMD.INC.utf8` hold UTF-8 bytes that were read as cp1252 and
  re-saved as UTF-8, so a plain grep for `nguồn`, `đích`, or `chờ` returns ZERO hits and the source looks empty (it
  isn't). Recover it in memory with `raw.encode('cp1252').decode('utf-8')`, keeping the C1 bytes (0x80–0x9F) that
  Python's cp1252 codec leaves unmapped: register an error handler that passes `chr(n)` through as byte `n`. Even then a
  few bytes were dropped by the original bad conversion, so about 700 characters stay unrecoverable (`ỏ` in `bỏ qua`,
  the initial `Đ`); read around the holes rather than trusting a single line. Don't write the decoded copy into the
  pile; decode to a scratch file.
- **Text expansion bites the queue-row status cell.** `fileOperations.transferProgress.stallNotice` is
  `Không có tiến triển trong {duration}` against English's `No progress for {duration}` (~3× the character count), and
  one key feeds two surfaces: the progress dialog and the narrow queue row, which otherwise shows `còn {duration}`. The
  row is the tighter of the two, so the translation has to fit it. Overflow-check that cell against the pseudolocale
  (`en-XA`); if it clips, shorten the string (for example `Đứng yên {duration}`) and accept that the dialog line gets
  the same short text.
- **Multipliers (`4x`, `100x`) spell out as `<số> lần`**: `4x slower` → `chậm hơn 4 lần`, with the compared thing
  trailing (`so với kết nối trực tiếp của Cmdr`). Vietnamese has no `x` multiplier notation in UI text, and no pile
  source attests one; `lần` is the standard counter. Prefer `chậm hơn N lần` over `chậm gấp N lần` when two things are
  being compared.
- **`sự cố` is "problem", not "crash": pick the verb that carries the quitting.** The pile shows `sự cố` used for
  ordinary problems the app survives (macOS Finder "Nếu bạn tiếp tục gặp sự cố…", AppKit "Đã có sự cố khi truy xuất…",
  verified 2026-08-23), so `gặp sự cố` is safe in a string that must NOT claim Cmdr quit. What claims quitting is the
  verb: `thoát bất ngờ` (macOS AppKit `AppKitErrors`). Keep that split, because the crash-dialog body now has three
  variants (`crashReporter.dialog .body.ended` / `.keptRunning` / `.unknown`) and only `.ended` may say the app quit.
  Same trap in the noun: `báo cáo sự cố` means "crash report", so a string that says just "a report" must read `báo cáo`
  alone.
- **Sibling copy variants share every sentence they can.** Where English varies only the first sentence across a set of
  keys (the three crash-dialog bodies), translate the shared tail ONCE and reuse it verbatim, so the dialog reads as one
  string with a swapped opener. Wording details and the settled values: `decisions.md` § Ba biến thể phần thân.
- **Eject / disconnect error copy sits AFTER a colon.** `errors.eject.*` is dropped into
  `Không thể tháo {volumeName}: …` or `Không thể ngắt kết nối: …`, so the wrapper already carries the "couldn't" part.
  Write only the reason plus the next step; don't restate the refusal. Terms and evidence: `decisions.md` § Lỗi khi tháo
  ổ đĩa / ngắt kết nối.
- **Một danh sách tên ứng dụng không làm động từ đổi dạng, nên đừng bịa dấu hiệu số.**
  `errors.eject.unmountRefusedByApp` và `…ByApps` chỉ khác nhau đúng một đại từ (`nó` / `chúng`); danh sách tên đứng
  ngay trước đã nói số rồi. Mục cuối của danh sách (`errors.eject.otherApps`) thì PHẢI có loại từ: `các ứng dụng khác`,
  vì `và ứng dụng khác` đọc thành "và một cái nữa". Bằng chứng: `decisions.md` § macOS từ chối tháo ổ đĩa.
- **disk image là `ảnh đĩa`, và không bao giờ rút gọn thành `ảnh`** (`ảnh` một mình là bức ảnh chụp). Kho tham chiếu
  không có chuỗi nào; nguồn Tier 1 nằm trong Disk Utility và `DiskImages.framework` của máy, phải quét `.loctable` mới
  thấy. `decisions.md` § macOS từ chối tháo ổ đĩa.
- **`di chuyển` is reserved for the Move operation.** When English uses a loose "moving files" that also covers copies
  and deletes, write the plain `chuyển tệp`; `di chuyển tệp` would narrow the sentence to one operation.
- **`rút` (unplug) has no pile source in Vietnamese** and rests entirely on catalog consistency (four shipped MTP
  strings). Use it only where the English genuinely tells someone to pull the cable, never for a connection reset.
- **Một gốc từ cho cả họ "add".** Mọi chỗ nói tới việc thêm ghi chú vào một báo cáo đã gửi đều đi từ `thêm`:
  `Thêm vào báo cáo sự cố của bạn` (tiêu đề), `Thêm vào báo cáo` (nút), `Đang thêm…` (đang chạy),
  `Đã thêm ghi chú vào báo cáo` (toast), `Không thể thêm ghi chú của bạn: {error}` (toast hỏng). Bằng chứng và các quyết
  định kèm theo: `decisions.md` § Thêm ghi chú vào báo cáo đã gửi.
- **Đừng lặp `của bạn` hai lần trong một câu ngắn.** Tiếng Anh rải "your" thoải mái; tiếng Việt thì nặng. Giữ `của bạn`
  ở chỗ nó mang thông tin (hoặc ở chỗ một chuỗi chị em đã dùng, để hai chuỗi khớp nhau) và bỏ ở chỗ quyền sở hữu đã hiển
  nhiên. Ví dụ `errorReporter.amendedToast.message`.
- **Nhắc tới một menu của ứng dụng thì viết `menu <Tên>`**, lấy tên đúng như `menu.bar.*` (macOS Finder `vi`):
  `từ menu Trợ giúp`. Catalog đã có sẵn câu này trong `settings.updates.errorReports.description`; dùng lại y hệt thay
  vì viết một biến thể mới.
- **"Back" has two right answers, and macOS draws the line.** Going back in the folder HISTORY is `Trở lại` (Finder Go >
  Back); returning to the previous SCREEN or step inside a flow is `Quay lại` (macOS Setup Assistant, the Apple ID and
  iCloud sheets). Don't flatten them: `decisions.md` § Rà soát trôi thuật ngữ toàn catalog.
- **Two report flows, two names.** A crash report is `báo cáo sự cố` (macOS Problem Reporter), an error report is
  `báo cáo trục trặc`. They sit next to each other as two toggles in Settings > Updates & privacy, so one word for both
  would make the panel unreadable.
- **`xóa` destroys, `gỡ bỏ` un-lists.** Delete (and anything that really erases bytes, like "Remove download") is `xóa`;
  taking an item off a list while it lives on is `gỡ bỏ`. macOS says `Xóa` for both because Finder never shows the two
  side by side; Cmdr does.
- **"Error" as a bare status cell is `Sự cố`; as a diagnostic prefix it's `Lỗi:`.** The `@key.description` of each key
  says which surface it is. Both are deliberate; see `decisions.md` § Giọng lỗi.
- **An English article that means "all of them" becomes `cả`.** English separates "Removed **the** N items" (everything)
  from "Removed N items" (only some) with an article alone; Vietnamese has no article, so two sibling strings collapse
  into one. Put `cả` in the complete one (`Đã đưa trở lại cả {countText} mục.`) and leave the partial one bare. ❌ Not
  `tất cả`: longer, and it reads like a select-all button. Example: `fileOperations.cancelRollback.done*` against
  `.some*`.
- **A named/counted reason set shares one sentence frame with its sibling set in another feature.** When two features
  list the same reasons (the item changed, Cmdr couldn't check, the folder has contents now, the drive turned it down),
  English writes them nearly identically, so Vietnamese has to read as one feature: reuse the `Giữ nguyên {name}: …` /
  `Giữ nguyên {countText} … : …` frame, and where the English strings are IDENTICAL the Vietnamese values must match
  word for word. The two sets today: `askCmdr.renameUndo.skipReason.*` and `fileOperations.cancelRollback.reason.*`;
  details and the `mục` vs `tệp` line: `decisions.md` § Toast sau khi hoàn tác thao tác đang chạy.
- **Ask Cmdr "looks inside" a file: `xem bên trong tệp`, and the three photo words stay apart.** The inspect tool and
  the consent copy share one root (`Đang xem bên trong tệp`, `có thể xem bên trong tệp mà bạn hỏi đến`). Around it:
  thumbnail is `hình thu nhỏ` (macOS + MS; not GNOME's `ảnh thu nhỏ`), a photo's camera is `máy ảnh`, and where it was
  taken is `vị trí chụp` (never bare `vị trí`, which the catalog uses for a file's path). When "photo" lands right next
  to "camera", write `một bức ảnh` so `ảnh` doesn't double up; elsewhere keep bare `ảnh`. Evidence: `decisions.md` § Ask
  Cmdr xem bên trong tệp.
- **`gỡ lỗi qua USB` and `bộ công cụ nền tảng Android` get translated, `adb` / `ADB` / `Android SDK` / `Homebrew`
  don't.** The `en` `@key.description` on `settings.fileOperations.adbEnabled.*` asks for both phrases the way the
  vendor's localized Android renders them, and Android's own Vietnamese UI translates both, so a Vietnamese reader
  looking for the phone toggle finds `Gỡ lỗi qua USB`. What stays verbatim is the command (`adb`), the acronym (`ADB`),
  and the packaged product names (`Android SDK`, `Homebrew`), matching how the sibling MTP strings keep quoted on-phone
  menu labels English while translating the prose around them.
- **The reference pile can be missing on the machine you're translating from.** The M1 agent box has no
  `_ignored/i18n/vi/` at all (it lives only on David's laptop). The fallback in
  `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" works and is Tier 1 all the same: mine the
  installed macOS bundles directly. On macOS 26 most strings sit in `.loctable` files that carry every language at once,
  so a scan is:
  `find /System/Library/{Frameworks,PrivateFrameworks,ExtensionKit} /System/Applications /Applications -name '*.loctable'`,
  then in Python `plistlib.load(f)['vi']` against `['en']` for the same keys. That's how the six terms above were
  sourced.
- **Dạng "(busy)" của một mục menu: giữ nguyên chữ của mục gốc rồi thêm ` (đang bận)`.** Bốn khóa dùng chung một dấu
  hiệu: `menu.volume.ejectBusy` (`Tháo ({name}) (đang bận)`), `menu.volume.disconnectBusy` (`Ngắt kết nối (đang bận)`),
  `menu.volume.forgetSavedPasswordBusy` (`Quên mật khẩu đã lưu (đang bận)`), `menu.volume.forgetServerBusy`
  (`Quên máy chủ (đang bận)`). Mục gốc và dạng mờ phải khớp từng chữ để người đọc thấy đó là một mục ở hai trạng thái;
  ❌ đừng nghĩ ra dấu hiệu thứ hai (`đang dùng`, `bận`) cho khóa mới. Thuật ngữ: `decisions.md` § Menu gốc.
- **`Quên` là ngoại lệ có chủ ý của luật `xóa` / `gỡ bỏ`.** Khi tiếng Anh gọi hành động là "Forget" (bỏ một máy chủ hay
  một mật khẩu đã lưu khỏi danh sách của Cmdr), tiếng Việt viết `Quên` ở mọi bề mặt: mục menu, tiêu đề hộp thoại, thân
  hộp thoại, và cả toast hỏng (`Cmdr không thể quên {name}.`). macOS làm y vậy với Wi‑Fi (`Forget` → `Quên`). Chỉ dùng
  `xóa` khi tiếng Anh thật sự nói "delete"/"remove" (`fileExplorer.network.deletePasswordFailed`).
- **"Reach" có hai lối, tùy đích đến.** Một MÁY CHỦ thì `không kết nối được tới` (`licensing.error.network`,
  `servers.refusal.unreachable`); một ĐƯỜNG DẪN hay ổ đĩa thì `không thể truy cập` (`fileExplorer.unreachable.title`,
  `.locationUnreachableToast`). Bằng chứng: `decisions.md` § Trung tâm máy chủ.
- **Một cú rớt mạng là `Đã mất kết nối`, không phải `Kết nối đã bị ngắt`.** `ngắt` thuộc về hành động chủ ý
  (`Ngắt kết nối`), nên dùng nó cho sự cố sẽ làm hai trạng thái khác hẳn nhau đọc y như nhau. macOS: `CFNetwork`
  (`The network connection was lost.` → `Đã mất kết nối mạng.`).
- **Một bề mặt, một tên tiếng Việt, kể cả khi tiếng Anh có hai.** Bộ chọn ổ đĩa được tiếng Anh gọi là "volume chooser" ở
  `shortcuts.scope.volumeChooser` và ba khóa `commands.*VolumeChooser`, nhưng là "volume switcher" ở hai toast
  `fileExplorer.navigation.server*PinnedToast`. Tiếng Việt chỉ có `bộ chọn ổ đĩa`. Gặp một tên tiếng Anh mới cho một
  danh sách đã có tên tiếng Việt thì dùng lại tên cũ, đừng dịch sát cái tên mới.
- **`Never` có hai nghĩa, và cột quyết định nghĩa nào.** Một ô nói về QUÁ KHỨ ("chưa xảy ra lần nào", như cột
  `Last used`) là `Chưa từng`, đúng như bảng quyền trong Cài đặt hệ thống. Một mục CHỌN nói về tương lai ("đừng bao giờ
  làm", như lịch lặp lại hay `settings.fileOperations.allowFileExtensionChanges.opt.no`) là `Không bao giờ`. macOS dùng
  cả hai; đừng lấy nhầm.
- **`Vị trí` là "nơi chốn để đi tới", `Địa điểm` là nơi chốn ĐỊA LÝ.** Finder gọi mục `Locations` ở khung bên là
  `Vị trí`; Photos/Maps/Journal gọi `Places` (ảnh chụp ở đâu) là `Địa điểm`. Cmdr chỉ dùng nghĩa thứ nhất
  (`shortcuts.scope.places` = danh sách các bản chia sẻ / bucket bên trong một máy chủ), nên không bao giờ viết
  `Địa điểm`.
- **`đăng nhập` luôn có NGƯỜI làm chủ ngữ.** Tiếng Anh cho phép "This server signs in with a key"; tiếng Việt thì
  `Máy chủ này đăng nhập bằng khóa` đọc thành máy chủ đi đăng nhập chỗ khác. Khi chủ ngữ là một MÁY, đổi động từ chính
  sang `dùng` và đẩy `đăng nhập` xuống vế mục đích: `Máy chủ này dùng khóa thay vì mật khẩu để đăng nhập`. Ví dụ:
  `servers.paneState.signedOutNothingToAsk`.
- **"rather than" / "instead of" là `thay vì`, và "type/enter vào một ô" là `nhập`.** Cả hai đã ship nhiều chỗ trong
  catalog; đừng nghĩ ra `chứ không phải` hay `gõ` cho chuỗi mới (`gõ ký tự` của macOS dành cho việc gõ trên bàn phím).
  Bằng chứng: `decisions.md` § Trung tâm máy chủ: khung đang kết nối lại.
- **`Ask Cmdr` names the chat panel only; prose about what the assistant DOES says `Cmdr`, and prose about the model
  says `AI`.** English draws the same line: `Ask Cmdr` survives in the panel title, the View menu, the palette command,
  the settings section, and the on/off switch, and is gone from every sentence that merely described the behavior. So a
  sentence subject is `Cmdr` (`Cmdr theo dõi các thư mục…`) while a surface name stays `cài đặt Ask Cmdr`. ❌ Don't put
  `Ask Cmdr` back into the sentences.
- **Where English says "the AI", Vietnamese says `AI`, not `Cmdr`.** The `suggestedOps.*` family (`Lý do của AI`,
  `AI đã đề xuất những thao tác này`) deliberately names the model, because its neighbour `suggestedOps.cmdrFacts`
  (`Những gì Cmdr biết`) claims the opposite thing (what the app VERIFIED). Collapsing the two subjects destroys the
  whole point of the pair.
- **`tải về` caught a real regression, so re-check it on every new download string.**
  `onboarding.cloudSetup.step.install` shipped as `Tải xuống và cài đặt` against the documented rule above (macOS vi: 35
  `tải về`, zero `tải xuống`); it now reads `Tải về và cài đặt`. The separable form is normal: `tải một mô hình về`.
- **"one" standing in for a just-named countable thing is `một cái`.** English leans on it twice in the AI copy ("Turn
  one on in settings", "Click to set one up in settings"); repeating `một nhà cung cấp` in the second sentence is heavy
  in Vietnamese, so `Hãy bật một cái trong cài đặt` / `Bấm để thiết lập một cái trong cài đặt`. Both keys use the same
  shape on purpose (`askCmdr.error.notConfigured`, `askCmdr.wake.needsApiKey`).
- **Tên bề mặt của macOS: `Dock` và `Finder` giữ tiếng Anh, `Applications` thì dịch.** Apple quyết định từng nhãn một,
  nên đừng suy từ nhãn này sang nhãn kia: `Dock` và `Finder` không bao giờ được dịch trong macOS `vi`, còn thư mục
  `Applications` luôn là `thư mục Ứng dụng`. Bằng chứng: `decisions.md` § Lời mời ghim Cmdr vào Dock.
- **"at any time" → `bất cứ lúc nào`, kể cả khi macOS viết `bất kỳ lúc nào`.** Cả hai đều đúng tiếng Việt, nhưng catalog
  đã ship `bất cứ lúc nào` ở chín chỗ; một chuỗi mới đi lệch sẽ làm hai câu chị em đọc như hai giọng khác nhau.
- **Menu chuột phải trên biểu tượng Dock có nguồn Tier 1 riêng, và kho tham chiếu KHÔNG chứa nó.**
  `/System/Library/CoreServices/Dock.app/Contents/Resources/vi.lproj/DockMenus.strings` (đọc bằng
  `plutil -convert json -o -`) chính là menu mà `menu.dock.*` rơi vào; macOS có ship `vi.lproj` cho bundle này. Nó chốt
  luật **tên ỨNG DỤNG đi trần, tên TỆP mới đóng ngoặc kép** (`Ẩn %@` / `Hiển thị %@` so với `Mở “%@”`), nên
  `menu.dock.openCmdr` là `Mở Cmdr`. Chi tiết: `decisions.md` § Menu chuột phải trên biểu tượng Dock.
- **Khuôn `{a} ({b})` để phân biệt hai hàng trùng tên giữ nguyên như tiếng Anh.** AppKit vi dịch `"%@ (%@)"` thành
  `"%1$@ (%2$@)"`: tiếng Việt đặt phần bổ nghĩa sau danh từ chính, nên không đảo thứ tự và không thêm giới từ vào trong
  ngoặc (`menu.dock.locationInParent`).
- **Một lệnh xuất hiện ở hai menu thì hai nhãn phải khớp từng chữ.** `menu.dock.searchFiles` và `menu.edit.searchFiles`
  đều là `Tìm kiếm tệp…`; một biến thể "hay hơn" ở một chỗ sẽ đọc thành hai chức năng khác nhau.
- **Quotation marks: the catalog uses STRAIGHT `"`, not the curly `“…”` this guide recommends.** Every quoted phrase
  shipped in `onboarding.json` (`onboarding.stepFda.step2.tip`, `onboarding.stepOptional.networking.desc`,
  `onboarding.stepAi.table.searchWithout`) mirrors the English's straight quotes, and there are only 12 curly quotes in
  the whole `vi/` catalog against 136 straight ones. Follow the file you're in: a new key beside straight-quoted
  siblings uses straight quotes. The curly preference stands for prose that has no quoting neighbours.
- **Two first persons, and they're not interchangeable.** David speaking is `mình` (`onboarding.stepBeta.greeting`,
  `.feedback.call`, `.star`, `.openBeta`); the USER speaking, as on a radio option, is `tôi`
  (`onboarding.stepAi.cloud.label` = `Có, tôi muốn AI`). Picking the wrong one flips who is talking.
- **A Settings path is `Cài đặt › <tên mục>`**, with the section name taken verbatim from `settings.section.*`
  (`Cài đặt › Cập nhật & quyền riêng tư`). Keep the `›` character; don't swap it for `>` or `/`.
- **The pile has no "Why", "More info", or permission-prompt names; sweep the installed OS instead.** Several Tier 1
  answers this catalog needs live only in `.loctable` files, not in the extracted pile. The whole-system sweep in
  `docs/i18n/reference-pile/how-to-mine.md` (walk `/System/**`, `plutil -convert json` each `.loctable`, filter the `en`
  side, read the `vi` side by the same key) takes ~2 minutes and is what sourced `Tại sao?` and `Mạng cục bộ`. Reach for
  it before recording a term as `tentative`.
- Record any case-by-case rulings here so they aren't relitigated.

## Open questions

Subjective calls, coined terms, and deferred migrations that a native reviewer should settle live in `review-queue.md`;
each already ships a reasoned value.

## Termbase files

- `../concepts.json`: the shared, language-agnostic concept registry (sense, `match` patterns, confusable neighbors).
- `concepts-proposed.json`: concepts this locale needed that the registry lacks, staged for the lead to merge.
- `terms.json`: this locale's ruling per concept, with the catalog keys that legitimately deviate under `exceptions`.
- `decisions.md`: the rationale journal, one section per feature, headings citing their keys.
- `review-queue.md`: open questions for a native reviewer.

Add or change a ruling in place in `terms.json` (a replaced form moves to `avoid`), and add a `decisions.md` section
when the reason needs more than a line.
