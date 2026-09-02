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
- **get info: `Lấy thông tin`; the Locked checkbox in that panel: `Đã khóa`** · macOS Finder Tier 1 (`N165`, `TL22`, the
  `"Get Info"` key in `Localizable.json`; `AXNODE1` is the checkbox's own accessibility name, and `NE18` builds our
  exact sentence: `Chọn Tệp > Lấy thông tin, bỏ chọn “Đã khóa” rồi thử lại.`), verified 2026-08-23. Apple DOES localize
  both, so ❌ never leave "Get Info" or "Locked" in English inside Vietnamese prose, whatever an `en` `@key.description`
  says. `high`.

Tentative / needs a native check:

- **volume: `ổ đĩa` / `phân vùng`** · no clean macOS "volume" string in the pile; "ổ đĩa" (drive) reads natural for a
  mounted volume, "phân vùng" = partition. `tentative`.
- **pane: `khung`** · three sources, three words: Total Commander vi says `bảng`, Microsoft says `ngăn`, and the Cmdr
  catalog uses `khung`. No macOS "pane" string exists. `khung` stays for catalog consistency. `tentative`.
- **bookmark: `dấu trang`** · GNOME phrasing for bookmarking; "đánh dấu" is the verb. `tentative`.
- **listing: `danh sách tệp`** · reads natural for the file list; no single canonical source term. `tentative`.
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
- **Ask Cmdr "looks inside" a file: `xem bên trong tệp`, and the three photo words stay apart.** The inspect tool
  and the consent copy share one root (`Đang xem bên trong tệp`, `có thể xem bên trong tệp mà bạn hỏi đến`).
  Around it: thumbnail is `hình thu nhỏ` (macOS + MS; not GNOME's `ảnh thu nhỏ`), a photo's camera is `máy ảnh`,
  and where it was taken is `vị trí chụp` (never bare `vị trí`, which the catalog uses for a file's path). When
  "photo" lands right next to "camera", write `một bức ảnh` so `ảnh` doesn't double up; elsewhere keep bare `ảnh`.
  Evidence: `glossary.md` § Ask Cmdr xem bên trong tệp.
- Record any case-by-case rulings here so they aren't relitigated.

## Glossary

The living term glossary for this language is in `glossary.md`. Read it before translating and add to it as you settle
terms, each sourced from the reference pile (`_ignored/i18n/vi/`; recipes in `docs/i18n/reference-pile/how-to-mine.md`).
Never guess a term.
