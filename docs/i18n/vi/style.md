# Vietnamese (vi) translation style guide

Working notes for translating Cmdr into Vietnamese. Read `../README.md` for how this fits the translation process, and
the app-wide `docs/style-guide.md` for the English voice these notes carry into Vietnamese.

Vietnamese is well-resourced: the pile (`_ignored/i18n/vi/`) has macOS Finder/AppKit/SystemSettings, MS terminology +
style guide, GNOME Nautilus, Xfce Thunar, KDE Dolphin, and Total Commander, so both UI families are covered. Most terms
reach `high`. Evidence verified against the pile on 2026-06-20, source list re-checked 2026-07-21.

## Decisions to confirm with David

The calls a translator can't make alone. The rest of the guide assumes them; both carry a confident default and are
listed so they're never relitigated.

- **Second person: `bạn` recommended (high).** Vietnamese has no T-V formality axis, but it has a huge kinship-based
  pronoun system (anh/chị/em/cô/chú…) keyed to relative age, gender, and status. A file manager can't know any of that,
  so it uses the neutral software pronoun **`bạn`** ("you", lit. "friend"). macOS, the MS Vietnamese style guide
  ("Address the user as you… third-person references like 'user' should be avoided", verified 2026-06-20), and the
  general SaaS convention all land on `bạn`. Flagging only because `bạn` can read slightly distant/flat to a native ear
  (unverified, web sources), but every major product accepts that tradeoff because picking any kinship term would be
  wrong for most users. Recommended default: **`bạn`, and often drop the pronoun entirely** where the sentence reads
  fine without it (Vietnamese imperatives commonly omit the subject).
- **Diacritics are mandatory, never optional (high).** See the decision point, this is the biggest technical hazard.

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

## Terminology and glossary

Format per term: `chosen · sources · confidence`. Confidence: `confirmed` (native sign-off), `high` (authoritative
sources agree), `tentative` (sources conflict or none had it). Evidence from `_ignored/i18n/vi/` (macOS Finder/AppKit,
MS terminology, GNOME Nautilus, Xfce Thunar), verified 2026-06-20. Sources decide the term; Cmdr writes its own value
(Apple/MS copyrighted, GNOME/Xfce GPL, never copied verbatim).

Settled terms (sources agree):

- **folder: `thư mục`** · macOS Finder ("Thư mục"), GNOME ("Thư mục"). No plural inflection (Vietnamese has no number
  morphology). `high`.
- **file: `tệp`** · macOS/MS convention ("tệp"); GNOME sometimes "tập tin" (Southern-flavored). Prefer **`tệp`** to
  match macOS. `high`.
- **trash: `thùng rác`** · macOS Finder ("Thùng rác"), GNOME ("Thùng rác"). `high`.
- **move to trash: `chuyển vào thùng rác`** · GNOME ("Cho vào Thùng rác"). `high`.
- **delete: `xóa`** · macOS AppKit ("Xóa"). `high`.
- **copy: `sao chép`** · macOS AppKit ("Sao chép"). `high`.
- **paste: `dán`** · macOS AppKit ("Dán"). `high`.
- **cut: `cắt`** · macOS AppKit ("Cắt"). `high`.
- **cancel: `hủy`** · macOS Finder/AppKit ("Hủy"). `high`.
- **open: `mở`** · macOS AppKit ("Mở"). `high`.
- **save: `lưu`** · macOS AppKit ("Lưu"). `high`.
- **move: `di chuyển`** · macOS AppKit ("Di chuyển"). `high`.
- **search: `tìm kiếm`** · macOS AppKit ("Tìm kiếm"). `high`.
- **eject: `đẩy ra`** · GNOME ("Đẩy ra"). `high`.
- **rename: `đổi tên`** · GNOME ("Đổi tên"). `high`.
- **sort: `sắp xếp`** · GNOME ("Sắp xếp"). `high`.
- **sidebar: `khung bên`** · GNOME ("khung bên"). `high`.
- **disconnect: `ngắt kết nối`** · macOS AppKit ("Ngắt kết nối"). `high`.
- **tab (a UI tab): `tab`; a Finder tag: `thẻ`** · macOS Finder vi ("Hiển thị Tất cả Tab", "Ẩn Thanh Tab") and Safari vi
  ("Tab mới", "Đóng tab", "Ghim tab") keep the loanword for the UI tab, while Finder's tag menu is `Thẻ…` / `Thêm thẻ…`
  (macOS 26.6.2, per-nib `MenuBar.strings` / `InfoWindowTaggingHeaderView.strings`, verified 2026-08-30). The catalog
  now names them apart across all 28 tab keys; `menu.bar.tab` is deliberately identical to English and carries a
  `sameAsSourceJustification`. `high`.
- **show: `hiển thị`, never `hiện`** · macOS vi says `Hiển thị` 147 times across Finder/AppKit/System Settings and never
  uses `hiện` as the verb (every `hiện` there is `hiện tại` / `hiện có` = "current"), so a label starting with `Hiện`
  reads as "current…". `high`.
- **size: `kích cỡ`, never `kích thước`** · macOS vi: 33 hits for `kích cỡ`, zero for `kích thước`; Microsoft
  terminology agrees (`size → kích cỡ`). `high`. The one compound that keeps its own shape is `Cỡ chữ` ("Text size").
- **search: `tìm kiếm`; find: `tìm`** · macOS AppKit splits them exactly this way (Search → `Tìm kiếm`, Find → `Tìm`,
  Finder `MenuBar 300783.title`). `high`.
- **download (noun and verb): `tải về`, never `tải xuống`** · macOS vi: 35 hits for `tải về`, zero for `tải xuống`
  (Microsoft's `tải xuống` is the Windows convention). A downloaded item is `bản tải về` (Finder "Remove Download" →
  `Xóa bản tải về`). `high`.
- **filesystem: `hệ thống tệp`** · macOS vi renders "file system"/"filesystem" as `hệ thống tệp` throughout Disk Utility
  and ASR (`Localizable.loctable`, `ASRLocalizable.loctable`: "Verifying file system." → `Đang xác minh hệ thống tệp.`),
  and the Cmdr catalog already uses it in `errors.listing.*`. `high`.
- **debugging: `gỡ lỗi`; USB debugging: `gỡ lỗi qua USB`** · macOS vi Safari `DeveloperPreferences.strings` ("Enable …
  debug mode" → `Bật chế độ gỡ lỗi …`, verified on macOS 26.6.2 build 25G83, live-bundle mining, 2026-09-06), and the
  catalog's `settings.advanced.logLlmCalls.description` already says `để gỡ lỗi`. The Android toggle's own Vietnamese
  name is `Gỡ lỗi qua USB`: AOSP `frameworks/base/packages/SettingsLib/res/values-vi/strings.xml`, key `enable_adb`,
  with `usb_debugging_title` in `packages/SystemUI/res/values-vi/strings.xml` reading `Cho phép gỡ lỗi qua USB?`
  (verified on `main`, 2026-09-07). Write it word for word so a reader finds the switch on their phone. `high`.
- **Android platform tools: `bộ công cụ nền tảng Android`** · Google's own Vietnamese developer docs render "SDK
  Platform-Tools" as `bộ công cụ nền tảng SDK` in the nav and in prose ("Xoá e2fsdroid khỏi công cụ nền tảng SDK"),
  while the download buttons keep the product name `Android SDK Platform-Tools` in English
  (`developer.android.com/tools/releases/platform-tools?hl=vi`, verified 2026-09-07). So the descriptive phrase is
  `bộ công cụ nền tảng`; `Android` and the command name `adb` stay verbatim. `high`.
- **command (a shell/CLI command): `lệnh`** · macOS vi loctables ("What command should be run?" → `Nên chạy lệnh nào?`,
  "Menu Command" → `Lệnh menu`), verified 2026-09-06. Name a specific one as `lệnh "adb"`. `high`.
- **location (of a file or binary, as a field label): `vị trí`** · macOS Finder vi `Localizable.strings` ("Location" →
  `Vị trí`, "Go To Location" → `Đi tới vị trí`), verified on macOS 26.6.2, 2026-09-06. Matches the catalog's use of
  `vị trí` for where something sits, against `đường dẫn` for the path string itself. `high`.
- **leave (a field) empty: `để trống`** · macOS vi ("You may also leave them blank to bind anonymously." →
  `Bạn cũng có thể để trống chúng để liên kết ẩn danh.`, `Localizable.loctable`), verified 2026-09-06. `high`.
- **the usual way: `theo cách thông thường`** · macOS vi uses `thông thường` for "normal/usual" ("as normal disk" →
  `như ổ đĩa thông thường`), verified 2026-09-06. `high`.
- **get info: `Lấy thông tin`; the Locked checkbox in that panel: `Đã khóa`** · macOS Finder Tier 1 (`N165`, `TL22`, the
  `"Get Info"` key in `Localizable.json`; `AXNODE1` is the checkbox's own accessibility name, and `NE18` builds our
  exact sentence: `Chọn Tệp > Lấy thông tin, bỏ chọn “Đã khóa” rồi thử lại.`), verified 2026-08-23. Apple DOES localize
  both, so ❌ never leave "Get Info" or "Locked" in English inside Vietnamese prose, whatever an `en` `@key.description`
  says. `high`.

- **AI provider: `nhà cung cấp AI`; provider on its own: `nhà cung cấp`** · Microsoft terminology ("A company that
  provides services or content for its customers" → `nhà cung cấp`), and the catalog already ships it. `high`.
- **AI features: `các tính năng AI`** · the catalog's own `settings.askCmdr.interactiveModel.description`
  (`các tính năng AI khác của Cmdr`). `high`.
- **model (an LLM): `mô hình`** · Microsoft terminology, machine-learning sense ("An artifact resulting from running a
  machine learning algorithm…" → `mô hình`). `high`.
- **endpoint (an API URL): `điểm cuối`** · Microsoft terminology, three separate senses all render `điểm cuối`. `high`.
- **deployment (Azure OpenAI's named model instance): `bản triển khai`** · Microsoft terminology (`deployment` /
  `deploy` → `triển khai`). Azure is Microsoft's own product, so Microsoft outranks macOS here. `high`.
- **resource (an Azure resource): `tài nguyên`** · Microsoft terminology (three senses agree). `high`.
- **library (a catalog of things to pick from, like Ollama's model list): `thư viện`** · Microsoft terminology and macOS
  vi (`thư viện` in several senses). `high`. Qualify it when the surrounding screen has no other library:
  `thư viện mô hình` in `onboarding.cloudSetup.step.ollamaModel`, because bare `thư viện` collides with macOS vi's
  `chế độ xem thư viện` (Finder's gallery view).
- **Terminal (the command-line app): `Terminal`, capitalized, kept verbatim** · macOS vi Finder (`Mở trong Terminal`),
  verified 2026-09-09. `high`. ❌ Not KDE Dolphin's `dòng lệnh`: that names the command line, and macOS is Tier 1 for an
  app the user actually opens. `lệnh` still stays the word for one command, per the entry above.

Tentative / needs a native check:

- **volume: `ổ đĩa` / `phân vùng`** · no clean macOS "volume" string in the pile; "ổ đĩa" (drive) reads natural for a
  mounted volume, "phân vùng" = partition. `tentative`.
- **pane: `khung`** · three sources, three words: Total Commander vi says `bảng`, Microsoft says `ngăn`, and the Cmdr
  catalog uses `khung`. No macOS "pane" string exists. `khung` stays for catalog consistency. `tentative`.
- **bookmark: `dấu trang`** · GNOME phrasing for bookmarking; "đánh dấu" is the verb. `tentative`.
- **listing: `danh sách tệp`** · reads natural for the file list; no single canonical source term. `tentative`.
- **placeholder (a template token in an address or a format string): `phần giữ chỗ`** · no macOS source; Microsoft says
  `chỗ dành sẵn`, which reads as "reserved space" and fits a slide layout better than a URL token. Catalog consistency
  picks `phần giữ chỗ` (`indexing.json`, `onboarding.cloudSetup.hint.azureEndpoint`). `tentative`.
- **progress (advancement, in a negated "no progress"): `tiến triển`** · shared-root pick over macOS `tiến trình` (which
  this catalog uses for an OS process) and MS `Tiến độ`. Progress-the-bar stays `tiến trình`. `tentative`.
- **"has stopped moving" (running but not advancing): `đang đứng yên`** · plain everyday Vietnamese; no source names the
  state. Avoids `treo` (hung), which reads as a crash. `tentative`.

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
  (`Thư mục chứa`, `Nhà`, `Trở lại`, `Kích cỡ`). Ngoại lệ đã ghi: `tab` vs `thẻ`, xem `glossary.md` § Menu gốc.
- **Quotation marks: `"…"`** (curly double quotes, U+201C/U+201D) are standard; guillemets `«…»` also appear in some
  formal text. Prefer the curly doubles to match macOS. Avoid straight ASCII `"`.
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
- **Text expansion bites the queue-row status cell.** `queue.row.stalled` is `Không có tiến triển trong {duration}`
  against English's `No progress for {duration}` (~3× the character count) in a narrow row that otherwise shows
  `còn {duration}`. Overflow-check that cell specifically; if it clips, shorten the ROW string alone (for example
  `Đứng yên {duration}`) and keep the dialog line full, rather than trimming both.
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
  string with a swapped opener. Wording details and the settled values: `glossary.md` § Ba biến thể phần thân.
- **Eject / disconnect error copy sits AFTER a colon.** `errors.eject.*` is dropped into
  `Không thể tháo {volumeName}: …` or `Không thể ngắt kết nối: …`, so the wrapper already carries the "couldn't" part.
  Write only the reason plus the next step; don't restate the refusal. Terms and evidence: `glossary.md` § Lỗi khi tháo
  ổ đĩa / ngắt kết nối.
- **`di chuyển` is reserved for the Move operation.** When English uses a loose "moving files" that also covers copies
  and deletes, write the plain `chuyển tệp`; `di chuyển tệp` would narrow the sentence to one operation.
- **`rút` (unplug) has no pile source in Vietnamese** and rests entirely on catalog consistency (four shipped MTP
  strings). Use it only where the English genuinely tells someone to pull the cable, never for a connection reset.
- **Một gốc từ cho cả họ "add".** Mọi chỗ nói tới việc thêm ghi chú vào một báo cáo đã gửi đều đi từ `thêm`:
  `Thêm vào báo cáo sự cố của bạn` (tiêu đề), `Thêm vào báo cáo` (nút), `Đang thêm…` (đang chạy),
  `Đã thêm ghi chú vào báo cáo` (toast), `Không thể thêm ghi chú của bạn: {error}` (toast hỏng). Bằng chứng và các quyết
  định kèm theo: `glossary.md` § Thêm ghi chú vào báo cáo đã gửi.
- **Đừng lặp `của bạn` hai lần trong một câu ngắn.** Tiếng Anh rải "your" thoải mái; tiếng Việt thì nặng. Giữ `của bạn`
  ở chỗ nó mang thông tin (hoặc ở chỗ một chuỗi chị em đã dùng, để hai chuỗi khớp nhau) và bỏ ở chỗ quyền sở hữu đã hiển
  nhiên. Ví dụ `errorReporter.amendedToast.message`.
- **Nhắc tới một menu của ứng dụng thì viết `menu <Tên>`**, lấy tên đúng như `menu.bar.*` (macOS Finder `vi`):
  `từ menu Trợ giúp`. Catalog đã có sẵn câu này trong `settings.updates.errorReports.description`; dùng lại y hệt thay
  vì viết một biến thể mới.
- **"Back" has two right answers, and macOS draws the line.** Going back in the folder HISTORY is `Trở lại` (Finder Go >
  Back); returning to the previous SCREEN or step inside a flow is `Quay lại` (macOS Setup Assistant, the Apple ID and
  iCloud sheets). Don't flatten them: `glossary.md` § Rà soát trôi thuật ngữ.
- **Two report flows, two names.** A crash report is `báo cáo sự cố` (macOS Problem Reporter), an error report is
  `báo cáo trục trặc`. They sit next to each other as two toggles in Settings > Updates & privacy, so one word for both
  would make the panel unreadable.
- **`xóa` destroys, `gỡ bỏ` un-lists.** Delete (and anything that really erases bytes, like "Remove download") is `xóa`;
  taking an item off a list while it lives on is `gỡ bỏ`. macOS says `Xóa` for both because Finder never shows the two
  side by side; Cmdr does.
- **"Error" as a bare status cell is `Sự cố`; as a diagnostic prefix it's `Lỗi:`.** The `@key.description` of each key
  says which surface it is. Both are deliberate; see `glossary.md`.
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
  details and the `mục` vs `tệp` line: `glossary.md` § Toast sau khi hoàn tác thao tác đang chạy.
- **Ask Cmdr "looks inside" a file: `xem bên trong tệp`, and the three photo words stay apart.** The inspect tool and
  the consent copy share one root (`Đang xem bên trong tệp`, `có thể xem bên trong tệp mà bạn hỏi đến`). Around it:
  thumbnail is `hình thu nhỏ` (macOS + MS; not GNOME's `ảnh thu nhỏ`), a photo's camera is `máy ảnh`, and where it was
  taken is `vị trí chụp` (never bare `vị trí`, which the catalog uses for a file's path). When "photo" lands right next
  to "camera", write `một bức ảnh` so `ảnh` doesn't double up; elsewhere keep bare `ảnh`. Evidence: `glossary.md` § Ask
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
  ❌ đừng nghĩ ra dấu hiệu thứ hai (`đang dùng`, `bận`) cho khóa mới. Thuật ngữ: `glossary.md` § busy.
- **`Quên` là ngoại lệ có chủ ý của luật `xóa` / `gỡ bỏ`.** Khi tiếng Anh gọi hành động là "Forget" (bỏ một máy chủ hay
  một mật khẩu đã lưu khỏi danh sách của Cmdr), tiếng Việt viết `Quên` ở mọi bề mặt: mục menu, tiêu đề hộp thoại, thân
  hộp thoại, và cả toast hỏng (`Cmdr không thể quên {name}.`). macOS làm y vậy với Wi‑Fi (`Forget` → `Quên`). Chỉ dùng
  `xóa` khi tiếng Anh thật sự nói "delete"/"remove" (`fileExplorer.network.deletePasswordFailed`).
- **"Reach" có hai lối, tùy đích đến.** Một MÁY CHỦ thì `không kết nối được tới` (`licensing.error.network`,
  `servers.refusal.unreachable`); một ĐƯỜNG DẪN hay ổ đĩa thì `không thể truy cập` (`fileExplorer.unreachable.title`,
  `.locationUnreachableToast`). Bằng chứng: `glossary.md` § Trung tâm máy chủ.
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
  Bằng chứng: `glossary.md` § Trung tâm máy chủ, đợt 4.
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
  in Vietnamese, so `Hãy bật một cái trong cài đặt` / `Nhấp để thiết lập một cái trong cài đặt`. Both keys use the same
  shape on purpose (`askCmdr.error.notConfigured`, `askCmdr.wake.needsApiKey`).
- **Tên bề mặt của macOS: `Dock` và `Finder` giữ tiếng Anh, `Applications` thì dịch.** Apple quyết định từng nhãn một,
  nên đừng suy từ nhãn này sang nhãn kia: `Dock` và `Finder` không bao giờ được dịch trong macOS `vi`, còn thư mục
  `Applications` luôn là `thư mục Ứng dụng`. Bằng chứng: `glossary.md` § Lời mời ghim Cmdr vào Dock.
- **"at any time" → `bất cứ lúc nào`, kể cả khi macOS viết `bất kỳ lúc nào`.** Cả hai đều đúng tiếng Việt, nhưng catalog
  đã ship `bất cứ lúc nào` ở chín chỗ; một chuỗi mới đi lệch sẽ làm hai câu chị em đọc như hai giọng khác nhau.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/vi/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
