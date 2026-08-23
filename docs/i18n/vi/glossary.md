# vi glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/vi/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: `docs/guides/i18n-translation.md`.

## Terms

Settled during the `errors.json` pass (2026-06-21), each mined from `_ignored/i18n/vi/`:

- **network: `mạng`** · macOS AppKit (`NSNetwork` → "mạng"). `high`.
- **server: `máy chủ`** · macOS AppKit (`Servers` → "Máy chủ"), GNOME ("máy chủ"). `high`.
- **computer: `máy tính`** · macOS AppKit (`NSComputer` → "máy tính"). `high`.
- **connection: `kết nối`** · Xfce Thunar ("kết nối mạng"), macOS ("Ngắt kết nối"). The verb connect/disconnect pair is
  `kết nối` / `ngắt kết nối`. `high`.
- **device: `thiết bị`** · GNOME ("thiết bị này"), Xfce Thunar ("Ngắt kết nối thiết bị"). `high`.
- **permission / access: `quyền`** · Xfce Thunar ("Quyền hạn", "không có quyền"). "Write access" → `quyền ghi`; "don't
  have permission" → `không có quyền`. `high`.
- **password: `mật khẩu`** · GNOME ("bằng mật khẩu"). `high`.
- **username: `tên người dùng`** · standard MS/GNOME convention. `tentative` (not directly grepped, but unambiguous).
- **mount / unmount: `gắn kết` / `bỏ gắn kết`** · Xfce Thunar ("Gắn kết", "\_Bỏ gắn kết"). Remount → `gắn kết lại`.
  `high`.
- **read-only: `chỉ đọc`** · Xfce Thunar, GNOME ("Chỉ đọc"). `high`.
- **try again / retry: `thử lại`** · GNOME ("Thử \_lại"). `high`.
- **sign in / log in: `đăng nhập`** · MS terminology (`sign in` → "đăng nhập", VNM). `high`.
- **internet: `internet`** (loanword, lowercase) · kept as-is; "internet connection" → `kết nối internet`. `tentative`.
- **couldn't / can't / unable to: `không thể`** · GNOME ("Không thể"), Xfce Thunar ("Không thể gắn kết"). The calm
  negative-capability framing Cmdr's error voice wants (avoids a bare "lỗi"/"failed"). `high`.

Added during the `fileExplorer.json` pass (2026-06-21), triangulated (macOS Finder/AppKit Tier 1, MS terminology Tier 2,
GNOME Nautilus Tier 3); macOS wins ties since Cmdr is a macOS app:

- **host: `máy chủ`** · macOS Finder ("Kết nối với máy chủ"), MS terminology. Same word as server; vi doesn't
  distinguish. `high`.
- **hostname: `tên máy chủ`** · MS terminology, macOS ("Máy chủ:"). `high`.
- **connect to server: `kết nối với máy chủ`** · macOS Finder verbatim ("Kết nối với máy chủ"). `high`.
- **server address: `địa chỉ máy chủ`** · macOS Finder ("Địa chỉ máy chủ"). `high`.
- **share (network share, noun): `chia sẻ`; shared folder: `thư mục chia sẻ`** · macOS Finder ("Thư mục được chia sẻ",
  "chia sẻ"). NOT MS's first hit "cổ phần" (financial sense, wrong). `high`.
- **eject: `tháo`** · macOS Finder/AppKit ("Tháo", `NSNavEjectButton` → "tháo"). Overrides the style guide's tentative
  "đẩy ra" — macOS Tier 1 says "tháo". `high`.
- **credentials: `thông tin đăng nhập`** · MS terminology. `high`.
- **guest: `khách`** · MS terminology, macOS. `high`.
- **Keychain -> `chuỗi khóa`; Keychain Access (the app) -> `Truy cập chuỗi khóa`** · macOS Vietnamese · `high`. The
  localized Apple feature name: Apple localizes "Keychain" as the common noun "chuỗi khóa" and the app as "Truy cập
  chuỗi khóa" (Apple vi support guide, `support.apple.com/vi-vn/guide/keychain-access`, verified 2026-06-21). Not kept
  verbatim because Apple does localize it for vi macOS users (Decision 1; same rule as Quick Look). Supersedes the old
  "keep Keychain verbatim" note. Applied to `ai.secretError.keychainTitle/Body` and the three
  `fileExplorer.network`/`navigation` strings referencing the credential store.
- **favorites / favorite: `mục ưa thích`** · macOS Finder ("Mục ưa thích", "Máy chủ ưa thích"). `high`.
- **tab (UI tab): `thẻ`** · macOS Finder ("Thẻ ưa thích"), GNOME ("thẻ mới"). Resolves the style guide's tentative.
  `high`.
- **refresh / rescan: `làm mới` / `quét lại`** · refresh → MS "làm mới"; rescan → "quét lại" (scan = "quét", natural).
  `high` / `tentative`.
- **index (noun): `chỉ mục`; indexing (verb): `lập chỉ mục`; indexed/up to date: `đã lập chỉ mục`** · macOS ("chỉ mục",
  "Đang cập nhật chỉ mục", "Đã lập chỉ mục"), MS terminology. `high`.
- **drive / volume: `ổ đĩa`** · macOS ("Ổ đĩa khởi động"), MS terminology. `high` (drive); `tentative` (volume reuse).
- **browse: `duyệt`** · macOS Finder ("Duyệt các máy chủ khả dụng"). `high`.
- **pane: `khung`** · the two file lists; style-guide tentative kept. `tentative`.
- **timeout (verb): `hết thời gian chờ`** · phrased naturally; no single term. `tentative`.
- **disk usage / disk space: `dung lượng đĩa`** · "dung lượng" (capacity) per macOS ("Giá trị dung lượng"). `tentative`.
- **read-only device/volume: `chỉ đọc`** · MS terminology, GNOME. `high`.

UI section/group names used (keep consistent across files):

- Favorites → **Mục ưa thích**; Volumes → **Ổ đĩa**; Cloud → **Đám mây**; Mobile → **Thiết bị di động**; Network →
  **Mạng**.

UI/section phrasings settled here (for consistency in other files):

- **"Here's what to try" (error-list lead-in): `Bạn có thể thử:`** · natural friendly framing, ends in a colon before
  the bullet list. `tentative`.
- **Terminal, Disk Utility, First Aid, Activity Monitor, Spotlight, Finder, Get Info, System Settings** · macOS
  feature/app names; kept in English per the do-not-translate rule (these match what a Vietnamese macOS may localize,
  but Cmdr's error copy references them as proper names alongside literal commands).

Added during the `settings.json` pass (2026-06-21). Reuses the prior-pass terms above (eject → `tháo`, tab → `thẻ`, pane
→ `khung`, share → `chia sẻ`, mount → `gắn kết`, index → `chỉ mục`/`lập chỉ mục`, drive/volume → `ổ đĩa`); new terms
below:

- **theme (light/dark/system): `Sáng` / `Tối` / `Hệ thống`** · MS ("Sáng"/"tối"), macOS ("Hệ thống"). `high`.
- **download (verb): `tải xuống`; Downloads (folder): `Tải về`** · MS verb ("tải xuống"), macOS folder ("Tải về").
  `high`.
- **notification: `thông báo`** · macOS/MS. `high`.
- **update(s): `cập nhật`** · macOS ("Cập nhật"), MS. `high`.
- **port: `cổng`** · MS ("cổng"). `high`.
- **cache (noun/verb): `bộ đệm` / `lưu vào bộ đệm`** · MS ("bộ đệm ẩn"); plain "bộ đệm" for UI brevity. `high`.
- **timeout: `thời gian chờ`** · standard MS phrasing. `high`.
- **threshold: `ngưỡng`** · MS ("ngưỡng"). `high`.
- **provider: `nhà cung cấp`** · MS. `high`.
- **service: `dịch vụ`** · MS. `high`.
- **context window: `cửa sổ ngữ cảnh`** · literal, no single source. `tentative`.
- **token (LLM): `token`** (loanword) · MS lists "token"/"mã thông báo"; keep `token` for the LLM sense. `tentative`.
- **binary / decimal (size base): `nhị phân` / `thập phân`** · MS. `high`.
- **reset: `đặt lại`** · macOS ("Đặt lại"). `high`.
- **restart: `khởi động lại`** · macOS ("Khởi động lại"). `high`.
- **preview: `xem trước`** · macOS ("Xem trước"). `high`.
- **sidebar: `thanh bên`** · macOS ("Thanh bên"), MS. (Overrides the style guide's GNOME "khung bên" — macOS wins.)
  `high`.
- **git terms — branch: `nhánh`, commit: `commit`, tag: `thẻ`, repository: `kho`, worktree: `worktree`** · MS ("nhánh",
  "kho lưu trữ"); commit/worktree kept as loanwords (dev audience, no clean native UI source). `tentative`.
- **stale (index): `lỗi thời`** · natural phrasing for an out-of-date index. `tentative`.
- **toast / chip / banner (UI): `thông báo nhỏ` / `huy hiệu` / `biểu ngữ`** · descriptive renderings; no single source.
  `tentative`.

Settings section/UI names (keep consistent across files):

- Appearance: `Giao diện` · Behavior: `Hành vi` · File systems: `Hệ thống tệp` · Search: `Tìm kiếm` · Viewer:
  `Trình xem` · Developer: `Nhà phát triển` · Advanced: `Nâng cao` · Keyboard shortcuts: `Phím tắt` · License:
  `Giấy phép` · Updates & privacy: `Cập nhật & quyền riêng tư`.
- View modes — Full: `Đầy đủ` · Brief: `Rút gọn`. Columns — Name: `Tên` · Ext: `Đuôi`.
- Commands — Rename: `Đổi tên` · View: `Xem` · Copy: `Sao chép` (keep aligned with other catalog files).

Added during the `licensing.json` + `ai.json` + `viewer.json` pass (2026-06-21). Reuses prior terms (server → `máy chủ`,
organization → `tổ chức`, model → `mô hình`, download → `tải xuống`, restart → `khởi động lại`, cancel → `hủy`, close →
`đóng`, retry/try again → `thử lại`); new terms below, each mined from `_ignored/i18n/vi/`:

- **license (noun): `giấy phép`; license key: `khóa giấy phép`** · MS terminology ("digital license" → "giấy phép kỹ
  thuật số"; "product key" → "khóa sản phẩm", adapted to "khóa giấy phép" for the license sense). macOS Tier 1 has no
  clean "License" string. `high` (giấy phép); `high` (khóa giấy phép).
- **activate / deactivate: `kích hoạt` / `hủy kích hoạt`** · MS terminology ("activate" → "kích hoạt", "deactivate" →
  "hủy kích hoạt"). `high`.
- **subscription: `đăng ký`** · MS terminology ("subscription" → "đăng ký"). Note: also the verb "subscribe"; context
  disambiguates. `high`.
- **renew: `gia hạn`** · MS terminology ("renew" → "gia hạn"). `high`.
- **expire / expired: `hết hạn`** · MS terminology ("expire" → "hết hạn"). `high`.
- **verify: `xác minh`** · MS terminology ("verify" → "xác minh"). `high`.
- **perpetual (license): `vĩnh viễn`** · no source term; natural rendering for a one-time/forever license. `tentative`.
- **valid / validity: `có hiệu lực` / `hiệu lực`** · natural legal-doc phrasing; no single source term. `tentative`.
- **commercial / personal (license tiers): `Thương mại` / `Cá nhân`** · standard rendering; kept capitalized as tier
  names. `high`.
- **(open) beta: `beta` (loanword)** · kept as-is, lowercase; "open beta" → "beta công khai". `tentative`.
- **clipboard: `bảng nhớ tạm`** · macOS Finder/AppKit verbatim ("Clipboard" → "bảng nhớ tạm"). `high`.
- **select all: `chọn tất cả`** · macOS AppKit ("Select All" → "Chọn Tất cả"; sentence-cased to "Chọn tất cả"). `high`.
- **viewer (file viewer): `trình xem`; file viewer: `trình xem tệp`** · Total Commander ("trình xem", "trình xem tập
  tin"; orthodox file-manager lineage). NOT MS's first hit "người xem" (audience sense, wrong). `high`.
- **view (verb) / view mode: `xem` / `chế độ xem`** · macOS Finder ("chế độ xem"), TC ("Xem"). `high`.
- **image: `hình ảnh`; document: `tài liệu`** · MS ("hình ảnh"), GNOME/Dolphin ("Tài liệu"). `high`.
- **(character) encoding: `mã hóa ký tự`** · MS terminology ("character encoding" → "mã hóa ký tự"). `high`.
- **regex: `Regex` (loanword)** · kept as the short form per the EN copy; "regular expression" has no clean native UI
  term. `tentative`.
- **line / character (of text): `dòng` / `ký tự`** · GNOME ("dòng"), standard. `high`.
- **memory (RAM): `bộ nhớ`** · MS ("memory" → "bộ nhớ"). `high`.
- **word wrap: `ngắt dòng`** · natural rendering (wrap at edge); no single source. `tentative`.
- **streaming (large-file mode): `phát trực tiếp`** · MS-style rendering for streaming. `tentative`.
- **zoom / pan / fit: `thu phóng` / `di chuyển` / `vừa khít`** · MS ("zoom" → "thu phóng"); pan/fit are natural
  renderings. `high` (zoom); `tentative` (pan, fit).
- **clipboard limit / paste: `dán`** · macOS AppKit ("Dán"). `high`.
- **endpoint: `điểm cuối`** · MS terminology ("endpoint" → "điểm cuối"). `high`.
- **API key: `khóa API`** · standard; "API" kept verbatim. `high`.
- **quota: `hạn ngạch`; rate-limit: `giới hạn tần suất`** · MS ("quota" → "hạn ngạch"); rate-limit is a natural
  rendering. `high` (quota); `tentative` (rate-limit).
- **provider (AI/sync): `nhà cung cấp`** · MS, reused from settings pass. `high`.
- **AI: `AI`** (loanword, kept verbatim) · universal in vi tech UI; "AI-powered" → "do AI hỗ trợ". `high`.
- **model (AI/ML): `mô hình`** · MS "model" lists "mô hình 3D" for the 3D sense; the bare ML sense is "mô hình". `high`.
- **endpoint URL / cloud: `URL điểm cuối` / `đám mây`** · cloud reused from settings (`Đám mây`). `high`.

UI/section phrasings settled here (for consistency in other files):

- **Settings > AI (nav path): `Cài đặt > AI`** · "Cài đặt" per macOS; "AI" kept verbatim; the `>` separator preserved.
- **Viewer window name: `Trình xem`** (matches the settings-pass Viewer section `Trình xem`).

Added during the `queryUi.json` + `commands.json` pass (2026-06-21), macOS Finder/AppKit Tier 1 (`vi/macOS/`), MS
terminology Tier 2 (`VIETNAMESE.tbx`); macOS wins ties:

- **search / search (the action): `tìm kiếm`** · macOS Finder ("Tìm kiếm"), MS. `high`.
- **query (noun, e.g. "Query:"): `truy vấn`** · macOS Finder ("Truy vấn để tìm kiếm trong Finder"), MS. `high`.
- **results: `kết quả`** · MS ("kết quả"). `high`.
- **scan / scanning: `quét` / `đang quét`** · MS ("quét"). "Scan in progress" → `Đang quét`. `high`.
- **pattern: `mẫu`** · MS ("mẫu hình"); short UI form `mẫu`. `high`.
- **wildcard: `ký tự đại diện`** · MS ("kí tự đại diện"; standard spelling `ký`). `high`.
- **glob / regex: kept verbatim (`Glob`, `Regex`)** · technical loanwords, no native UI source. `tentative`.
- **case-sensitive: `phân biệt chữ hoa/thường`** · macOS Finder ("Phân biệt Chữ hoa/thường"). `high`.
- **ascending / descending: `tăng dần` / `giảm dần`** · MS ("thứ tự tăng dần", "thứ tự giảm dần"). `high`.
- **sort by: `sắp xếp theo`** · macOS Finder ("sắp xếp theo tên"). `high`.
- **zoom in / out: `phóng to` / `thu nhỏ`; zoom level: `mức phóng`** · macOS AppKit ("thu phóng"), GNOME ("Phóng
  to"/"Thu nhỏ"). `high`.
- **clipboard: `bảng nhớ tạm`** · macOS AppKit ("Bảng nhớ tạm"). `high`.
- **context menu: `menu chuột phải`** · MS ("menu chuột phải"). `high`.
- **quit: `thoát`; hide: `ẩn`** · macOS AppKit/MS. `high`.
- **offline (cloud): `ngoại tuyến`; "make available offline": `tải xuống để dùng ngoại tuyến`** · standard MS/macOS
  convention; reworded for clarity. `tentative`.
- **command palette: `bảng lệnh`** · descriptive (no single source); `bảng` (panel) + `lệnh` (command). `tentative`.
- **onboarding (the first-launch wizard, noun): `thiết lập ban đầu`** · the setup sense (the wizard walks through FDA,
  AI, and optional setup), matching the wizard's own title `Thiết lập ban đầu Cmdr`. Unified app-wide post-translation
  (the earlier `hướng dẫn ban đầu` / "guide" rendering in `queryUi`/`commands`/`shortcuts` was retired so the menu item,
  command-palette entry, shortcut scope, and wizard title all match). MS "triển khai" is the deployment sense, wrong
  here. `high`.
- **scope (search scope): `phạm vi`** · macOS Finder ("phạm vi tìm kiếm"). `high`.
- **cursor (file-list cursor): `con trỏ`** · standard. `high`.
- **toggle (verb prefix): `bật/tắt`** · standard MS UI form for on/off commands. `high`.
- **Recents / recent: `gần đây`** · macOS Finder ("Gần đây"). `high`.
- **byte/bytes (unit): `byte`** (loanword, no plural inflection) · MS, macOS. `high`.

`queryUi`/`commands` phrasings settled (for consistency):

- **"Coming soon": `Sắp ra mắt`** · natural friendly framing. `tentative`.
- **"Hide boring folders" (playful): `Ẩn các thư mục nhàm chán`** · keeps the casual product voice per the en `@key`
  note. `tentative`.
- **agent (AI agent): `tác nhân`** · MS sense for software agent. Used in the `queryUi.ai.*` strip and the
  `onboarding.stepAi.*` comparison table. Unified app-wide post-translation (the onboarding pass's loanword `agent` was
  retired in favor of this). `high`.
- **`View > Zoom > 100%` (literal menu path in `commands.handler.zoomResetHintMenu`)** kept in English per the en
  `@key`: it's a literal menu-bar path, not prose.

Added during the `onboarding.json` + `fileOperations.json` pass (2026-06-21), triangulated (macOS Finder/AppKit Tier 1,
MS Tier 2, GNOME Nautilus/Xfce Thunar Tier 3); macOS wins ties. Reuses prior-pass terms (trash → `thùng rác`, delete →
`xóa`, copy → `sao chép`, move → `di chuyển`, rename → `đổi tên`, cancel → `hủy`, drive/volume → `ổ đĩa`, share →
`chia sẻ`, scan → `quét`/`đang quét`, cursor → `con trỏ`, network → `mạng`, server → `máy chủ`, restart →
`khởi động lại`, download → `tải xuống`, provider → `nhà cung cấp`, toast → `thông báo nhỏ`, quit → `thoát`); new terms
below:

- **overwrite / replace: `ghi đè`** · macOS Finder ("Ghi đè hay giữ lại phần mở rộng tệp"), GNOME ("ghi đè"). Cmdr uses
  `ghi đè` (overwrite) consistently; GNOME's "thay thế" (replace) not used. `high`.
- **permanently delete: `xóa vĩnh viễn`** · GNOME ("xóa vĩnh viễn"). `high`.
- **move to trash: `chuyển vào thùng rác`** · macOS Finder ("Di chuyển các mục vào Thùng rác"), GNOME. `high`.
- **skip: `bỏ qua`** · GNOME ("\_Bỏ qua"). Also used for Dismiss (timeout warning button) → `bỏ qua`. `high`.
- **merge (folders): `hòa trộn`** · GNOME ("\_Hòa trộn", "Hòa trộn thư mục"). `high`.
- **symlink / symbolic link: `liên kết mềm`** · GNOME ("liên kết mềm"). Link "target" → `đích`. `high`.
- **hardlink: `liên kết cứng`** · descriptive (parallels `liên kết mềm`); no single UI source. `tentative`.
- **destination: `đích` / `đích đến`; source: `nguồn`** · GNOME ("thư mục đích", "thư mục nguồn", "đích đến").
  Destination volume/path → `ổ đĩa đích` / `đường dẫn đích`. `high`.
- **rollback (undo an operation's partial work): `hoàn tác`** · natural Vietnamese; no single UI source (GNOME uses
  "\_Hoàn lại" for plain undo). Conflict-step Rollback button + tooltips use `hoàn tác`. `tentative`.
- **conflict (file clash): `xung đột`; "file already exists": `tệp đã tồn tại`** · standard MS/dev phrasing. `high`.
- **verify (before copy/move): `xác minh`** · "Verifying before copy" → `Đang xác minh trước khi sao chép`. `tentative`.
- **technical details: `chi tiết kỹ thuật`** · MS/standard. `high`.
- **retry / try again: `thử lại`** · macOS Finder ("Thử lại"). `high`.
- **close: `đóng`** · macOS ("Đóng"). `high`.
- **endpoint (URL): `điểm cuối`** · descriptive; "Endpoint URL" → `URL điểm cuối`. `tentative`.
- **API key: `khóa API`** · "API" verbatim, "key" → `khóa`. `high`.
- **model (AI/LLM): `mô hình`** · MS ("model" Noun sense). LLM kept verbatim. `high`.
- **full disk access: `truy cập toàn bộ đĩa`** · descriptive (no macOS TCC-pane string in the pile). Privacy & Security
  pane → `Quyền riêng tư & Bảo mật` (macOS SystemSettings verbatim). `tentative` (FDA phrase); `high` (Privacy &
  Security).
- **review and apply / at will: `xem lại rồi áp dụng` / `tùy ý`** · the with/without-AI table's recurring phrasing.
  `tentative`.

**Cross-pass terms resolved post-translation** (2026-06-21 reconciliation pass; both unified app-wide):

- **onboarding** → `thiết lập ban đầu` (setup sense), matching the wizard title `Thiết lập ban đầu Cmdr`. The
  `queryUi`/`commands`/`shortcuts` `hướng dẫn ban đầu` was retired. See the `onboarding` term entry above.
- **agent** → `tác nhân` (MS sense). The onboarding loanword `agent` was retired. See the `agent` term entry above.

macOS proper-name labels referenced in onboarding instructions (Vietnamese macOS wording where the pile has it, else
best-effort + `tentative`): Quit & Reopen → `Thoát & Mở lại` (macOS "Reopen" → `Mở lại`); Applications → `Ứng dụng`;
Documents → `Tài liệu`; Downloads → `Tải về`; Desktop → `Màn hình nền` (all macOS Finder); Full Disk Access →
`Truy cập toàn bộ đĩa`, Local network access → `Truy cập mạng cục bộ`, Accepting incoming connections →
`Chấp nhận kết nối đến` (no pile string; best-effort, `tentative`).

File-operation toggle/action names (keep consistent across files): Trash/Delete toggle → `Thùng rác` / `Xóa`; Copy/Move
toggle → `Sao chép` / `Di chuyển`; conflict actions — Skip → `Bỏ qua`, Overwrite → `Ghi đè`, Rename → `Đổi tên`,
Rollback → `Hoàn tác`.

Added during the `indexing.json` + `downloads.json` + `errorReporter.json` + `shortcuts.json` + `mtp.json` + `ui.json`
pass (2026-06-21, wave 1 vi batch 3). Reuses prior terms (index/indexing → `chỉ mục`/`lập chỉ mục`, scan → `quét`,
drive/volume → `ổ đĩa`, stale → `lỗi thời`, download → `tải xuống`/Tải về folder, default → `mặc định`, reset →
`đặt lại`, retry → `thử lại`, close → `đóng`, dismiss → `bỏ qua`, preview → `xem trước`, clipboard → `bảng nhớ tạm`,
network → `mạng`, server/hostname → `máy chủ`/`tên máy chủ`, device → `thiết bị`, permission → `quyền`, command palette
→ `bảng lệnh`, file list → `danh sách tệp`); new terms below, each mined from `_ignored/i18n/vi/`:

- **report (error report): `báo cáo`; error report: `báo cáo sự cố`** · MS terminology ("report" → "báo cáo"). "Error
  report" rendered `báo cáo sự cố` (sự cố = incident/issue) to keep the calm voice — avoids a bare "lỗi" status label
  per the style guide. `high` (báo cáo); `tentative` (sự cố framing for "error").
- **log / log file / logs: `nhật ký` / `tệp nhật ký`** · standard vi convention for logs (MS's `.tbx` "log" hit is a
  fragment; `nhật ký` is canonical). "Log lines" → `dòng nhật ký`; "file change log" (FS journal) →
  `nhật ký thay đổi tệp`. `high`.
- **redact / scrub (privacy): `lược bỏ` / `xóa`** · descriptive; no single source. "Redacted client-side" →
  `lược bỏ phía máy của bạn`. `tentative`.
- **send: `gửi`** · MS terminology ("send" → "gửi"). `high`.
- **process (OS process): `tiến trình`** · standard vi OS term (NOT MS's first hit "quy trình", which is the
  business-process sense — wrong here). `high`.
- **daemon: `daemon`** (loanword, kept) · no clean native UI term; macOS system-daemon names (ptpcamerad) kept literal
  alongside. `tentative`.
- **bundle (log bundle): `gói`** · natural rendering for a packaged set of files. `tentative`.
- **manifest: `bản kê`** · descriptive (a listing of contents); no single source. `tentative`.
- **event (filesystem/change event): `sự kiện`** · standard MS/vi. "events processed" → `đã xử lý ... sự kiện`. `high`.
- **buffer / channel (internal): `bộ đệm` / `kênh`** · buffer reused from settings pass (`bộ đệm`); channel → `kênh`
  (standard). `high` (buffer); `tentative` (channel).
- **watcher (file-change watcher): `bộ theo dõi`** · descriptive ("watch" → `theo dõi`, reused from downloads "watch
  your Downloads folder"). `tentative`.
- **shortcut (keyboard): `phím tắt`; modifier (key): `phím bổ trợ`** · MS terminology ("shortcut" → "phím tắt");
  modifier → `phím bổ trợ` (the ⌘/⌃/⌥/⇧ keys; descriptive, glyphs kept literal). `high` (phím tắt); `tentative` (phím bổ
  trợ).
- **register (a shortcut): `đăng ký`** · MS terminology ("register" → "đăng ký"). Reuses the sign-in word; context
  disambiguates. `high`.
- **combo / key combination: `tổ hợp` / `tổ hợp phím`** · descriptive (tổ hợp = combination); no single UI source.
  `tentative`.
- **conflict (shortcut clash): `xung đột`** · MS terminology, reused from fileOperations pass. `high`.
- **scope (shortcut group): `phạm vi`** · reused from queryUi pass; here used as section-heading framing for shortcut
  groups. `high`.
- **bind / bound (shortcut → command): `gán`** · descriptive ("bound to" → `được gán cho`). `tentative`.
- **global (shortcut scope): `toàn cục`** · standard vi for system-wide. "global shortcut" → `phím tắt toàn cục`.
  `high`.
- **jump (to a file/download): `nhảy đến`** · natural friendly rendering for the "jump to" action. `tentative`.
- **reference ID: `ID tham chiếu`** · "ID" kept verbatim; "reference" → `tham chiếu` (MS). `high`.
- **note (free-text): `ghi chú`; optional: `tùy chọn`** · MS/standard. `high`.
- **MTP / PTP / udev / USB / ptpcamerad / Terminal / daemon names: kept verbatim** · protocol/system proper names per
  the do-not-translate rule; surrounding prose translated.

`shortcuts`/`indexing`/`downloads` phrasings settled here (for consistency in other files):

- **Shortcut scope/group names**: App → `Ứng dụng`; Main window → `Cửa sổ chính`; File list → `Danh sách tệp`; Brief
  mode → `Chế độ rút gọn`; Full mode → `Chế độ đầy đủ`; Volume chooser → `Bộ chọn ổ đĩa`; Network → `Mạng`; Share
  browser → `Trình duyệt chia sẻ`; Command palette → `Bảng lệnh`; About window → `Cửa sổ Giới thiệu`; Onboarding →
  `Thiết lập ban đầu` (unified app-wide; see the `onboarding` term entry).
- **macOS feature names inside conflict warnings kept in English** (Spotlight, Mission Control, Spaces, App windows,
  Force Quit, Character Viewer): they read as proper nouns and match what a vi macOS often shows. Descriptive lowercase
  mid-sentence phrases ARE translated (the app switcher → `bộ chuyển ứng dụng`, screenshots → `chụp màn hình`, screen
  recording → `quay màn hình`, logging out → `đăng xuất`, locking the screen → `khóa màn hình`, input source switching →
  `chuyển nguồn nhập`). Finder kept verbatim; "Finder search window" → `Cửa sổ tìm kiếm Finder`.
- **System Settings > Keyboard** (macOS settings path) kept in English (matches `downloads.fda.openSystemSettings` → "Mở
  System Settings"; the pile has no clean vi string for the Keyboard pane).
- **"Almost done" → `Sắp xong`; ETA `Ns left`/`Nm left` → `còn Ns`/`còn Nm`** (the `s`/`m` abbreviations kept attached,
  "còn" = remaining, leading word per vi grammar).

**Onboarding** here was unified to `Thiết lập ban đầu` in the 2026-06-21 reconciliation pass (see the `onboarding` term
entry).

Added during the wave-1 prep pass (2026-06-21): `search` + `feedback` + `crashReporter` + `goToPath` + `transfer` +
`updates` + `lowDiskSpace` + `commandPalette` + `whatsNew` + `main` + `common` + `notifications`. Reuses prior terms
(tìm kiếm, thư mục/tệp, thùng rác, sao chép/di chuyển/đổi tên, hủy, đóng, thử lại, bảng lệnh, lệnh, đường dẫn, tải về,
khởi động lại, cập nhật, thông báo, đích, gần đây, ổ đĩa/dung lượng đĩa, Truy cập toàn bộ đĩa, Cài đặt hệ thống); new
terms below, each mined from `_ignored/i18n/vi/`:

- **crash / crash report: `sự cố` / `báo cáo sự cố`** · macOS ("problem" → "sự cố", verbatim in Finder/AppKit), MS
  ("crash" → "sự cố"). The calm framing Cmdr's error voice wants — avoids a bare "lỗi". `high`.
- **report (noun): `báo cáo`; report ID: `mã báo cáo`** · MS terminology ("report" → "báo cáo"). `high`.
- **send: `gửi`** · MS terminology ("send" → "gửi"). `high`.
- **feedback: `phản hồi`** · MS ("feedback" → "ý kiến phản hồi"; shortened to `phản hồi` for UI brevity). `high`.
- **version: `phiên bản`** · macOS Finder/AppKit ("version" → "phiên bản", verbatim). `high`.
- **changelog: `nhật ký thay đổi`** · MS terminology ("changelog" → "nhật ký thay đổi"). `high`.
- **attach: `đính kèm`** · MS terminology ("attach" → "đính kèm"). `high`.
- **character (text length): `ký tự`** · MS terminology, reused from viewer pass. `high`.
- **dismiss (close-without-action button): `bỏ qua`** · reuses the file-ops Skip/Dismiss → `bỏ qua`. macOS "dismiss" has
  no clean single string; `bỏ qua` reads natural. `high`.
- **restart: `khởi động lại`** · macOS AppKit ("Restart" → "Khởi động lại"), reused from settings pass. `high`.
- **startup disk: `đĩa khởi động`** · descriptive (boot volume); no single macOS string. `tentative`.
- **command (palette item): `lệnh`; command palette: `bảng lệnh`** · MS ("command" → "lệnh"); `bảng lệnh` reused from
  queryUi pass. `high` (lệnh); `tentative` (bảng lệnh).
- **"quit unexpectedly" (crash body): `thoát đột ngột`** · `thoát` (quit, macOS) + `đột ngột` (sudden). No single
  source; natural rendering. `tentative`.
- **build folder (e.g. node_modules): `thư mục build`** · `build` kept as a dev loanword (no clean native term; dev
  audience). `tentative`.

UI/path phrasings settled here (keep consistent across files):

- **Onboarding (menu item / wizard): `Thiết lập ban đầu`** · the unified app-wide rendering (setup sense). The menu path
  `Cmdr > Thiết lập ban đầu…` keeps the trailing ellipsis. `high`.
- **"What's new in Cmdr" (dialog title): `Có gì mới trong Cmdr`** · natural friendly framing. `tentative`.
- **Settings > Updates & privacy: `Cài đặt > Cập nhật & quyền riêng tư`** · reuses the settings-pass section name.
  `high`.
- **Settings > Updates (crash-toast button): `Cài đặt > Cập nhật`** · matches the settings-pass Updates section. `high`.
- **"Error:" prefix on a raw update-check error (`updates.checkToast.errorPrefix`): `Sự cố:`** · uses `sự cố`
  (problem/issue) not a bare "Lỗi", keeping the calm error voice. `tentative`.

Settled term decision (2026-06-21):

- **Quick Look -> `Xem nhanh`** · macOS Vietnamese · `high`. The localized Apple feature name: macOS Finder localizes it
  as "Xem nhanh" (`vi/macOS/Finder` `TL14`, sentence case; AppKit uses title-case "Xem Nhanh" — Cmdr follows Finder's
  sentence case). Applied to `commands.fileQuickLook.mac.label` and the three settings strings that reference the
  feature. Not kept verbatim because Apple does localize it for vi macOS users.

Added during the wave-1 prep pass (2026-06-21): `queue.json` (new transfer-queue window) + the new
pause/queue/background keys in `fileOperations.json` and `commands.json`. macOS Finder/AppKit Tier 1, MS terminology
Tier 2; macOS wins ties. Reuses prior terms (sao chép/di chuyển/xóa, thùng rác, hủy, đóng, thử lại, đích, con trỏ, "còn
{duration}" ETA framing). New terms below, each mined from `_ignored/i18n/vi/`:

- **pause: `tạm dừng`** · macOS AppKit (`NSPauseTemplate`/`NSTouchBarPauseTemplate` → "tạm dừng"), MS terminology (verb
  "pause" → "tạm dừng"). "Paused" (status/title) → `Đã tạm dừng`. `high`.
- **resume: `tiếp tục`** · macOS Finder ("Tiếp tục", the Continue/Resume action `66.title`). NOT the MS "resume" noun
  "sơ yếu lý lịch" (the CV/résumé sense — wrong here). `high`.
- **queue (noun): `hàng đợi`; queue (verb, "send to the queue"): `đưa vào hàng đợi`** · MS terminology ("queue" noun →
  "hàng đợi", verb → "cho vào hàng"; adapted to `đưa vào hàng đợi` for the UI action). `high`. (The window-name
  rendering that once sat here, "Transfer queue" → `hàng đợi truyền`, is SUPERSEDED: the window is now the operation
  queue, `Hàng đợi thao tác`. See the 2026-08-08 rename section at the end of this file.)
- **background / run in the background: `nền` / `chạy ở chế độ nền`** · MS terminology ("background task" → "tác vụ
  nền"). "Keep running in the background" → `giữ chạy ở chế độ nền`. `high`.
- **transfer (a copy or move, as a countable noun): `lần truyền`** · descriptive (`lần` = instance/occurrence + `truyền`
  = transfer). Still current for the NARROW copy-or-move sense (`fileOperations.transferProgress.pauseAria`,
  `settings.network.smbConcurrency.description`, the stalled-transfer strings). `tentative`. (The queue-window use that
  once sat here, heading "Transfers" → `Các lần truyền`, is SUPERSEDED by `Các thao tác`; see the 2026-08-08 rename
  section at the end of this file.)

Wave-1-prep phrasings settled (keep consistent): "Waiting" (queued status) → `Đang chờ`; "Running" → `Đang chạy`; "Done"
→ `Xong`; "Cancelled" → `Đã hủy`; "Couldn''t finish" (gentle failed wording) → `Chưa hoàn tất được` (negative-capability
framing per the error voice, avoids a bare "lỗi"/"thất bại"). "Cancel selected" → `Hủy mục đã chọn`. (The command label
that once sat here, "Show transfer queue" → `Hiện hàng đợi truyền`, is SUPERSEDED: the command now reads exactly like
the window title, `Hàng đợi thao tác`. See the 2026-08-08 rename section at the end of this file.)

Added during the navigation-and-file-ops pass (2026-06-26): the new `settings` Navigation & file ops section + the
`fileExplorer` breadcrumb tooltip and double-click-to-parent hint toast. RE-VALIDATED against the reference pile
(`_ignored/i18n/vi/`, mined 2026-06-26) after a first pass that wrongly assumed the pile absent. Two terms have a
macOS-vs-shipped-catalog split: the pile's macOS-ideal form differs from what the shipped vi catalog already uses, and
catalog consistency wins (one catalog must not carry two terms for one concept; adopting the macOS form is a
full-catalog migration, not a 14-key split). Reuses prior terms (pane → `khung`, file list → `danh sách tệp`, rename →
`đổi tên`, file ops/file operations → `Thao tác tệp`):

- **navigation / navigate (section + card heading): `điều hướng`** · macOS Finder (the `điều hướng` verb/noun, e.g.
  "Location to navigate to" → `Vị trí sẽ điều hướng đến`) and GNOME Nautilus ("Điều hướng"). Used for the `Navigation`
  card heading and the `Navigation & file ops` section (`Điều hướng & thao tác tệp`, joined with `&` like the
  `Cập nhật & quyền riêng tư` section). `high`.
- **navigate to (an action, "go to X"): `đi tới`** · macOS Finder ("Go To Folder" → `Đi tới Thư mục`, "Go To Location" →
  `Đi tới vị trí`) and the in-catalog command convention (`commands.navParent` → `Đi tới thư mục cha`,
  `commands.navGoToPath` → `Đi tới đường dẫn…`). Breadcrumb "Click to navigate to {path}" → `Bấm để đi tới {path}`.
  "Navigates to parent" rendered `đi tới thư mục cha` (matching `commands.navParent`), not the first pass's
  `lên thư mục cha`. `high`.
- **double-click: `bấm đúp`** (kept for catalog consistency) · the shipped vi catalog uses `bấm đúp`
  (`fileExplorer.network.browser.tooltip.doubleClickToConnect` = "Bấm đúp để kết nối…"), so these keys match it. Note
  `bấm đúp` already uses macOS's click verb `bấm` (macOS Finder/AppKit: `bấm` for "click", 54 occurrences, ZERO `nhấp`
  in `vi/macOS/`), paired with the common `đúp` for "double". The pile-IDEAL form is `bấm kép` (macOS `kép` = "double",
  e.g. "Gạch chân kép"; MS terminology "double-click" → `bấm kép` VNM), deferred to a full-catalog migration to avoid
  forking terminology. A stray `nhấp đúp` (`viewer.binaryWarning.body`) is a separate pre-existing catalog
  inconsistency, not touched here. `high` (catalog-consistent).
- **click (single, the action): `bấm`** · macOS Finder/AppKit (`bấm` for "click", 54×, ZERO `nhấp`). Breadcrumb "Click
  to navigate" → `Bấm để đi tới`. (The catalog also has `nhấp` in `viewer.statusBar.hint.image` — same pre-existing
  inconsistency as `nhấp đúp`.) `high`.
- **parent folder: `thư mục cha`** (kept for catalog consistency) · the shipped vi catalog uses `thư mục cha`
  (`commands.navParent` → `Đi tới thư mục cha`; multiple `errors.json` suggestions), so these keys match it. The
  pile-IDEAL form is macOS's `thư mục chứa` (macOS Finder localizes the exact up-navigation command "Go To Enclosing
  Folder" → `Đi tới thư mục chứa`, and uses it generically, "thư mục chứa tệp này"), deferred to a full-catalog
  migration to avoid forking. `high` (catalog-consistent).
- **pane: `khung`** (UPGRADES the prior `tentative` to `high`) · macOS AppKit/Finder uses `khung` for a UI pane: "Khung
  Xem trước" (Preview pane), "Khung hiện tại" (current pane). Total Commander also uses `khung`. `high`.
- **pane background: `nền khung`** · `nền` (background, macOS-attested: "màu nền chữ") + `khung` (pane, above); the
  constructed compound is sound. KDE Dolphin has the parallel concept "double clicking view background" (untranslated in
  vi, but it confirms the "background" framing). `high`.
- **hint (one-time UI hint/tip): `gợi ý`** · macOS ("Cửa sổ gợi ý" = hint window, "Gợi ý mật khẩu" = password hint); MS
  terminology "hint" Noun. `high`.
- **empty space (in a list): `khoảng trống`** · natural rendering ("the empty space in a file list" → "khoảng trống
  trong danh sách tệp"; "the empty space around the file list" → "khoảng trống xung quanh danh sách tệp"). `high`.
- **row (file-list row): `hàng`** · Cmdr's own catalog already uses `hàng` for file-list rows ("Hàng sọc xen kẽ" =
  alternating striped rows; "Bộ đệm ảo hóa (hàng)" / "Số hàng dựng thêm phía trên và dưới vùng hiển thị" = list
  virtualization rows), reserving `dòng` for text LINES in the viewer ("{count} dòng", "ngắt dòng"). MS terminology
  agrees (row → `hàng`). "a file row" → `một hàng tệp`. KDE Dolphin's `dòng` ("click anywhere on the row" → "trong
  dòng") is overridden by Cmdr's own established `hàng`. `high` (catalog-consistent).

A later copy revision (2026-06-26, David picked shorter wording) reworded this switch's label + description; the keys
now read: label `Bấm đúp vào nền khung để lên thư mục cha` ("go up a folder" → `lên thư mục cha`, the shorter
directional form), description `Đó là khoảng trống xung quanh danh sách tệp, không phải một hàng tệp.` ("That''s…, not a
file row").

Phrasings settled this pass (double-click-to-parent hint toast, casual/friendly product voice — free copy, no single
pile source):

- **"What just happened?" → `Chuyện gì vừa xảy ra?`** · natural friendly framing. `tentative`.
- **"Don''t like it?" → `Không thích à?`** · `à` casual softening particle; matches the friendly voice. `tentative`.
- **"Never do this again" (button) → `Đừng làm vậy nữa`** · natural imperative. `tentative`.
- **"I like it" (primary button) → `Tôi thích`** · the user speaking in first person; `Tôi` (I) here, not the app''s
  `bạn`. `tentative`.
- **"This navigates to the parent folder" (hint body) → `Thao tác này đưa bạn đến thư mục cha`** · `đưa bạn đến` (takes
  you to) for a friendly, concrete rendering. `tentative`.
- preset (value in a settings-picker dropdown) → đặt trước, rendered as "tùy chọn đặt trước" (preset options); "back to
  presets" → "Quay lại tùy chọn đặt trước" · Microsoft terminology (preset → "đặt trước", e.g. "khung thời gian đặt
  trước"); "tùy chọn" (options) heavily attested. "đặt trước" can also read as "reserved", so pairing it with "tùy chọn"
  disambiguates · tentative

Added during the filesystem-size-guard pass (2026-06-30): the FAT32 file-too-large error
(`errors.write.filesTooLargeForFilesystem.*`) + the `fileOperations.errorDialog.tooLargeAndMore` count line. Reuses
prior terms (tệp, ổ đĩa = drive, không thể = can''t). New terms below, each mined from `_ignored/i18n/vi/`:

- **too large (for X): `quá lớn (đối với X)`** · GNOME Nautilus ("Tập tin quá lớn đối với vị trí dán" = "File too large
  for the paste location"), a near-exact structural parallel. `đối với` = "for / with respect to". `high`.
- **format (filesystem format, noun): `định dạng`; "formatted as FAT32": `được định dạng FAT32`** · macOS Finder Get
  Info ("Định dạng:" = "Format:"). The passive state "is formatted as" → `được định dạng` (no "as" word needed). `high`.
- **FAT32 / exFAT: kept verbatim** · filesystem-format names; not translated (per the en `@key` note). `high`.
- **limit (size/quota limit): `giới hạn`; "no such limit": `không có giới hạn như vậy`** · GNOME/Xfce/MS terminology
  ("Không giới hạn" = "No limit", "không có giới hạn"). `high`.
- **store / hold (a drive holding files): `chứa`** · `chứa` (contain/hold) for a drive storing files; "can''t store
  files larger than X" → `không thể chứa các tệp lớn hơn {maxSize}`. Reads more natural than `lưu trữ` (archive) for a
  drive''s capacity. `tentative` (no single pile source; natural rendering).
- **larger than: `lớn hơn`** · standard comparative; `lớn` (large) + `hơn` (more/than). `high`.
- **"{name} is {size}" (size statement): `{name} có dung lượng {size}`** · `có dung lượng` (has a size of), matching
  macOS Get Info "Dung lượng:" (Size:). `high`.
- **"files this large": `các tệp lớn cỡ này`** · `cỡ này` (of this size) — casual, everyday rendering. `tentative`.
- **"and N more files" (overflow count line): `và thêm {countText} tệp nữa`** · `và thêm … nữa` = "and … more"; noun
  uninflected (vi has one plural category, `other`). `high`.
- preset (value in a settings-picker dropdown) → đặt trước, rendered as "tùy chọn đặt trước" (preset options); "back to
  presets" → "Quay lại tùy chọn đặt trước" · Microsoft terminology (preset → "đặt trước", e.g. "khung thời gian đặt
  trước"); "tùy chọn" (options) heavily attested. "đặt trước" can also read as "reserved", so pairing it with "tùy chọn"
  disambiguates · tentative

Added during the dialog-polish pass (2026-06-30): short labels / tooltips in `fileOperations.json` (the copy/move +
delete dialogs). Reuses prior terms (scan/scanning → `quét`/`đang quét`, source → `nguồn`, destination → `đích`, file
ops → `Thao tác tệp`):

- **action (what a control chooses; screen-reader label `transferDialog.operationAria`): `Thao tác`** · the catalog''s
  established operation term (`Thao tác tệp` = file operations). Names which operation to run; `thao tác` (operation,
  user-performed action) reads more natural here than MS''s `hành động` (action, behavioral sense) or macOS''s `tác vụ`
  (task; macOS uses it for "undo this action"). Catalog-consistent. `high`.
- **"Scanning…" (spinner tooltip / SR label while counting): `Đang quét…`** · reuses the glossary''s "scan in progress"
  → `Đang quét`; ellipsis `…` kept. `high`.
- **"This folder doesn''t exist yet. Cmdr will create it during the copy/move." (yellow inline warning under the
  destination box): `Thư mục này chưa tồn tại. Cmdr sẽ tạo nó khi sao chép.` / `… khi di chuyển.`** · `chưa tồn tại`
  (not-yet-exist) is the precise "doesn''t exist yet" counterpart to the catalog''s `đã tồn tại` (already exists); GNOME
  Nautilus attests plain `không tồn tại` ("đích đến là "%s" không tồn tại") and `chưa` for "not yet". `tạo nó` (create
  it, inanimate pronoun) is attested in the pile (Nautilus "không có quyền tạo nó ở đích đến");
  `khi sao chép`/`khi di chuyển` (when copying/moving) renders "during the copy/move" concisely. Two literal sentences
  per the en `@key` (no ICU select; the verb is operation-specific). `high`.
- **queue.row.label progress arms (rename / create folder / create file)** · `Đang đổi tên` / `Đang tạo thư mục` /
  `Đang tạo tệp` · "Đang [verb]" style of the sibling arms; Nautilus ("Đang đổi tên", "Đang tạo"), settled `đổi tên`,
  `thư mục`/`tệp` · high

Added during the archive-browsing pass (2026-07-05): the 28 archive/bundle keys (browse-into-zip feature). Reuses prior
terms (browse → `duyệt`, folder → `thư mục`, file → `tệp`, open → `mở`, default → `mặc định`, read-only → `chỉ đọc`,
trash → `thùng rác`, delete → `xóa`, copy/move → `sao chép`/`di chuyển`, permanently/for good → `vĩnh viễn`, format →
`định dạng`, preview → `xem trước`, can't → `không thể`). New terms below:

- **archive (a zip/tar/7z browsed like a folder): `tệp nén`** · Cmdr's OWN catalog already renders compressed/archive
  files as `tệp nén` (`settings.listing.sizeDisplay.description` "tệp nén", `settings.fileViewer.suppressBinaryWarning`
  "tệp nén"), so these keys MATCH it (catalog-consistency, the no-forking-terminology rule). Corroborated by GNOME
  Nautilus + MS terminology "nén" (compress) and macOS Finder "đã nén" (compressed). Deliberately NOT the archival
  `kho lưu trữ` (GNOME's "Archive" menu) or `Bộ lưu trữ` (macOS "iOS Package Archive"): both read as backup/storage, the
  wrong register for a browsable zip. Renders zip/tar/7z generically; "zip archives" → `tệp nén zip`, "archive format" →
  `định dạng nén`. `high` (catalog-consistent).
- **app bundle: `gói ứng dụng`** · macOS Finder `gói` for package/bundle ("iOS Package Archive" → "Bộ lưu trữ gói iOS")
  - `ứng dụng` (app, macOS Finder "Ứng dụng"). The .app/.bundle/.framework folders macOS shows as one item. `high`.
- **extract (an archive): `giải nén`** · Cmdr's own catalog (`ai.local.installStepExtracting` "Đang giải nén"), GNOME
  Nautilus ("Giải nén"). Used in `readOnly.archiveMessage` ("browses and extracts" → "duyệt và giải nén"). `high`.
- **edit (an archive's contents): `chỉnh sửa`** · catalog reserves `chỉnh sửa` for "edited" (`errors` "đã bị chỉnh sửa
  bên ngoài git") vs `sửa đổi` for the "modified" date; editing zip entries is the `chỉnh sửa` sense.
  `readOnly.archiveMessage` "can be edited" → "có thể chỉnh sửa"; `queue.row.label` `archive_edit` "Editing archive" →
  `Đang chỉnh sửa tệp nén`. `high`.
- **configure (menu item): `Cấu hình`** · MS terminology ("configure" → "cấu hình", many hits). Trailing `…` kept (opens
  Settings). `high`.
- **damaged / corrupt (of a file): `hỏng`** · catalog's established term (`errors` "đĩa đang hỏng dần", "vùng hỏng").
  "It may be damaged" → "Có thể tệp bị hỏng". `high`.
- **encrypted: `được mã hóa` / `bị mã hóa`** · catalog (`errors.provider.veraCrypt` "ổ đĩa được mã hóa"). `high`.
- **default app: `ứng dụng mặc định`** · `mặc định` (default, settings pass) + `ứng dụng` (app). `high`.
- **fresh copy (of a file): `một bản mới`** · `bản` (copy/version) + `mới` (new); "ask whoever sent it for a fresh copy"
  → "nhờ người đã gửi nó cung cấp một bản mới". Natural rendering, no single pile source. `tentative`.
- **pressing Enter (does X): `nhấn Enter (sẽ làm gì)`** · catalog convention `nhấn Enter` (`queryUi` "nhấn Enter để tìm
  kiếm"); Enter key name kept verbatim (macOS vi keeps "Enter"). "What pressing Enter does" → "Nhấn Enter sẽ làm gì".
  `high`.
- **ask (each time / on Enter): `hỏi`** · segmented-control opt + `enterBehavior` "ask each time" → "hỏi mỗi lần". macOS
  Finder attests `hỏi` in prompts. `high`.

Added during the paste-clipboard-as-file pass (2026-07-07): the 7 keys for pasting non-file clipboard content (text,
image, PDF) into a folder as a new file. Reuses prior terms (paste → `dán`, clipboard → `bảng nhớ tạm`, file → `tệp`,
image → `hình ảnh`, folder → `thư mục`, rename → `đổi tên`, copy → `sao chép`, Settings → `Cài đặt`, hold/contain →
`chứa`, ⌘V kept verbatim). New terms below:

- **content (of the clipboard): `nội dung`** · Cmdr's own catalog uses `nội dung` throughout for content (e.g.
  `settings.listing.sizeDisplay` "kích thước nội dung", `dirSize.contentLabel` "Nội dung"), and MS terminology attests
  it. "Paste clipboard content as a file" → `Dán nội dung bảng nhớ tạm thành tệp`. `high`.
- **text (clipboard content, not text lines): `văn bản`** · the shipped vi catalog already uses `văn bản` for text
  (`settings.developer`/`fileViewer` "nội dung phi văn bản" = non-text content); MS "text" → "văn bản". Distinct from
  `dòng` (text LINES in the viewer). The `other` (non-image, non-PDF) branch of `clipboard.pastedAsFile` → `văn bản`.
  `high` (catalog-consistent).
- **as a file / into a file (result form): `thành tệp` / `thành {filename}`** · `thành` (into/becomes) for the
  transform-into-a-file sense; the catalog also attests `dưới dạng` for "as" (git portal "dưới dạng thư mục ảo"), but
  `thành` is the tighter fit for content turning into a file and reads shorter in a label. `high`.
- **do nothing (radio option): `Không làm gì`** · plain negation of `làm` (do); natural. `high`.

Settled during the archive-password dialog pass (encrypted-zip unlock modal `fileOperations.archivePassword.*`,
2026-07-08):

- password-protected → `được bảo vệ bằng mật khẩu` · TC/DC vi phrasing · high. Body: "… được bảo vệ bằng mật khẩu."
- password (noun) → `Mật khẩu` · macOS/MS · high.
- unlock (button + verb) → `Mở khóa` · macOS AppKit ("Mở khóa") · high.
- archive (the `{name}` head / input label) → `tệp nén` (compressed file) · settled vi glossary · high. Input aria-label
  "Mật khẩu tệp nén".

Settled while translating the Compress feature:

- compress (verb / control label) → `Nén` · Finder `vi/macOS` ("Nén các mục", `Compress ${sources}` → "Nén ${sources}")
  · high. Used for `commands.fileCompress.label`, `toggleCompress`, `confirmCompress`, and both title-verb branches.
- compressing (progress form) → `Đang nén` · derived on the sibling `Đang sao chép`/`Đang di chuyển` · high.
  `scanTitleCompress` = "Đang xác minh trước khi nén...".
- compressed (result toast) → `Đã nén` · mirrors `transfer.split.clean` ("Đã sao chép {phrase}") · high. Plural uses
  only the `other` CLDR category (Vietnamese has no plural distinction), matching the sibling toasts.
- replace (overwrite warning) → `thay thế` · Finder `Replace` → "Thay thế" · high.
- archive (name) → `tệp lưu trữ` · Finder `Zip archive` → "Tệp lưu trữ Zip" · high. `.zip` in straight double quotes.
- compression level (slider label) → `Mức nén` · TC `vi` "Sự nén ZIP nội (0-9)"; `mức` (level) + `nén` (compress),
  standard vi 7-Zip `Mức nén` · high. `settings.archives.compressionLevel.label`.
- faster (slider low end, level 1) → `Nhanh hơn` · TC `vi` "nén nhanh nhất (1)" (root `nhanh`) · high. Marks quicker
  packing, not app speed. `.faster`.
- smaller (slider high end, level 9) → `Nhỏ hơn` · pairs with `Nhanh hơn`; marks the smaller output file (TC `vi` high
  end "nén tối đa") · high. `.smaller`.
- No `sameAsSourceJustification` needed: all values differ from English.

Settled while translating the Operation log feature (alpha `operationLog.json` + `commands.logOperationLog.*`,
2026-07-09). Reuses prior terms (sao chép/di chuyển/xóa/đổi tên/nén, thùng rác, tệp/thư mục, thử lại → `thử lại`, close
→ `đóng`, không thể, agent → `tác nhân`, archive → `tệp nén`, extract → `giải nén`, edit archive → `chỉnh sửa`). New/
confirmed terms below:

- **operation (a file operation, as a logged event): `thao tác`** · macOS Finder (`thao tác di chuyển ^0 mục`,
  `thao tác chưa hoàn tất`, `Thao tác lưu tệp`), matching the catalog''s `Thao tác tệp` (file operations) and the
  `Thao tác:` action-field label. `high`.
- **operation log (dialog title + command label): `Nhật ký thao tác`** · `nhật ký` (log, settled glossary term) +
  `thao tác` (operation, macOS). `high`. Used for `operationLog.dialog.title` and `commands.logOperationLog.label`.
- **history (operation history): `lịch sử`** · macOS (`NSToolbarHistoryTemplate` → "lịch sử", "lịch sử phiên bản" =
  version history), MS ("Nhật ký Lịch sử"). "your operation history" → `lịch sử thao tác của bạn`. `high`.
- **roll back / rollback (reverse a completed operation): `hoàn tác`** · macOS AppKit Undo → "Hoàn tác"; catalog already
  renders the file-ops Rollback button as `Hoàn tác`. Catalog-consistent, so the whole rollback state set uses it:
  "Can''t roll back" → `Không thể hoàn tác`, "Can roll back" → `Có thể hoàn tác`, "Rolling back" → `Đang hoàn tác`,
  "Rolled back" → `Đã hoàn tác`, "Partly rolled back" → `Đã hoàn tác một phần`. "roll them back" (command description) →
  `hoàn tác chúng` (`chúng` = them, inanimate; pile attests `tạo nó`). `high` (catalog-consistent).
- **client (AI client, an external app over the automation interface): `máy khách`** · MS/standard client-server term
  (counterpart to `máy chủ` = server). "AI client" → `Máy khách AI` (AI kept verbatim). `high`.
- **item (generic logged item, vs file/folder): `mục`** · macOS Finder ("các mục", "^0 mục"). English keeps item generic
  (distinct from file → `tệp`, folder → `thư mục`); the summary lines use `mục`. `high`.

Operation-log status/outcome set, aligned to `queue.json` for catalog consistency (queue lifecycle already ships these):
Queued → `Đang chờ`; Running → `Đang chạy`; Done → `Xong`; Canceled → `Đã hủy`; "Didn''t finish" (gentle failed, avoids
"lỗi"/"thất bại") → `Chưa hoàn tất được` (macOS also attests `thao tác chưa hoàn tất`). Per-item outcomes: Skipped →
`Đã bỏ qua` (past aspect, matches the other completed-aspect outcomes); Done → `Xong`; "Didn''t finish" →
`Chưa hoàn tất được`; "Rolled back" → `Đã hoàn tác`. Initiator provenance: You → `Bạn`; AI client → `Máy khách AI`;
Agent → `Tác nhân`. Summary verbs reuse the transfer past-tense forms (`Đã sao chép`/`Đã di chuyển`/`Đã xóa`/
`Đã chuyển … vào thùng rác`/`Đã đổi tên`/`Đã tạo`/`Đã nén`). Plurals collapse to a single `other` branch (vi has one
CLDR category), keeping the `{count}`/`{countText}` placeholders. "and N more items" → `và thêm {countText} mục nữa`. No
`sameAsSourceJustification` needed: all values differ from English.

Settled while translating Ask Cmdr (the read-only AI chat rail: `askCmdr.json` + `settings.askCmdr.*` +
`settings.advanced.logLlmCalls.*` + `settings.section.askCmdr` + `commands.askCmdrToggle.*`, 2026-07-13). Reuses prior
terms (chỉ đọc = read-only, thao tác/lịch sử thao tác = operation/operation history, nhà cung cấp = provider, mô hình =
model, token loanword, hạn ngạch = quota, giới hạn = limit, cục bộ = local, cơ sở dữ liệu = database, không thể =
can''t, sự cố = issue/problem framing, thử lại = retry, ổ đĩa = drive, con trỏ = cursor, mục = item, khóa API = API key,
nhật ký = log, đóng = close, đổi tên = rename, mục đã chọn = selected item(s), đính kèm = attach/attachment, Cài đặt =
Settings, Nâng cao = Advanced). New terms below, each mined from `_ignored/i18n/vi/`:

- **chat (a conversation with the AI, noun): `trò chuyện`** · MS terminology (`chat` → `trò chuyện`, VNM). Used as the
  section/nav noun ("Chats" → `Trò chuyện`) and in compounds ("New chat" → `Trò chuyện mới`, "Back to chat" →
  `Quay lại trò chuyện`). Vietnamese has no plural inflection so the same noun covers "chat"/"chats". `high`.
- **message (a chat message): `tin nhắn`** · MS terminology (`message` → `tin nhắn`, VNM). "Send message" →
  `Gửi tin nhắn`; "Load earlier messages" → `Tải tin nhắn trước đó`. `high`.
- **archive (a chat, verb) / archived: `lưu trữ` / `đã lưu trữ`** · macOS Finder (key `AR40`, `Archive` → `Lưu trữ`,
  cross-referenced key-to-key en↔vi). Deliberately NOT the browsable-zip sense `tệp nén` (a different concept — hiding a
  conversation from the active list, not compressing a file). `high`.
- **unarchive (a chat): `bỏ lưu trữ`** · no direct pile hit (the pile has no chat/mail app); mirrors the common
  Vietnamese-software convention for undoing an archive action (Gmail/Zalo-style `Lưu trữ`/`Bỏ lưu trữ` pairing).
  `tentative`.
- **attachment (a staged file/folder attached to a question, noun): `tệp đính kèm`** · MS terminology (`attachment` →
  `đính kèm`, VNM), combined with the established `tệp`/`thư mục` nouns. "Remove attachment" → `Gỡ tệp đính kèm` (`gỡ` =
  detach, distinct from `xóa` = delete; the attachment is unstaged, not deleted). `high`.
- **Not now (decline/dismiss button): `Để sau`** · macOS AppKit (`Not Now` → `Để sau`, `en/macOS/AppKit/Document.json`).
  `high`.
- **database: `cơ sở dữ liệu`** · MS terminology (`database` → `Cơ sở dữ liệu`, VNM). "local database" →
  `cơ sở dữ liệu cục bộ`. `high`.
- **dashboard (a provider's billing dashboard): `bảng thông tin`** · MS terminology (`dashboard` → `bảng thông tin`,
  VNM). `high`.
- **bill (verb, "your provider bills you"): `thanh toán`** · MS terminology (`billing` → `thanh toán`, VNM). "bills you
  directly" → `thanh toán trực tiếp với bạn`. `high`.
- **free (cost-free, not gratis-as-liberty): `miễn phí`** · standard everyday Vietnamese for "free of charge"; NOT MS's
  first hit `tự do` (the liberty/freedom sense, wrong here — mining gotcha 4). "free, on-device" → `miễn phí, cục bộ`
  (reuses `cục bộ` = local from `settings.ai.provider.opt.local` → `LLM cục bộ`). `high`.
- **reach (couldn't reach the provider): reframed as `kết nối` (connect)** · macOS Finder
  (`Could not connect to the server.` → `Không thể kết nối máy chủ.`, keys `CS204`/`CS208`) is the closest structural
  parallel for an unreachable-network-endpoint sentence; there's no literal "reach" verb in the pile, so the sentence is
  restructured around the attested "couldn't connect" pattern rather than translated word-for-word. `high` (structural
  match).
- **estimate / estimated (a spend estimate, adjective/adverb use): `ước tính`** · natural rendering; NOT MS's `báo giá`
  (a price quotation, the sales-quote sense — wrong here). "about {amount}" → `khoảng {amount}`; "These are estimates" →
  `Đây chỉ là ước tính`. `tentative` (no direct pile string for this UI sense, quotation sense rejected).
- **cost (noun, chat spend): `chi phí`** · standard Vietnamese for a general cost/expense; NOT MS's `giá vốn` (cost of
  goods sold, an accounting term — wrong here). "cost unknown" → `chi phí không rõ`. `tentative`.
- **spending (settings section heading): `chi tiêu`** · standard Vietnamese for personal/app spending. `tentative` (no
  direct pile hit; natural rendering).
- **usage (token/spend usage): `sử dụng` / `mức sử dụng`** · standard Vietnamese tech usage; "This chat's usage" →
  `Mức sử dụng của cuộc trò chuyện này`. `tentative`.
- **debugging (verb, "for debugging"): `gỡ lỗi`** · standard Vietnamese dev term. `tentative` (no direct pile hit;
  universal dev-audience convention).
- **working (generic tool-call fallback status): `đang xử lý`** · natural present-tense fallback ("processing"), used
  only when no specific tool label applies. `tentative`.
- **look up (a logged operation's detail, verb): `tra cứu`** · standard Vietnamese for looking up a record. "Looking up
  an operation" → `Đang tra cứu một thao tác`. `high`.
- **available (a tool request that wasn't possible, read-only refusal): `khả dụng`** · standard Vietnamese IT adjective.
  "That request wasn't available" → `Yêu cầu đó không khả dụng` (avoids "lỗi"/"thất bại" per the error voice). `high`.

`askCmdr` phrasings settled here (for consistency in other files):

- **"Chats" (nav/heading, both the rail-header button and the sessions-panel title) → `Trò chuyện`**; "New chat" →
  `Trò chuyện mới`; "Start a fresh chat" → `Bắt đầu trò chuyện mới`.
- **"file history" (the operation log, as referenced from Ask Cmdr's tool descriptions) → `lịch sử thao tác`**, not a
  literal `lịch sử tệp` — Ask Cmdr's file history tool reads the operation log (past copies/moves/deletes/renames), so
  this reuses the `operationLog` pass's `lịch sử thao tác` rather than coining a new "file history" term. "Searching
  your file history" → `Đang tìm kiếm trong lịch sử thao tác của bạn`. Confidence: `high` (catalog-consistent with the
  `operationLog` pass). Note: this is a small tension with the app-facing English string "file history", which reads
  slightly broader than "operation history" — flagged here so a future pass doesn't fork the term if the English copy is
  ever split.
- **"Try again?" (short retry question, several error strings): `Thử lại?`** kept as a plain question (no softening
  particle) for consistency across the five error strings that use it.
- **Consent-screen items ("Sentence case, no period" per the en `@key` notes) keep no trailing period** in Vietnamese,
  matching the English constraint.

Added during the network-drive image-indexing pass (2026-07-13): the 19 `settings.mediaIndex.networkVolumes.*` /
`.alwaysIndex*` keys + `search.imageResults.networkOff`/`.paused` (opting an SMB drive into background photo-content
indexing so photos become searchable by the text inside them). Reuses prior terms (index/indexing → `chỉ mục`/
`lập chỉ mục`, indexed → `đã lập chỉ mục`, drive/volume → `ổ đĩa`, network → `mạng`, folder → `thư mục`, image →
`hình ảnh`, text → `văn bản`, search → `tìm kiếm`, pause/paused → `tạm dừng`/`Đã tạm dừng`, resume → `tiếp tục`,
disconnect → `ngắt kết nối`, browse → `duyệt`, background → `ở chế độ nền`, Settings → `Cài đặt`, Mac → `Mac` verbatim,
"in the background" → `ở chế độ nền` per `settings.indexing.enabled.description`, "Internal:" → `Nội bộ:`). New terms
below:

- **photo (vs image): `ảnh`** · macOS (`Chọn ảnh` = Choose Photo, `Cắt ảnh` = Crop photo; Apple's Photos app is `Ảnh`).
  Deliberately distinct from the feature-level "image" → `hình ảnh` (`settings.mediaIndex.card` = `Tìm kiếm hình ảnh`,
  `enabled.label` = `Lập chỉ mục nội dung hình ảnh`): the English copy itself splits "image" (feature/card) from
  "photos" (the concrete per-drive strings), and `ảnh` is the natural concrete word. "photos indexed" →
  `Đã lập chỉ mục … ảnh`; "photos on {name}" → `ảnh trên {name}`. `high`.
- **network drive: `ổ đĩa mạng`** · `ổ đĩa` (drive) + `mạng` (network), both settled. `high`.
- **reconnect: `kết nối lại`** · macOS (`Để kết nối lại, hãy bấm…`). "resumes when this drive reconnects" →
  `sẽ tiếp tục khi ổ đĩa này kết nối lại`. `high`.
- **photo archive (a rarely-browsed NAS of photos): `kho ảnh`** · `kho` (store/archive, the archival-storage sense, NOT
  the browsable-zip `tệp nén`) + `ảnh`. Register matches "an archive you rarely browse". `tentative` (constructed
  compound; no single pile source).
- **gently (reads photos gently): `một cách nhẹ nhàng`** · adverbial rendering; no pile hit. `tentative`.
- **at a limited speed: `ở tốc độ giới hạn`** · `tốc độ` (speed, macOS "Tốc độ ghi đĩa") + `giới hạn` (limit, settled).
  `high`.
- **so far (photos indexed so far): `cho đến nay`** · standard temporal phrasing. `high`.
- **mark (a drive/folder, internal): `đánh dấu`** · `đánh dấu` (mark). Internal dev strings for the always-index lists.
  `high`.
- **The `indexed` ICU plural collapses to a single `other` branch** (vi has one CLDR category), keeping both `{count}`
  (selector) and `{countText}` (preformatted display): `{count, plural, other {Đã lập chỉ mục {countText} ảnh}}`.
- No `sameAsSourceJustification` needed: all 19 values differ from English.

Quality-review pass over the 54 keys of the bulk-rename review, image-index scope, and Ask Cmdr tool labels (2026-07-21;
the keys had been translated mid-feature without the process, so this pass re-mined them against `_ignored/i18n/vi/`).
Reuses prior terms (đổi tên, tệp/thư mục, ghi đè, hủy, ảnh vs hình ảnh, chỉ mục/lập chỉ mục, quét, ổ đĩa, thùng rác, thử
lại, không thể, gỡ, mức độ quan trọng). New or newly-sourced below:

- **review (verb + the review modal): `xem lại`** · macOS AppKit (`Review Changes…` → `Xem lại Thay đổi…`,
  `Review Unsaved Items` → `Xem lại Mục chưa lưu`, and the running "Nếu bạn không xem lại…" alerts), sentence-cased for
  Cmdr. Confirms the modal title `Xem lại việc đổi tên tệp` and "this review" → `Lần xem lại này`. `high`.
- **allow / deny (per-row approval buttons): `Cho phép` / `Từ chối`** · MS terminology (`deny` Verb → `từ chối`;
  `Allow …` entries → `Cho phép …`), macOS Finder AirDrop (`Decline` → `Từ chối`). "Allow all"/"Deny all" →
  `Cho phép tất cả` / `Từ chối tất cả`. `high`.
- **"this rename" (one proposed row): `lần đổi tên này`** · `lần` (instance/occurrence, the catalog's counter for a
  single operation, as in `lần truyền` = a transfer). Unified across the row message, the overwrite tooltip, and the SR
  status line; the generic heading keeps the gerund `việc đổi tên tệp`. `high` (catalog-consistent).
- **"needs attention" (blocked row): `cần được xem lại`** · no `chú ý` anywhere in the macOS pile, and the passive "cần
  được chú ý" reads stilted; `xem lại` (above) is both attested and the action the modal asks for. `high`.
- **rename cycle (a → b → a dependency loop): `chu trình đổi tên`** · `chu trình` is the graph-theory "cycle" in
  Vietnamese. MS's `chu kỳ` (time cycle) and `vòng tròn` (SmartArt circle) are the wrong senses (mining gotcha 2), and
  the pile has no file-manager string for it. Badge `(chu trình)`, tooltip explains it. `tentative`.
- **filename extension: `đuôi tệp`** (kept) · the shipped vi catalog uses `đuôi tệp` throughout
  (`Cho phép đổi đuôi tệp`, `Đổi đuôi tệp?`, the `Đuôi` column), so the badge `(đuôi tệp)` and its tooltip match it.
  macOS's fuller `phần mở rộng tệp` is the pile-ideal form but adopting it is a full-catalog migration, not a two-key
  split, and it's too long for a compact badge. `high` (catalog-consistent).
- **remove (from a list, not a deletion): `Gỡ`** (kept) · GNOME Nautilus (`Gỡ biểu tượng tự chọn` = Remove custom icon),
  and the catalog's `Gỡ tệp đính kèm`. Deliberately NOT macOS's `Xóa` (`Xóa khỏi thanh bên` = Remove from Sidebar):
  `xóa` is Cmdr's delete verb, and this button's own help text promises nothing is deleted. `high`.
- **image (the feature-level word) vs photo: `hình ảnh` vs `ảnh`** · applied to the whole `fileExplorer.imageIndex.*`
  status-bar family, which had drifted to `ảnh`. English splits them deliberately, and the same feature's settings pane
  already ships `Tìm kiếm hình ảnh` (the card) and `Lập chỉ mục nội dung hình ảnh`, so the pane labels and their
  tooltips now read `hình ảnh` and only the concrete per-drive photo counts keep `ảnh`. `high`.
- **folder/file size: `kích cỡ`** (not `kích thước`) · `fileExplorer.json` uses `kích cỡ` for every listing and
  drive-index size string (`kích cỡ thư mục`, `Không rõ kích cỡ`), so the two drive-index coalesced tooltips were
  aligned to it. `kích thước` stays for the physical-dimension sense (`Đổi kích thước khung` = resize panes). `high`
  (catalog-consistent).
- **indexing pass: `lượt quét`** · `lượt` (round/turn) + the settled `quét` (scan); "on the next pass" →
  `ở lượt quét tiếp theo`. `tentative` (no pile string; constructed on settled parts).
- **"Ask Cmdr to prepare it again": `Hãy nhờ Cmdr chuẩn bị lại.`** · `nhờ` (ask someone to do something as a favor) is
  the natural verb for asking a helper; `yêu cầu` (demand/request) reads formal and made the rail's brand name ("yêu cầu
  Ask Cmdr") read as an object. The English is a deliberate double reading (the imperative "ask Cmdr" and the feature
  name); Vietnamese keeps the imperative one, and `Cmdr` stays verbatim. `tentative`.
- No `sameAsSourceJustification` needed anywhere in these 54: every value differs from English.

Added during the image-index-indicator pass (2026-07-22): the 13 new keys for the per-file / per-folder / per-drive
image-search status badges (`fileExplorer.imageIndex.file.*`, `.folder.*`, `.drive.*` +
`settings.mediaIndex.showFileStatusIcons.*`). Reuses prior terms (image, feature-level → `hình ảnh` per the 2026-07-21
decision on the whole `fileExplorer.imageIndex.*` family; image search → `tìm kiếm hình ảnh` =
`settings.mediaIndex.card`; index/indexed/re-index → `lập chỉ mục`/ `đã lập chỉ mục`/`lập chỉ mục lại`; drive → `ổ đĩa`;
file list → `danh sách tệp`; scope → `phạm vi`; can't → `không thể`; "of" in a count → `trên`, matching
`settings.mediaIndex.progress.ofTotal`; toggle off → `tắt`). New/confirmed below:

- **status badge (the small per-file indicator): `huy hiệu`** · reuses the settled toast/chip/**badge** → `huy hiệu`
  rendering. "status badge" → `huy hiệu trạng thái`. `tentative` (descriptive; no single pile source).
- **status (state indicator): `trạng thái`** · standard vi UI term (MS/macOS convention). "Image search status" →
  `Trạng thái tìm kiếm hình ảnh`. `high`.
- **waiting (queued to be indexed): `Đang chờ`** · reuses the queue lifecycle "Waiting" → `Đang chờ`. "Waiting to be
  indexed" → `Đang chờ lập chỉ mục`. `high` (catalog-consistent).
- **is off (a feature turned off for a drive): `đang tắt`** · the toggle `bật/tắt` verb in its present-state form.
  "Image search is off for this drive." → `Tìm kiếm hình ảnh đang tắt cho ổ đĩa này.` `high`.
- **still working (an indexing pass in progress): `vẫn đang xử lý`** · reuses the `đang xử lý` (processing) fallback
  status. `tentative`.
- **The four ICU-plural keys (`folder.allIndexed`/`someIndexed`, `drive.indexing`/`done`) collapse to a single `other`
  branch wrapping just the noun** (`{total, plural, other {hình ảnh}}`; vi has one CLDR category), keeping the full
  placeholder set. `drive.indexing` fronts the drive ("Trên ổ đĩa này, …") to avoid a double `trên` (of / on this
  drive).
- No `sameAsSourceJustification` needed: all 13 values differ from English.

Added during the dialog-polish pass (2026-07-23): the delete dialog swapped its Thùng rác/Xóa picker for a "Move to
trash" switch plus a matching confirm button, and the copy/move/compress dialog groups the source path and the
destination volume+path under "From" and "To" headings.

- **"Move to trash" (`delete.trashSwitch`; switch in the delete dialog, on = thùng rác, off = permanent delete):
  `Chuyển vào thùng rác`** · identical to this file''s `transferDialog.titleVerbOnly` `other {Chuyển vào thùng rác}`
  arm, so the switch and the confirm button read as one pair; macOS Finder vi AL13/N153 `Chuyển vào Thùng rác` confirms
  the phrase. The catalog capitalizes `Thùng rác` only where it names the Trash location itself ("check the Trash"), and
  lowercases it inside an action phrase. `high`.
- **"Delete" (`delete.confirmDelete`; destructive confirm button while the switch is off): `Xóa`** · settled delete
  verb, identical to `transferDialog.titleVerbOnly`''s `delete {Xóa}` arm. `high`.
- **"From" / "To" (`transferDialog.sourceGroupTitle` / `targetGroupTitle`; headings over the source path and over the
  destination volume + path): `Từ` / `Đến`** · Total Commander vi ships this exact label pair in its copy/move dialog
  (entries 662/663; the `.LNG` file sits in the pile as UTF-8 misread as Latin-1, so decode before reading it); macOS
  "Move To" = `Di chuyển đến` confirms `đến` for a destination. The settled nouns `nguồn` / `đích` stay for the
  destination CONTROLS (`Ổ đĩa đích`, `Đường dẫn đích`); the headings take the light prepositional pair the English
  uses. `high`.

Reviewed during the master-drive-indexing-switch pass (2026-07-27): the five keys that explain why the per-drive index
controls are overridden while the master switch is off (`fileExplorer.navigation.driveIndex.refusedIndexingOff` /
`.tooltipIndexingOff` / `.menuIndexingOffNote`, `settings.indexing.masterOffNote` / `.overriddenBadge`). Reuses the
settled head terms (index/indexing → `chỉ mục`/`lập chỉ mục`, indexed → `được lập chỉ mục`, drive → `ổ đĩa`, Settings →
`Cài đặt`, off → `đang tắt`). Settled here:

- **"no drive is indexed" (a flat present state, not an unfulfilled expectation):
  `không có ổ đĩa nào được lập chỉ mục`** · use `không` (not), never `chưa` (not yet), when the English states what IS
  the case right now; `chưa` belongs only where the sentence really means "not yet" (`refusedIndexingOff` keeps
  `vẫn chưa được lập chỉ mục` for "stays unindexed", where the drive is expected to get indexed once the switch is back
  on). The existential `có` is required: the catalog's own passive-negative shape is `không có + N + nào + được + V`
  (`operationLog` "không có mục nào được ghi lại", plus six more `không có … nào` lines), and `hiện không` is the
  settled "right now" adverb pair (`hiện không khả dụng`, `hiện không có quyền truy cập toàn bộ đĩa`). GNOME Nautilus
  attests the bare variant ("Nếu không thư mục nào được chọn"), so both parse, but the catalog's `có` form is the one
  this app ships. `high` (catalog-consistent).
- **"off with X" (a control the master switch overrides): `Tắt theo X`** · `theo` in its follows-another-setting sense,
  as in the catalog's `settings.theme.mode.description` ("hoặc theo hệ thống" = or follow the system). Badge
  `settings.indexing.overriddenBadge` → `Tắt theo lập chỉ mục ổ đĩa` (25 chars, same glance weight as the English).
  `high`.
- **"turn this back on" (referring to a settings toggle, not a file): `bật lại mục này`** · the catalog already renders
  "if you turn this off" as `nếu tắt mục này` (`settings.indexing.staleNotify.description`), so `mục này` is the
  established stand-in for the setting itself. `high` (catalog-consistent).
- **folder sizes in the indexing pane: `kích thước thư mục`** (kept) · `settings.json` uses `kích thước` throughout (16
  hits, including the master toggle's own description `để có ngay kích thước thư mục`), while `fileExplorer.json` uses
  `kích cỡ` (10 hits, per the 2026-07-21 decision). `masterOffNote` sits in the settings pane, so it matches its own
  file. The cross-file split is pre-existing; unifying it is a full-catalog migration, not a one-key edit.
- No `sameAsSourceJustification` needed: all five values differ from English.

## Chỉ mục ổ đĩa: lượt kiểm tra thay đổi (2026-07-28)

- **"Checking for changes" (run-kind header) → `Kiểm tra thay đổi`** · verb-phrase shape matching the sibling headers
  (`Quét toàn bộ lần đầu`, `Cập nhật nhanh`); `Kiểm tra` is macOS VI's checking verb (Finder BN9 "Kiểm tra nội dung
  của…"), `thay đổi` is catalog-settled (`các thay đổi gần đây`) and glossary-settled as the MS term · high.
- **"Update the file list" → `Cập nhật danh sách tệp`** · composed from the settled siblings `Lưu danh sách tệp` +
  `Cập nhật chỉ mục` · high.
- **"the check running right now" → `lần kiểm tra đang chạy ngay bây giờ`** · reuses `lần kiểm tra` as this catalog's
  settled phrase for a full check (`tooltipCoalesced`: "lần kiểm tra toàn bộ tiếp theo của Cmdr") and that string's
  closing `sẽ chỉnh lại cho đúng` · high.

## Lần truyền bị đứng yên: thông báo trên hộp thoại + hàng đợi (2026-07-31)

The eight stalled-transfer strings (`fileOperations.transferProgress.stall*` + `close`, `queue.row.stalled`). Mined
2026-07-31 against `_ignored/i18n/vi/` (macOS Finder/AppKit Tier 1, MS terminology Tier 2, GNOME Nautilus + Total
Commander Tier 3). Reuses settled terms (close → `đóng`, cancel → `hủy`, destination/source → `đích`/`nguồn`, log →
`nhật ký`, transfer (countable) → `lần truyền`, background → `chạy ở chế độ nền`, file → `tệp`).

- **progress (advancement, in "no progress"): `tiến triển`** · shared-root pick (mining gotcha 4): macOS renders the
  progress noun as `tiến trình` (Finder SD24 "Hiển thị tiến trình sao chép", PW60 "Hiển thị cửa sổ tiến trình") and MS
  terminology as `Tiến độ` (12×). Neither fits a negated "no progress": `tiến trình` is this glossary's word for an OS
  **process**, so `không có tiến trình` misreads, and `không có tiến độ` is unidiomatic (a rate, not a countable). Same
  `tiến` root, most natural negated form. Progress-the-bar/status stays `tiến trình` (catalog:
  `Tiến trình theo kích thước`). `tentative`.
- **respond: `phản hồi`** · macOS AppKit `AppKitErrors` ("…vì ứng dụng không phản hồi yêu cầu dịch vụ"). NOT MS's
  `hồi đáp` (macOS wins ties). "Waiting for X to respond" → `Đang chờ X phản hồi`. `high`.
- **"Waiting for…": `Đang chờ…`** · macOS Finder (ME23/MR13 `Đang chờ…`, NE88.4 `Đang chờ tải lên`, NE88.5
  `Đang chờ tải về`). Finder also has `Đang đợi` (BU54, AppKit SavePanel "Đang đợi ổ đĩa…"), and Total Commander uses it
  too (`1216` = "Đang đợi máy chủ…"), but `Đang chờ` is the dominant Finder form and already the catalog's queued status
  (`queue.row.status` → `Đang chờ`). `high`.
- **destination / source as BARE nouns: `đích` / `nguồn`** · the standalone (non-attributive) use is attested in the
  orthodox pair: Total Commander vi `1224` = "Nguồn và đích khác nhau!", `5328` = "Nguồn+Đích trên cùng ổ đĩa:". GNOME
  covers the attributive forms (`thư mục đích`, `thư mục nguồn`). So `Đang chờ đích phản hồi.` needs no added classifier
  noun. `high`.
- **"has stopped moving" (a transfer that is still running but not advancing): `đang đứng yên`** · no source names this
  state; `đứng yên` (motionless) is plain everyday Vietnamese and keeps the honest distinction the English draws: the
  transfer has NOT stopped (`đã dừng`) and has NOT hung (`treo`, which reads as a crash and would break the no-"lỗi"
  voice), it just isn't advancing. `tentative`.
- **"still open" (a file whose handle is open): `vẫn đang mở`** · `mở` is the settled open verb (macOS AppKit "Mở").
  macOS's nearest state phrase is `đang được sử dụng` ("in use", Finder PE7/NE66), which names a different concept
  (something else holds the file) — mining gotcha 2, so it isn't adopted. `high` (on `mở`); `tentative` (on the phrase).
- **"partly written": `đã được ghi một phần`** · `ghi` is the settled write verb (`ghi đè` = overwrite, macOS Finder).
  The `được` passive is natural here and keeps the file (not Cmdr) as the subject. `high`.
- **"The log has the details.": `Chi tiết có trong nhật ký.`** · `nhật ký` (log, settled) + GNOME Nautilus's
  `Chi tiết: ` / macOS `Hiện chi tiết`. Fronting `Chi tiết` keeps it short and puts the useful noun first. `high`.
- No `sameAsSourceJustification` needed: all eight values differ from English.

Phrasings settled (keep consistent): "No progress for {duration}" → `Không có tiến triển trong {duration}` (with the
period on the dialog line, without it on the queue row, matching English); "Cancel it, or leave it running in the
background." → `Hãy hủy, hoặc để nó tiếp tục chạy ở chế độ nền.` (`tiếp tục chạy ở chế độ nền` composed from the
catalog's `Giữ chạy ở chế độ nền` + `Vẫn đang chạy ở chế độ nền`).

## Đường dẫn đã sao chép: xác nhận bảng nhớ tạm (`fileExplorer.clipboard.copiedPath`, 2026-08-05)

Một khóa: dòng thông báo thông tin sau ⌃⌘C. Đường dẫn hiện ngay bên dưới trên một dòng riêng với phông chữ đơn cách, nên
nó KHÔNG phải chỗ giữ chỗ trong câu: câu kết thúc bằng dấu hai chấm và phải đứng vững khi thiếu đường dẫn.

- **"Copied the path, it's now on your clipboard:" → `Đã sao chép đường dẫn vào bảng nhớ tạm:`** · dùng lại
  `clipboard → bảng nhớ tạm` và `path → đường dẫn` đã chốt trong glossary (macOS AppKit) · high. Mở đầu bằng `Đã` khớp
  các thông báo anh em (`Đã sao chép {countText} mục`). Gộp "it's now on your clipboard" vào cụm `vào bảng nhớ tạm`:
  dịch sát bằng đại từ `nó` sẽ lủng củng, và tiếng Việt không dùng sở hữu cho một bảng nhớ tạm duy nhất.
- Không cần `sameAsSourceJustification`: giá trị khác tiếng Anh.

## Hàng đợi thao tác: đổi tên từ "transfer queue" sang "operation queue" (2026-08-08)

The queue window widened from "Transfer queue" to **"Operation queue"** in English. This is a MEANING change, not a copy
tweak: the window lists deletes, trashes, renames, folder and file creations, and archive edits, not only transfers, and
"transfer" already means copy-or-move one level down in Cmdr (the transfer progress dialog, the transfer driver). So the
English moved from the narrow word to the CATEGORY word, and `vi` widens the same way. The rename also makes a
deliberate View-menu pair: **Hàng đợi thao tác** (running now) beside **Nhật ký thao tác** (already ran). Fourteen keys
re-translated across `queue.json`, `commands.json`, and `fileOperations.json`.

- **operation (the category word for a copy, move, delete, trash, rename, folder/file creation, or archive edit):
  `thao tác`** (CONFIRMS and re-uses the `operationLog` pass's term) · macOS Finder Tier 1 uses `thao tác` for exactly
  this concept, densely: `thao tác sao chép ^0 mục`, `thao tác di chuyển “^1”`, `thao tác đổi tên`, `thao tác xóa`,
  `thao tác này` (7×), `thao tác đã hoàn thành`, `thao tác chưa hoàn tất`. The vi catalog already ships it 39× and
  already named the Operation log `Nhật ký thao tác`, so the queue takes the SAME head noun (no two words for one
  concept in neighbouring menu items). NOT MS's first "operation" hit `phép toán` (the arithmetic sense) nor
  `phẫu thuật` (surgery) — both wrong senses, mining gotcha 2; MS does attest `thao tác` in compounds
  (`thao tác ghi gom` = gather-write operation, `thao tác WSDL`). `high`.
- **operation queue (the window, the View menu item, and the command-palette entry): `Hàng đợi thao tác`** · `hàng đợi`
  (queue, MS terminology `queue` Noun → `hàng đợi`, corroborated by GNOME Nautilus "Job queued" → "Công việc đã trong
  hàng đợi") + `thao tác`. The `hàng đợi + <modifier>` compound is MS's own shape for this family (`hàng đợi công việc`
  = work queue, `hàng đợi đích` = target queue, `hàng đợi cuộc gọi` = call queue), so the term is built the way
  Vietnamese already builds queue names rather than calqued. The three surfaces stay byte-identical per the en `@key`:
  `queue.windowTitle`, `commands.queueShow.label`, and the "operation queue" mention inside every `fileOperations`
  string. `high`.
- **"Operations" (bare plural heading, `queue.heading` + `queue.list.aria`): `Các thao tác`** · Vietnamese has no plural
  morphology, so the plural has to be carried by a marker or dropped. `Các` (the definite plural marker, "the set of")
  is the right one here because the heading names the specific set listed below it, not operations in general (`những`
  would read indefinite). Three reasons over a bare `Thao tác`: (1) the catalog's own `queue.empty.body` in this very
  window already opens `Các thao tác sao chép, di chuyển, và xóa…`, so the heading and the empty state now match; (2) a
  bare `Thao tác` collides with the transfer dialog's "Action" control label, which is exactly `Thao tác`
  (`transferDialog.operationAria`), and a heading that reads "Action" over a list is wrong; (3) macOS attests `Các` +
  noun freely as a set marker in labels (`Các mục`, `Các thay đổi`, `Các cột`, `Các tab`). `high`.
- **"this operation" (the four per-row screen-reader labels): `thao tác này`** · macOS Finder ships this exact phrase 7×
  (`thao tác này`), so the row labels are Tier-1 verbatim: `Tạm dừng thao tác này`, `Tiếp tục thao tác này`,
  `Hủy thao tác này`, `Chọn thao tác này`. `high`.
- **Keep `lần truyền` for the NARROW sense.** The rename does not retire it: `fileOperations.transferProgress.pauseAria`
  / `.resumeAria` sit on the copy/move progress dialog, where the thing really is a transfer, and
  `settings.network.smbConcurrency.description` and the stalled-transfer strings mean copy-or-move too. Two words is
  correct here because English draws the same distinction. The test is the surface: the QUEUE (which lists every kind of
  job) says `thao tác`; the TRANSFER dialog (which only ever runs a copy or a move) says `lần truyền`.
- **The queued toast carries `thao tác` three times and that's fine.**
  `{countText} đang ở phía trước, nên thao tác này phải chờ đến lượt. Tìm nó trong hàng đợi thao tác.` reads as count +
  subject + window name, and Vietnamese repeats a head noun far more comfortably than English does. Don't "fix" it by
  pronominalizing the middle one: the new operation is FIRST mentioned there, so a bare `nó` would bind to the jobs
  ahead of it and invert the sentence's meaning.
- `queue.empty.title` (`Không có gì trong hàng đợi`) needed no change: it names the queue generically, and its English
  didn't move.
- No `sameAsSourceJustification` needed: all fourteen values differ from English.

## Huy hiệu tiến trình ở góc + thông báo chưa hoàn tất (2026-08-08)

Nine new keys in `queue.json` for two new surfaces: the main window's corner progress chip (`queue.chip.*`) and the
failure notice plus its per-row / toolbar Dismiss buttons (`queue.failureToast.*`, `queue.row.dismiss*`,
`queue.toolbar.dismissAll`). The head noun and the window name come from the rename section directly above; nothing here
re-derives them.

- **dismiss (a failed row, and the toolbar's "Dismiss all"): `Bỏ qua`** · the catalog's OWN settled dismiss word,
  shipped in six places already (`crashReporter.dialog.dismiss`, `downloads.empty.dismiss`, `downloads.fda.dismiss`,
  `errorReporter.sentToast.dismiss`, `errorReporter.bundleSavedToast.dismiss`, `fileOperations.mkdir.timeoutDismiss`,
  `lowDiskSpace.toast.closeTooltip`), so the seventh matches rather than forking. Deliberately NOT `Xóa` (delete) or
  `Gỡ` (remove, the catalog's word for `askCmdr.attachment.remove` / `settings.mediaIndex.chosenFolders.remove`): the
  button removes a ROW, undoes nothing, and a queue row for a delete operation wearing a `Xóa` button would read as
  "delete it again". `high` (catalog-consistent).
- **`Bỏ qua` also renders Skip (`fileOperations.transferProgress.conflictSkip`), and that's accepted.** The two never
  share a surface (the conflict step is a dialog inside the transfer flow; Dismiss lives on a queue row and the queue
  toolbar), and both senses are the same everyday "pass this over" verb in Vietnamese. Don't split them.
- **"Dismiss this operation" (row SR label): `Bỏ qua thao tác này`** · verb + macOS Finder's Tier-1 `thao tác này` (7×),
  the exact shape the other three row labels already use (`Tạm dừng thao tác này`, `Hủy thao tác này`,
  `Chọn thao tác này`). `high`.
- **"Dismiss all" (toolbar): `Bỏ qua tất cả`** · parallel to the toolbar's settled `Tạm dừng tất cả` / `Tiếp tục tất cả`
  (verb + `tất cả`). `high`.
- **"Couldn't finish <action>" (the nine `failureToast.title` arms): `Chưa hoàn tất được thao tác <verb>`** · keeps the
  catalog's settled failed wording `Chưa hoàn tất được` (`queue.row.status`) verbatim as the head, then names the
  operation with the settled head noun. macOS Finder attests both halves densely: `Không thể hoàn tất thao tác này.`,
  `Cần xác thực để hoàn tất thao tác này.`, and the `thao tác + verb` compounds `thao tác sao chép ^0 mục`,
  `thao tác di chuyển “^1”`, `thao tác cắt`, `thao tác dán`. So the toast and the row now say the same thing. `high`.
- **`thao tác` is load-bearing here, not filler: dropping it flips the sentence to a passive.** Finder also attests the
  nominalizer-free `hoàn tất sao chép` ("Bạn có thể hoàn tất sao chép bây giờ"), which tempts a shorter
  `Chưa hoàn tất được sao chép`. Don't: with the short verbs, `được` + bare verb is the standard PASSIVE (`được xóa` =
  "was deleted", `được đổi tên` = "was renamed"), so `Chưa hoàn tất được xóa` reads "hasn't finished being deleted"
  instead of "couldn't finish deleting". A noun after `được` forces the potential reading. This is the vi elided-word
  trap in `docs/i18n/translation-learnings.md` in a new costume: the short version is fluent and wrong.
- **The `other` arm is `Chưa hoàn tất được thao tác`, not the bare `Chưa hoàn tất được`.** English degrades to a bare
  "Couldn't finish" there, which works as an English headline; in Vietnamese the bare form is a status LABEL (it earns
  its keep in the row's status cell, where a bare state is expected) and reads as a fragment missing its object when it
  headlines a notice. The generic head noun completes it and keeps all nine arms parallel. `high`.
- **"N operations couldn't finish" (`failureToast.summary`, and the first sentence of `chip.failed`):
  `{countText} thao tác chưa hoàn tất được`** · count + noun with NO plural marker (`Các` is for the bare plural heading
  only; a counted noun takes neither `các` nor any inflection). The settled `chưa hoàn tất được` follows as the
  predicate. Finder's own subject-predicate form is `thao tác chưa hoàn tất` (without `được`), but `được` is kept: it
  carries the "couldn't" (inability) that English says and plain `chưa hoàn tất` ("didn't finish") drops. `high`.
- **"Open the operation queue to see why.": `Mở hàng đợi thao tác để xem lý do.`** · `mở` (settled open verb, macOS
  AppKit) + the window name lowercased mid-sentence, same as the rename pass's `Tìm nó trong hàng đợi thao tác.` `high`.
- **"Show in operation queue" (the toast's button): `Hiện trong hàng đợi thao tác`** · `Hiện trong X` is the catalog's
  settled "Show in X" shape (`commands.fileShowInFinder.mac.label` and `errorReporter.bundleSavedToast.reveal`, both
  `Hiện trong Finder`). `high`.
- **"percent" spelled as a word for screen readers: `phần trăm`** · MS terminology (`phần trăm`,
  `phần trăm hoàn thành`). `{percentText} phần trăm` puts the number first, as Vietnamese does. Used ONLY in
  `chip.ariaLabel`; the visible tooltip keeps the `%` sign. `high`.
- **The `%` sign takes NO space before it in vi.** Unlike de/fr/sv. The catalog already ships `Phóng to 100%`,
  `({percent}%) đã chọn trong`, `({percentText}%)`, and `indexing.progress.percentEta` is justified as identical to
  English on exactly this ground. So `{percentText}%` stays glued in `chip.tooltip`.
- **items (files and folders alike, in the chip tooltip): `mục`** · macOS Finder (`^0 mục`, `Các mục`), already the
  catalog's word (`Đã sao chép {countText} mục`). No classifier and no plural marker with a count. `high`.
- **"to {destination}": `vào {destination}`** · `vào` (into) is the catalog's preposition for a destination folder
  (`Dán các tệp … vào thư mục hiện tại`, `sao chép "{name}" vào thư mục con của chính nó`) and macOS's
  (`Di chuyển các mục vào Thùng rác`). NOT `sang`, which this catalog reserves for switching/converting
  (`Chuyển sang dạng xem Rút gọn`, `đổi đuôi từ ".{oldExt}" sang ".{newExt}"`). `high`.
- **`chip.tooltip` plural: write `=0 {}` plus `other`, and no `one`.** vi's CLDR set is `other` only; the explicit `=0`
  arm is an exact-value match, which ICU allows alongside `other` regardless of the language's categories, and it's what
  makes the item-count clause vanish before the first progress arrives. Every optional clause keeps its own LEADING
  space (` {countText} mục`, ` vào {destination}`, ` · {detail}`) so the four combinations never produce a double space
  or a dangling `·`. Verified by formatting all four.
- **The chip word itself (`huy hiệu`) appears in no value.** It's only the surface's name; recorded here because the
  Settings labels that name Cmdr's other chips already use it (`Hiện huy hiệu kho`,
  `Hiện huy hiệu trạng thái trên tệp hình ảnh`), so a future string that has to SAY "chip" should say `huy hiệu`.
  `tentative`.
- ETA / time-left inside `{detail}` is formatted elsewhere and arrives as the settled `còn {duration}`; these keys pass
  it through untouched.
- No `sameAsSourceJustification` needed: all nine values differ from English.

## Lời nhắc xung đột đứng riêng: dòng ngữ cảnh + ghi chú tạm dừng (2026-08-09)

Two keys for the standalone conflict prompt (`fileOperations.operationConflict.context` / `.pausedNote`), the surface
that asks which operation a name clash belongs to. Both are edits of settled siblings, not fresh translations:
`queue.row.label` gives the verb arms, `queue.chip.tooltip` gives the destination preposition, `queue.row.status` gives
the paused word.

- **The progress sentence keeps `vào` for BOTH copy and move, even though macOS splits them.** Finder's own progress
  lines are `Đang sao chép “thứ gì đó” vào “nơi nào đó”` (AirDrop, Tier 1, exact shape) but
  `Đang di chuyển “^1” đến “^2”` / `Đang di chuyển ^0 mục đến “^2”` for a move. The catalog already settled one
  preposition for a destination folder (`vào`, and `queue.chip.tooltip` renders ` · vào {destination}` for every
  operation kind), so the two arms stay parallel rather than forking on a distinction the rest of the catalog doesn't
  draw. `high` (catalog-consistent). `đến` stays the `transferDialog.targetGroupTitle` heading word (`Đến`).
- **"Working in {destination}" → `Đang xử lý trong {destination}`** · `trong` (in), not `vào` (into): the generic arm
  says work is happening INSIDE the folder, not moving into it. `trong thư mục này` is the catalog's own shape
  (`shared.conflictExistsFile`). `high`.
- **`archive_edit` deliberately says two different things.** With a destination it names the archive itself
  (`Đang chỉnh sửa {destination}` → "Đang chỉnh sửa ảnh.zip"); without one it keeps `queue.row.label`'s generic
  `Đang chỉnh sửa tệp nén`. English draws the same split, and Vietnamese needs no article to carry it. `high`.
- **Subject-drop is right here, verb-drop is not.** Every arm is `Đang` + a real verb, so each reads as a complete
  clause under the title `Tệp đã tồn tại`; the elided subject is what macOS's own progress lines elide too. This is the
  vi trap in `docs/i18n/translation-learnings.md` (an elided word still reads fluent), so re-read each of the eight
  formatted outputs standalone before shipping a change here.
- **"Everything else is paused until you answer." → `Các thao tác khác đã tạm dừng cho đến khi bạn trả lời.`** · NOT a
  literal `Mọi thứ khác`: in Vietnamese that scopes to the whole app and reads as "Cmdr is frozen", which is the one
  thing this reassuring line must not say. What actually stops is the rest of the operation queue, so the line names it
  with the settled head noun `thao tác` plus `Các` (the definite-set plural marker, same as `queue.heading`). The state
  word is `queue.row.status`'s `Đã tạm dừng` verbatim, so the note and the rows the user then opens read alike;
  `tạm dừng cho đến khi …` is the ordinary vi collocation for a pause with an endpoint. `trả lời` (answer) over
  `phản hồi` (respond): the glossary reserves `phản hồi` for a machine responding (macOS AppKit "ứng dụng không phản
  hồi"). `high` (on the parts); `tentative` (on `trả lời` for answering a dialog).

## Nút "Chạy nền": trạng thái hàng đợi trống của nút Hàng đợi (2026-08-09)

Two keys, `fileOperations.transferProgress.background` + `.backgroundAria`: the SAME button as `.queue`/`.queueAria`,
worded for an EMPTY operation queue (nothing to queue behind, so the button names what it does). "Background" is a VERB
in the English, not the backdrop noun.

- **"Background" (the button, empty-queue state): `Chạy nền`** · verb `chạy` + the settled `nền`, the compact everyday
  vi form ("cho phép ứng dụng chạy nền"). Reads as a command, which a bare `Nền` would not: in THIS app `nền` alone is
  already the visual sense (`nền khung` = pane background, macOS `màu nền`/`hình nền`/`Màn hình nền`), so the noun would
  name a backdrop, not an action. Total Commander vi ships exactly this button pair in its `{COMMON}` block —
  `4004="&Nền"` (Background) beside `4005="&Hàng đợi"` (Queue), the orthodox dialog Cmdr mirrors — which settles the
  head noun (`nền`) and confirms the catalog's `Hàng đợi` for the sibling, but its bare `&Nền` is a calque of the
  English button and isn't adopted. TC also attests the concept for running work: `1237` = "%i thao tác đang hoạt động
  trong nền!", `1185`/`1189`/`1190` = "Tải xuống/Tải lên/Xóa trong nền (luồng riêng biệt)". MS terminology agrees on the
  head noun (`background` Noun → `nền`, `background task` → `tác vụ nền`) and on the `chạy + <place>` shape
  (`ứng dụng chạy trong hộp cát`, `SharePoint chạy trên máy chủ`). macOS has NO run-in-the-background string at all
  (every vi `nền` hit in the pile is visual: `màu nền chữ`, `Màn hình nền`, `Đặt Màu nền`), so Tier 1 is silent here and
  Tier 3's orthodox pair carries it. `high`.
- **`chạy nền` is a BUTTON-LENGTH compression of the settled `chạy ở chế độ nền`, not a fork.** Prose keeps the full
  form everywhere it already ships (`queueTooltip` = `Giữ chạy ở chế độ nền và quản lý…`, `backgroundedToast` =
  `Vẫn đang chạy ở chế độ nền.`, `stallUnknown` = `để nó tiếp tục chạy ở chế độ nền`); only the button label and its
  aria use `chạy nền`. The full form is 17 characters against English's 10 on a button that must also fit `Hàng đợi`
  (8), and vi already runs ~20-25% long; the compressed form lands at 8, exactly the sibling's width. Rule for future
  keys: a running-in-the-background SENTENCE says `chạy ở chế độ nền`; a CONTROL says `chạy nền`.
- **"Keep this running in the background" (screen-reader name): `Giữ chạy nền`** · the `queueTooltip`'s shipped
  rendering of the same English clause (`Giữ chạy ở chế độ nền`), with the same subject elision, compressed to match the
  label. Eliding the object matches its direct partner `queueAria` (`Đưa vào hàng đợi thao tác`, also object-less);
  naming one would force a `lần truyền`-vs-`thao tác` call the English deliberately dodges with "this". `Giữ` is the
  catalog's and macOS's keep-verb (`Giữ thư mục ở trên cùng`, `Giữ cả hai`, `Giữ lại bản gốc`). `high`.
- **WCAG 2.5.3 (Label in Name) drove the aria's shape.** The visible label must appear inside the accessible name, so a
  voice-control user saying "bấm Chạy nền" is understood. `Giữ chạy nền` contains `chạy nền` — same case-insensitive
  containment English ships (`Background` ⊂ `…in the background`), the `G`/`C` difference being Vietnamese sentence case
  on the label's first letter. Any future rewording of either key must preserve the containment: the aria is not free to
  drop the label's words.
- No `sameAsSourceJustification` needed: both values differ from English.

## Hộp thoại thoát khi thao tác đang chạy (`main.quit.*`, 2026-08-10)

Seven keys for the modal Cmdr raises when the user quits while a copy, move, delete, trash, or archive edit is still
running: a title, a reassuring body, the running-operations heading, a live countdown plus its aria label, and the two
buttons. Reuses the settled head terms (operation → `thao tác`, item → `mục`, file → `tệp`, quit → `thoát`, restart →
`khởi động lại`, running → `đang chạy`, wait → `chờ`). New or newly-sourced below:

- **"while X is running": `trong khi X đang chạy`** · macOS Finder Tier 1 ships the exact structural parallel (`N144` =
  "You can't open “^0” while the Finder is running." → `Bạn không thể mở “^0” trong khi Finder đang chạy.`; `RN26` =
  "…while it's open?" → `…trong khi đang mở không?`). So `main.quit.title` is
  `Thoát trong khi {countText} thao tác đang chạy?`, the Finder shape with Cmdr's settled head noun. `high`.
- **The title is ONE `other` branch, and the count always shows.** vi has a single CLDR category, so English's
  `one {an operation}` / `other {{countText} operations}` split collapses; the counted noun takes no marker and no
  inflection (`1 thao tác`, `12 thao tác`), matching `queue.failureToast.summary`'s shipped
  `{countText} thao tác chưa hoàn tất được`. No `=1` arm: the style guide forbids re-introducing an English-shaped
  singular/plural split where the noun doesn't change.
- **"Quit now" (primary, destructive button): `Thoát ngay`** · `thoát` (quit, macOS AppKit `Quit` → `Thoát`; MS
  terminology `quit` Noun → `thoát`) + `ngay` (right now). `ngay` is the pile's immediacy adverb (macOS
  `xóa ngay lập tức`; MS `Họp ngay` = Meet now, `Quay lại ngay` = Back now), and it carries the load-bearing "now" the
  en `@key` flags: the app quits either way, this button skips the wait. NOT macOS's `Vẫn Thoát` ("Quit Anyway",
  `AppKit/Document.json`), which answers a different question (overriding an objection, not skipping a timer). `high`.
- **"Keep working" (the button that calls the quit off): `Tiếp tục làm việc`** · `tiếp tục` + verb is densely attested
  in the pile as "carry on doing X" (macOS `tiếp tục sao chép`, `tiếp tục chạy`, `tiếp tục duyệt`, `tiếp tục xem`), and
  `làm việc` is MS's work verb (`giờ làm việc`, `làm việc từ xa`). Deliberately NOT a bare `Hủy` (cancel): on a dialog
  that lists running operations, `Hủy` would read as cancelling THEM, the exact opposite of what the button does. Also
  NOT `Để sau` (the catalog's "Not now", `askCmdr.consent`) nor anything built on `sau` / `nhắc lại`: the countdown is
  deleted, not deferred, and the en `@key` forbids a postpone reading. The object `làm việc` is what keeps `Tiếp tục`
  from colliding with `queue.row.resume`'s bare `Tiếp tục` (Resume) — different surface, and the operations here are
  running, not paused. `high` (on the parts); `tentative` (on the whole label reading unambiguously as "you keep
  working" to a native ear).
- **"Still running" (heading over the operation rows): `Vẫn đang chạy`** · `Đang chạy` is `queue.row.status`'s Running
  verbatim, and `Vẫn đang chạy` already ships as the head of `fileOperations.transferProgress.backgroundedToast`
  (`Vẫn đang chạy ở chế độ nền.`). The heading and the rows below it now use the same words. `high`
  (catalog-consistent).
- **"half-written file": `tệp ghi dở`** (kept) · the shipped vi catalog already renders this exact English phrase in
  `settings.advanced.showStagingTempFiles.description` ("a crash can't leave a half-written file under a real name" →
  `sự cố không thể để lại tệp ghi dở dưới một tên thật`), so the dialog matches it rather than coining a second form.
  The glossary's `đã được ghi một phần` (`stallInFlight`) stays for the predicative "may already be partly written";
  `ghi dở` is the attributive one. `high` (catalog-consistent).
- **"clears away" (removing the leftover partial file): `dọn dẹp`** · MS terminology's cleanup verb (`dọn dẹp nhanh`,
  `dọn dẹp phân phối`, `dọn dẹp bản ghi ghost`). Deliberately NOT `xóa` (delete): `xóa` is Cmdr's delete verb, and a
  reassurance inside a quit dialog must not read as "Cmdr deletes your file". Also NOT macOS's `Dọn sạch` (`Clean Up` →
  `Dọn sạch`, and Finder's `Dọn sạch Thùng rác` = Empty Trash), which names emptying a container, not tidying one
  leftover away. `high` (on `dọn dẹp`); `tentative` (on choosing it over `xóa` for this register).
- **"it leaves behind" → `còn sót lại`, with the pronoun dropped** · `Cmdr dọn dẹp tệp ghi dở còn sót lại.` A literal
  `mà nó để lại` would put `nó` in a clause whose subject is already `Cmdr`, reading as Cmdr cleaning up after itself
  twice. `còn sót lại` (left over) is the catalog's own word for exactly this artifact
  (`showStagingTempFiles.description`: `Các tệp còn sót lại từ lần sao chép bị gián đoạn`). `high` (catalog-consistent).
- **"Anything still being written": `Những gì đang được ghi`** · **the body must stay number-neutral**: one operation
  writes several files at once and several operations can run at once, so `Mục duy nhất đang được ghi` states something
  false. Vietnamese nouns carry no number, so dropping `duy nhất` was the whole fix and `tệp ghi dở còn sót lại` was
  already neutral. `đang được ghi` is the same `được` + write-verb passive the glossary settled in
  `đã được ghi một phần`. ⚠️ **Never open this clause with `Chỉ mục …`**: `chỉ mục` is this glossary's word for an
  INDEX, so `Chỉ mục đang được ghi` would read "the index being written". `high`.
- **"Whatever's finished stays done": `Những gì đã xong vẫn được giữ nguyên.`** · `Xong` is `queue.row.status`'s Done,
  and `giữ nguyên` is the catalog's keep-as-is verb (`Đã giữ nguyên tên gốc`, `Cmdr giữ nguyên tên hiện tại`). NOT
  `Mọi thứ`, which scopes to the whole app (the trap the 2026-08-09 conflict-prompt pass recorded for "Everything else
  is paused"). `high`.
- **"Quitting in {secondsText} seconds": `Sẽ thoát sau {secondsText} giây`** · subject-drop under a sentence-initial
  `Sẽ` is the catalog's own shape for a future statement about Cmdr (`fileExplorer.smbReconnect.willKeepTrying` =
  `Sẽ tiếp tục thử trong tổng cộng {duration}.`); `sau N giây` is macOS-attested (`sau 15 giây`, `sau 30 giây`) and
  `giây` is the settled seconds noun. One `other` branch, both placeholders kept. `high`.
- **"a restart or logout" gets an explicit `máy`: `việc khởi động lại máy hoặc đăng xuất`** · the en `@key` states these
  are the OPERATING SYSTEM's, not Cmdr's, and vi needs the disambiguation English gets free from context: this catalog
  uses bare `khởi động lại` for restarting the APP in several places (`settings.control.restartRequired`,
  `ai.local.statusRestarting`, `onboarding.stepFda.postAction.intro`), so a bare one here, one clause after `Sẽ thoát`,
  would read "Cmdr will restart". `máy` (the user's machine) is the catalog's word for it
  (`Không dữ liệu nào rời khỏi máy của bạn`). Logout → `đăng xuất` · MS terminology (`log off` / `sign out` →
  `đăng xuất`) and the catalog's `shortcuts.system.loggingOut`. `việc` nominalizes both so they can be the subject of
  `phải chờ`. `high`.
- **"never waits on Cmdr": `không bao giờ phải chờ Cmdr`** · `chờ` is the settled wait verb (`Đang chờ`,
  `Đang chờ đích phản hồi`), `không bao giờ` the catalog's "never" (`Cmdr không bao giờ gửi chính các tệp`). `phải` (has
  to) is what keeps it from reading as a choice. `high`.
- **`countdownAria`: `Thời gian còn lại trước khi Cmdr tự thoát`** · `trước khi thoát` is macOS AppKit Tier-1 verbatim
  ("before quitting" → `trước khi thoát`, the three unsaved-documents alerts), and `tự` + verb is the catalog's "on its
  own" (`thường tự hết trong vài giây`, `Tự lặng lẽ tiến lên mỗi lần khởi chạy`). `còn lại` names what the number
  actually measures (time remaining), which the visible countdown doesn't spell out. This key has NO visible label to
  contain, so WCAG 2.5.3 doesn't bind it; it may be reworded independently of `main.quit.countdown`. `high`.
- No `sameAsSourceJustification` needed: all seven values differ from English.

## Usage stats: bỏ "ẩn danh", nêu rõ "một mã định danh ngẫu nhiên" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, 2026-08-12)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `bút danh` / `giả danh` — that jargon
is exactly what the copy avoids.

- **usage stats → `thống kê sử dụng`** · the settings label's existing term; only the `ẩn danh` adjective was cut ·
  high. The five keys touched here are now uniform on it. ⚠️ `onboarding.stepBeta.emailNote` (untouched, its English
  didn't change) still says `số liệu sử dụng` — fold it in on the next pass over that key.
- **a random id → `một mã định danh ngẫu nhiên`** · MS terminology (random → `ngẫu nhiên`, identifier → `mã định danh`)
  · high. ❌ Not a bare `mã ngẫu nhiên`: `mã` alone reads as a code (a coupon, a PIN), which loses the identifier sense.
- **tied to → `gắn với`** · plain everyday Vietnamese for the relation; `liên kết với` (used in
  `onboarding.stepBeta.emailNote`) is the heavier, more technical register and is kept for the linking sense · high
- No `sameAsSourceJustification` needed: every value differs from English.

## Câu hỏi làm dừng một hàng trong hàng đợi + hộp thoại hoàn tác (2026-08-13; `queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, bốn khóa `fileOperations.rollbackConfirm.*`, và hai khóa viết lại `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- **"Needs your answer" (queue-row status) → `Cần bạn trả lời`** · ⚠️ NOT anything opening on `Đang chờ`: that is the
  queued status in the same narrow column. `trả lời` is the catalog's own answering verb
  (`fileOperations.operationConflict.pausedNote` "cho đến khi bạn trả lời") and macOS AppKit renders Reply as `trả lời`
  · high
- **the prompt (the on-screen question) → `câu hỏi`** · the conflict prompt IS a question; `lời nhắc` reads as a
  reminder · high. Main window stays `cửa sổ chính` (`queue.row.foregroundAria`, `search.action.showAll.label`).
- **"carries on" → `sẽ tiếp tục`** · `tiếp tục` is the settled resume/continue verb (macOS Finder "Tiếp tục") · high
- **"Keep them" (the safe button) → `Giữ lại`** · macOS AppKit "Keep Selected" → `Giữ lại Tệp đã chọn`, "Keep Both
  Files" → `Giữ lại Cả hai Tệp`; sentence-cased per style.md · high
- **"Roll back" / "Roll this operation back?" → `Hoàn tác` / `Hoàn tác thao tác này?`** · the settled `hoàn tác`
  rollback family (matches `transferProgress.conflictRollback` and the `operationLog.rollback.*` chips) · high
- **"Stop" in the rollback tooltip → `Dừng lại`** · macOS AppKit `NSStopProgressTemplate` → `dừng tiến trình`. ❌ Never
  `Hủy` here: that IS Cancel, which keeps the finished files · high
- **"so far" → `đến giờ`** · the catalog's own phrase (`search` result counter "# kết quả đến giờ") · high
- **the files an operation overwrote → `những tệp bị ghi đè`** · the settled `ghi đè` (overwrite) · high
- `foregroundBusyToast` no longer claims another operation holds the window ("Ở đây đang mở một thứ khác"): the blocker
  can be any dialog. "bring this one up" → `hiện thao tác này lên`, tying to the row's `Hiện` (Show) button · high
- No `sameAsSourceJustification` needed: all eight values differ from English.

## Đổi tên liên tiếp: thông báo gộp khi nhiều tệp giữ nguyên tên (2026-08-18; `fileExplorer.rename.chainKeptOriginalNameAndOthers`)

The growing sibling of `fileExplorer.rename.chainKeptOriginalName` (`{reason}. "{name}" vẫn giữ nguyên tên.`). One
toast, rewritten each time another file in the arrow-key rename run keeps its name, so the two must read as one voice:
same `vẫn giữ nguyên tên` predicate, same straight ASCII quotes around `{name}` (the sibling already ships them; ❌
don't switch this pair to the style guide's curly `“…”` alone — that's a both-keys migration).

- **"and N other files" → `và {othersText} tệp khác`** · macOS Finder Tier 1 has the exact name-plus-count shape:
  `Đang gửi “^1” và ^0 mục khác.` ("Sending "X" and N other items."), `và ^0 mục khác.`, and
  `bảo lưu tất cả các mục mới hơn như “^1” và ^0 mục khác`. GNOME Nautilus agrees on the bare numeral + noun + `khác`
  (`Đã chọn %'d mục khác`), Xfce Thunar on `Các tệp khác`. ⚠️ No `các` before a numeral: the number already carries the
  count · high
- **"and so did …" → `… cũng vậy`** (sentence-final pro-predicate) · everyday standard Vietnamese for "likewise", and
  the only way to keep English's two-clause scoping: `{reason}` describes ONE file, so merging into
  `"{name}" và N tệp khác vẫn giữ nguyên tên` would silently spread the reason across all of them. `cũng` as "also" is
  densely attested in the pile (macOS `tài liệu cũng sẽ được mở khóa`, `Chúng cũng sẽ bị xóa`); the `vậy` pro-predicate
  is not, hence `tentative`. Fully explicit fallback if a native reader finds it thin:
  `và {othersText} tệp khác cũng giữ nguyên tên` · tentative
- **Plural: one `other` arm only** (`{others, plural, other {{othersText} tệp khác}}`), per style.md's single-category
  rule. `{others}` is kept solely to drive the selection; ❌ never add an `=1`/`one` arm reproducing English's split —
  `tệp` doesn't inflect. The framing words (`và`, `cũng vậy`) sit OUTSIDE the branch so the arm holds exactly what
  English's arm holds.
- No `sameAsSourceJustification` needed: the value differs from English.

## Đổi tên không xác nhận được + tên không dùng được (2026-08-18; `fileExplorer.rename.unconfirmed`/`.unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

Cặp `unconfirmed*` là anh em của cặp `chainKeptOriginalName*` (cùng dạng thông báo nhỏ), nhưng NGHĨA khác hẳn:
`chainKept*` khẳng định tệp vẫn giữ nguyên tên, còn `unconfirmed*` nói rằng Cmdr chưa biết, và việc đổi tên vẫn có thể
đã xảy ra. ❌ Đừng bao giờ dùng `vẫn giữ nguyên tên` trong cặp này.

- **"Couldn't confirm …" (thao tác hết thời gian chờ, có thể đã thành công) → `Chưa xác nhận được …`** · chính catalog
  vi đã dùng đúng khung này cho hai chuỗi song song: `fileOperations.mkdir.timeoutMessage`
  (`Chưa xác nhận được thư mục đã được tạo. Ổ đĩa có thể chậm, nên thư mục vẫn có thể đã được tạo.`) và
  `fileExplorer.pane.trashUnconfirmedToast`. `Chưa … được` (chưa làm được, còn để ngỏ) hợp hơn `Không thể` cho tình
  huống "chưa biết", và tránh hẳn `lỗi`/`thất bại` theo giọng lỗi trong style.md · high (nhất quán catalog)
- **"the rename of X" (danh từ hóa) → `việc đổi tên "{name}"`** · `việc` + động từ là cách danh từ hóa chuẩn; GNOME
  Nautilus dùng `đổi tên "%s"` với tên trong ngoặc kép. Dạng này quan trọng vì nó nở ra được:
  `việc đổi tên "{name}" và {othersText} tệp khác` · high
- **"The volume may be slow" → `Ổ đĩa có thể chậm`** · giống hệt vế giữa của `mkdir.timeoutMessage`; `ổ đĩa` là từ đã
  chốt cho drive/volume, và `có thể chậm` giữ đúng giọng dè dặt (app không biết chắc) · high
- **"the rename may still have gone through" → `việc đổi tên vẫn có thể đã hoàn tất`** · `vẫn có thể đã` là đúng cấu
  trúc `mkdir.timeoutMessage` dùng (`thư mục vẫn có thể đã được tạo`), và `hoàn tất` là từ hoàn thành catalog đã dùng
  nhiều (`hoàn tất thao tác`, `trước khi hoàn tất`). Bản tiếng Anh lặp lại "the rename" ở vế sau, bản vi lặp lại
  `việc đổi tên` y hệt · high
- **"the renames" (số nhiều, bản nhiều tệp) → `các lần đổi tên`** · `lần` là lượng từ cho một lượt thao tác, đúng kiểu
  `lần truyền` (transfer) đã chốt trước đó. ❌ Không viết `các việc đổi tên`: `việc` không đếm được kiểu đó · high
- **"and N other files" → `và {othersText} tệp khác`; một nhánh `other` duy nhất** · dùng lại nguyên cách của
  `chainKeptOriginalNameAndOthers` (xem mục 2026-08-18 ở trên), kể cả ngoặc kép ASCII thẳng quanh `{name}` · high
- **"That filename can't be used" → `Không thể dùng tên tệp đó`; nhánh folder → `Không thể dùng tên thư mục đó`** ·
  `không thể` là từ đã chốt cho can't/couldn't, và GNOME Nautilus có đúng cấu trúc `không thể dùng X`. Câu trọn vẹn,
  KHÔNG có dấu chấm cuối (nó được ghép vào câu dài hơn của `chainKeptOriginalName`:
  `Không thể dùng tên tệp đó. "notes.txt" vẫn giữ nguyên tên.`). Cố ý KHÔNG đoán lý do (`không hợp lệ` ám chỉ một quy
  tắc cụ thể), giữ đúng vai trò chuỗi bắt-tất-cả · high
- Không cần `sameAsSourceJustification`: cả ba giá trị đều khác bản tiếng Anh.

## Thao tác được đề xuất: hộp thoại cho những gì Ask Cmdr đề xuất (`suggestedOps.*`, `commands.suggestedOpsShow.*`, 2026-08-19)

- ops (các thao tác tệp do tác nhân đề xuất) → `thao tác`; tiêu đề là `Thao tác được đề xuất` · theo thuật ngữ sẵn có
  ("File operations" → "Thao tác tệp") · high
- approve → `Phê duyệt` · chuẩn; chọn thay cho `Chấp nhận` của macOS, vốn dành cho việc nhận tệp qua AirDrop · high
- reject → `Từ chối` · macOS Finder, cặp Chấp nhận/Từ chối trong bảng AirDrop (Tier 1) · high
- "This can't be undone" → `Bạn không thể hoàn tác việc này` · macOS Finder ("Bạn không thể hoàn tác tác vụ này") · high
- pattern → `mẫu` · đã có trong `queryUi.json` · high

## Nhân bản: lệnh sao chép ngay trong cùng thư mục (`commands.fileDuplicate.*`, 2026-08-19)

- **duplicate (lệnh sao chép mục đã chọn ngay trong thư mục của nó) → `Nhân bản`** · macOS Finder `vi`, menu "Tệp > Nhân
  bản" (`N154`), cùng "Nhân bản các mục" và "Nhân bản các mục trong vị trí hiện tại của chúng" (kiểm chứng trên macOS
  26.6.1, `Finder.app/Contents/Resources/vi.lproj`, 2026-08-19) · high. Không trùng với `Sao chép` (F5) hay `Di chuyển`
  (F6).
- **"Make a copy of the selected files in the same folder" → `Tạo bản sao của các tệp đã chọn trong cùng thư mục`** ·
  theo các mô tả lân cận ("Sao chép các tệp đã chọn…"); `bản sao` là từ đã chốt cho "copy" (danh từ), và "cùng thư mục"
  là thư mục các tệp đang nằm sẵn · high.

## Menu gốc: thanh menu, menu chuột phải, tiêu đề cửa sổ (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`, 2026-08-19)

Nguồn cho cả nhóm này: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/vi.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) là Tier 1 và quyết định gần như mọi thứ; phía tiếng Anh đọc từ `en_GB.lproj`, vì
`Base.lproj` chỉ chứa nib đã biên dịch. Safari 26 (`MainMenu.strings`) cung cấp từ vựng về tab, còn thuật ngữ Microsoft
bù vào chỗ Apple không đặt tên. Họ RAW: **dấu nháy đơn**, một `''` sẽ hiện thành hai dấu trên menu.

- **Thanh menu → `Tệp`, `Sửa`, `Xem`, `Đi`, `Cửa sổ`, `Trợ giúp`, `Dịch vụ`** · macOS Finder và Safari `vi` · high.
- **⚠️ tab (thẻ giao diện): Tier 1 dùng từ mượn `tab`, không phải `thẻ`.** macOS Finder `vi` viết „Tab mới”, „Hiển thị
  Tất cả Tab”, và Safari `vi` viết „Tab mới”, „Đóng tab”, „Ghim tab” (kiểm chứng trên macOS 26.5.2, 2026-08-19). Catalog
  Cmdr hiện dùng `thẻ` ở 36 chỗ trong 6 tệp, nên đợt này giữ `thẻ` cho nhất quán toàn ứng dụng. **Việc cần làm:** một
  đợt riêng nên đổi toàn bộ `thẻ` → `tab` và nâng mục này trong `style.md` từ `tentative` lên `high`. Không tự ý đổi lẻ
  một chỗ.
- **pane → `khung`, vẫn `tentative`** · Total Commander `vi` dùng `bảng` (`WCMD.INC` 104, 531), Microsoft dùng `ngăn`,
  catalog Cmdr dùng `khung`. Ba nguồn, ba từ; giữ `khung` vì catalog đã dùng, và ghi lại hai lựa chọn kia.
- **Quick Look → `Xem nhanh`** · macOS Finder (`TL14`) · high. Apple có dịch tên tính năng này nên nó KHÔNG nằm trong
  danh sách không-dịch.
- **Get Info → `Lấy thông tin`, Enclosing Folder → `Thư mục chứa`, Go > Home → `Nhà`, Sort By → `Sắp xếp theo`,
  Back/Forward → `Trở lại` / `Tiếp theo`, Size → `Kích cỡ`, Default → `Mặc định`, Other… → `Khác…`** · macOS Finder Tier
  1 · high.
- **Minimize → `Thu nhỏ`, Window > Zoom → `Thu phóng`** · macOS Finder (`300666`, `300667`) · high. Hai giá trị này
  trùng với `menu.zoom.out` và `menu.view.zoom`, nhưng chúng nằm ở hai menu khác nhau nên không gây nhầm lẫn, và cả bốn
  đều là từ Tier 1.
- **ascending / descending → `Tăng dần` / `Giảm dần`** · Thunar + Dolphin `vi` · high.
- **changelog → `Nhật ký thay đổi`** · thuật ngữ Microsoft · high. Khác với Trợ giúp > `Có gì mới`: một bên gọi tên tài
  liệu, một bên gọi tên tin tức.
- **word wrap → `Tự ngắt dòng`** · thuật ngữ Microsoft · high.
- **pin / unpin tab → `Ghim thẻ` / `Bỏ ghim thẻ`** · Safari `vi` („Ghim tab”), chuyển sang thuật ngữ `thẻ` của catalog ·
  high.
- **Màu nhãn Finder → `Đỏ, Cam, Vàng, Lục, Lam, Tía, Xám`** · macOS Finder (`TG_COLOR_*`) · high.
- **busy (ổ đĩa đang được dùng) → `(đang bận)`** · thuật ngữ Microsoft (`bận`) · high.
- **Eject → `Tháo`, Disconnect → `Ngắt kết nối`, Remove (khỏi một danh sách) → `Gỡ bỏ`** · macOS Finder và Thunar `vi` ·
  high. `Gỡ bỏ` tránh nghe giống `Xóa` (xóa tệp).
- **Giống hệt tiếng Anh có chủ đích** (`sameAsSourceJustification`): `menu.zoom.percent*` và `menu.view.askCmdr`.

## Thông báo dự phòng khi phải dùng kết nối SMB của macOS (`fileExplorer.network.osMountFallback.*`, 2026-08-21)

Ba khóa: phần thân thông báo, nút thử lại, và tooltip của nút X. Dùng lại từ vựng đã chốt của nhóm
`fileExplorer.network` /`navigation` (kết nối trực tiếp → `kết nối trực tiếp`, share → `mục chia sẻ`, thử lại →
`thử lại`, dismiss → `bỏ qua`). Giọng văn trấn an, không báo lỗi: mục chia sẻ vẫn chạy được, chỉ là chậm.

- **native (kết nối SMB có sẵn của macOS) → `tích hợp sẵn`** · thuật ngữ Microsoft (`built-in toolbar` →
  `thanh công cụ tích hợp sẵn`; MS dịch `native` thành `riêng`, nhưng `riêng của macOS` dễ đọc nhầm thành "chỉ dành cho
  macOS") · high. Cả câu: `kết nối mạng SMB tích hợp sẵn của macOS`. Các thông báo anh em vẫn gọi nó là
  `kết nối hệ thống` khi không cần nêu tên SMB.
- **"4x slower" → `chậm hơn 4 lần`, "(sometimes 100x)" → `(đôi khi là 100 lần)`** · không nguồn nào trong pile có bội
  số; `<số> lần` là cách viết bội số chuẩn của tiếng Việt, và `chậm hơn N lần` rõ hơn `chậm gấp N lần` khi so sánh hai
  bên · tentative. Đặt vế so sánh sau: `chậm hơn 4 lần so với kết nối trực tiếp của Cmdr`.
- **"which is …" (mệnh đề quan hệ giải thích) → `vốn …`** · `vốn` là cách nối tự nhiên cho mệnh đề nêu tính chất sẵn có,
  tránh phải cắt thành hai câu · high (ngữ pháp phổ thông).
- **"Click the button below" → `Hãy bấm nút bên dưới`** · macOS `vi` có cả hai mảnh: `hãy bấm vào nút Thêm (+)`
  (SystemSettings) và `vùng bên dưới` (Finder `993.title`) · high.
- **"Try connecting directly" (nút) → `Thử kết nối trực tiếp`** · ghép `Thử lại` (`fileExplorer.network.retry`) với
  `Kết nối trực tiếp để truy cập nhanh hơn` (`fileExplorer.navigation.connectDirectly`), cắt phần đuôi vì nút nằm ngay
  trong thông báo đã giải thích lợi ích · high.
- **Dismiss (tooltip nút X) → `Bỏ qua`** · dùng lại `lowDiskSpace.toast.closeTooltip`; thuật ngữ Microsoft (`dismiss` →
  `bỏ qua`) · high.

## Lỗi khi đổi tên / tạo mới: 31 khóa `errors.mutation.*` + `errors.volume.*` (2026-08-23)

Một dòng hiện ngay dưới ô nhập tên (hoặc trong thông báo nhỏ) khi việc đổi tên, tạo thư mục, hay tạo tệp bị từ chối. Họ
RAW, không phải ICU: dùng dấu nháy đơn thường, giữ `{path}` nguyên vẹn. `{path}` là phần chèn không kiểm soát được
(đường dẫn bất kỳ, độ dài bất kỳ), nên câu phải đứng vững với mọi giá trị. Dùng lại từ đã chốt (ổ đĩa, tệp/thư mục, tệp
nén, chỉnh sửa, thiết bị, ngắt kết nối, quyền, mật khẩu, đích, thử lại, không thể, chưa hoàn tất được). Mọi giá trị đều
tránh `lỗi`/`thất bại` theo giọng lỗi trong `style.md`. Mới hoặc mới có nguồn:

- **System Integrity Protection → `tính năng Bảo vệ Toàn vẹn Hệ thống`** · macOS Finder Tier 1 dịch nguyên tên tính năng
  này (`LocalizableMerged` `ET6`: "Some items in the Trash cannot be deleted because of System Integrity Protection." →
  "Không thể xóa một số mục trong Thùng rác vì tính năng Bảo vệ Toàn vẹn Hệ thống.", kiểm chứng 2026-08-23). Apple CÓ
  bản địa hóa tên này nên nó không nằm trong danh sách không-dịch (cùng quy tắc với Quick Look). Giữ nguyên cách viết
  hoa của Apple; `macOS` vẫn nguyên văn · high
- **Get Info (bảng thông tin của Finder) → `cửa sổ Lấy thông tin`** · macOS Finder ("Get Info" → "Lấy thông tin"; "Shows
  the Get Info window for an item or items" → "Hiển thị cửa sổ Lấy thông tin cho một hoặc nhiều mục"), khớp luôn với
  `commands.fileGetInfo.mac.label` mà catalog đã ship (`Lấy thông tin`) · high. ⚠️ Ba khóa cũ hơn trong `errors.write.*`
  (`fileLocked.suggestion.mac`, `permissionDenied.suggestion.deleteMac`) vẫn để "Get Info" và "Locked" bằng tiếng Anh;
  đó là chỗ chưa đồng bộ có sẵn, gộp lại ở đợt sau chứ đừng sửa lẻ.
- **locked / unlock (cờ Locked của macOS) → `bị khóa` / `mở khóa`** · macOS Finder (`NE17` "tệp "^0" đã bị khóa", `NE18`
  "bỏ chọn "Đã khóa" rồi thử lại", `AXNODE1` "Đã khóa") · high
- **top folder của một ổ đĩa (root folder) → `thư mục gốc`** · thuật ngữ Microsoft (`root folder` / `top-level folder` /
  `root directory` → "thư mục gốc") · high. Câu theo khung Tier 1 của Finder (`RN33`: "The item "^0" can't be renamed."
  → "Không thể đổi tên mục "^0"."): `Không thể đổi tên thư mục gốc của ổ đĩa từ đây.`
- **"There's nothing at X any more" → `Không còn gì ở "{path}" nữa`** · macOS Finder `PE131` ("^0" doesn't exist
  anymore. → ""^0" không còn tồn tại nữa.") cho khung `không còn … nữa`; bản vi giữ cách nói tồn tại ("không còn gì")
  đúng như bản tiếng Anh, thay vì nói về chính đường dẫn · high
- **"There's already something at X" → `Đã có thứ gì đó ở "{path}"`** · macOS Finder `NE21` ("…vì đã có mục với tên
  đó."), giữ "thứ gì đó" chung chung như bản tiếng Anh (không đoán là tệp hay thư mục) · high
- **not supported → `không hỗ trợ`** · macOS Finder `PE96` ("…vì thao tác không được hỗ trợ."). Ở đây chủ ngữ là ổ đĩa
  nên dùng thể chủ động: `Ổ đĩa này không hỗ trợ việc đó.` · high
- **"isn't available any more" (ổ đĩa biến mất) → `không còn khả dụng nữa`** · macOS Finder `NE7` ("…vì ổ đĩa "^0" không
  khả dụng nữa.") · high
- **"no room left" → `không còn dung lượng trống`** · `dung lượng trống` là từ catalog đã dùng
  (`errors.listing.storageFull.explanation`), Finder nói "ổ đĩa … đã đầy" (`NE5`) cùng nghĩa · high
- **"That password didn't work" → `Mật khẩu đó không đúng`** · macOS Finder `PE77` ("…vì tên hoặc mật khẩu không
  đúng."). Chủ ngữ là mật khẩu, không phải người dùng, đúng yêu cầu của `@key` · high
- **"lost track of" → `mất dấu`** · chính catalog vi đã dùng (`fileExplorer.navigation.driveIndex.tooltipCoalesced*`:
  "macOS đã mất dấu các thay đổi của hệ thống tệp") · high (nhất quán catalog)
- **"on its way out" (tệp đã được đánh dấu xóa) → `sắp bị gỡ bỏ`** · `errors.write.deletePending.message` đã ship đúng
  cụm này ("Tệp này sắp bị gỡ bỏ.") · high (nhất quán catalog)
- **"The destination can't hold that name" → `Đích không dùng được tên đó`** · `đích` là danh từ trần đã chốt (Total
  Commander "Nguồn và đích khác nhau!"), và khung `không thể dùng X` lấy từ `fileOperations.validation.nameNotUsable`
  ("Không thể dùng tên tệp đó"). ❌ Không dùng `không hợp lệ` (Finder `IN_S13`): nó ám chỉ một quy tắc cụ thể, trong khi
  chuỗi này bắt tất cả các kiểu từ chối tên · high (nhất quán catalog)
- **"take an item out of an archive" → `đưa một mục ra khỏi tệp nén`** · `ra khỏi một tệp nén` đã có trong
  `fileExplorer.archive.useTransferToCopyOut`; "from one archive to another" → `từ tệp nén này sang tệp nén khác`
  (`sang` là giới từ catalog dành cho việc chuyển đổi, xem mục `vào` vs `sang` ở đợt 2026-08-08) · high
- **"Something went wrong" → `Có gì đó không ổn`** · catalog đã dùng ba chỗ (`ai.cloud.genericError`,
  `licensing.error.generic`, `onboarding.cloudSetup.status.genericError`); "and Cmdr couldn't tell what" →
  `và Cmdr không rõ là gì` (`không rõ` là cách nói "unknown" của catalog: "Không rõ kích cỡ", "chi phí không rõ") · high
  (nửa đầu); tentative (nửa sau)

Hai chuỗi dễ dịch sai, và cách chốt:

- **`errors.mutation.timedOut` KHÔNG phải là thất bại**: thao tác chưa bị hủy và vẫn có thể thành công. Bản vi đi theo
  đúng khung của `fileOperations.mkdir.timeoutMessage` ("Ổ đĩa có thể chậm, nên thư mục vẫn có thể đã được tạo."):
  `Ổ đĩa vẫn chưa phản hồi, nên thay đổi vẫn có thể đã được thực hiện.` `phản hồi` là động từ dành cho máy móc (macOS
  AppKit "ứng dụng không phản hồi"), `trả lời` chỉ dùng khi người dùng trả lời hộp thoại.
- **`errors.volume.deviceSessionReset` KHÔNG phải là rút thiết bị ra**: máy vẫn đang cắm, chờ vài giây rồi thử lại là
  được. Bản vi lấy đúng cặp câu của `errors.listing.deviceReconnecting` ("Thiết bị vẫn đang cắm…" + "Hãy đợi vài giây
  rồi thử lại."): `Thiết bị đã khởi động lại kết nối. Hãy đợi vài giây rồi thử lại.` ❌ Đừng bao giờ viết `rút`/`tháo` ở
  khóa này.

Ghi chú khác:

- **Dấu nháy quanh `{path}` giữ nguyên kiểu ASCII thẳng `"…"` như bản tiếng Anh**, giống cách catalog đang làm với
  `{name}` (`fileExplorer.renameConflict.description`, cặp `rename.chainKeptOriginalName*`). Đổi sang nháy cong `“…"` là
  việc của một đợt di trú toàn catalog, không sửa lẻ ở đây.
- `errors.mutation.notFound` và `errors.volume.notFound` có bản tiếng Anh giống hệt nhau nên dùng chung một bản dịch.
- Không cần `sameAsSourceJustification`: cả 31 giá trị đều khác bản tiếng Anh.

## Lỗi khi đổi tên / tạo mới, đợt 2: hai khóa `errors.mutation.trash*` (2026-08-23)

Hai khóa thêm sau đợt 31 khóa ở trên, cùng bề mặt (một dòng dưới ô nhập tên hoặc trong thông báo nhỏ), cùng họ RAW, cùng
luật giọng (không `lỗi`/`thất bại`). Dùng lại từ đã chốt: ổ đĩa (volume), mục (item), `macOS` nguyên văn.

- **Trash (tên vị trí, viết hoa trong bản tiếng Anh) → `Thùng rác`** · macOS Finder Tier 1 ("Trash" → "Thùng rác",
  "Moves items to the Trash" → "Di chuyển các mục vào Thùng rác"), AppKit `Common` ("Trash" → "Thùng rác"), kiểm chứng
  2026-08-23. Theo đúng luật viết hoa đã ghi ở đợt 2026-07-23: viết hoa `Thùng rác` khi câu gọi tên chính vị trí đó (như
  hai khóa này), để thường `thùng rác` khi nó là một phần của hành động (`Chuyển vào thùng rác`). Giới từ là `vào`,
  không phải `sang`/`tới` · high
- **"the only way is to delete permanently" → `cách duy nhất là xóa vĩnh viễn`** · `xóa vĩnh viễn` là từ đã chốt cho
  permanently delete (macOS AppKit `Document`: "permanently delete" → "xóa vĩnh viễn"), catalog cũng đã ship đúng cụm
  này ở `errors.write.trashNotSupported.suggestion`. Cả câu:
  `Ổ đĩa này không có Thùng rác, nên cách duy nhất là xóa vĩnh viễn.` ❌ Đừng dùng `không hỗ trợ` ở đây: bản tiếng Anh
  nói ổ đĩa KHÔNG CÓ Thùng rác, khác với `errors.volume.notSupported` · high
- **"macOS wouldn't move this" (hệ điều hành từ chối) → `macOS đã từ chối chuyển mục này`** · `từ chối` là từ Apple dùng
  cho việc khước từ (macOS Finder ""^0" đã từ chối yêu cầu của bạn.", "Yêu cầu bị từ chối"; AirDrop "Decline" → "Từ
  chối"), và catalog đã chốt `Từ chối` cho deny/reject ở đợt phê duyệt theo dòng. `mục này` = "this item" giống
  `errors.mutation.fileLocked` ("Mục này đang bị khóa."). Chuỗi cố ý ngắn vì lý do kỹ thuật hiện riêng dưới "Chi tiết kỹ
  thuật", nên đừng thêm gợi ý khắc phục · high

## Ba biến thể phần thân của hộp thoại báo cáo sự cố (`crashReporter.dialog.body.*`, 2026-08-23)

Hộp thoại lần khởi động kế tiếp giờ chọn một trong ba câu tùy theo những gì báo cáo ghi lại. `.ended` (Cmdr thoát đột
ngột) giữ nguyên bản dịch cũ; hai khóa mới phải nói ĐÚNG sự thật:

- `.keptRunning`: sự cố xảy ra ở một tác vụ nền và Cmdr **vẫn chạy tiếp**, người dùng tự thoát. Tuyệt đối không được nói
  ứng dụng thoát, đóng, hay dừng.
- `.unknown`: báo cáo do phiên bản Cmdr cũ ghi, không biết ứng dụng có chạy tiếp hay không, nên câu phải đúng cho cả hai
  trường hợp: không nói thoát, cũng không nói chạy tiếp.

Từ và cách nói đã chốt cho nhóm này:

- **"ran into a problem" → `gặp sự cố`** · macOS Finder
  (`Nếu bạn tiếp tục gặp sự cố, hãy gặp quản trị viên hệ thống của bạn.`) và AppKit
  (`Đã có sự cố khi truy xuất thông tin dịch vụ từ ứng dụng.` = "There was a problem retrieving…"), kiểm chứng trong
  `_ignored/i18n/vi/macOS/`, 2026-08-23 · high. Đây là bằng chứng quan trọng: `gặp sự cố` là "gặp vấn đề", KHÔNG hàm ý
  ứng dụng đã thoát, nên dùng được cho cả `.keptRunning` lẫn `.unknown`. Từ mang nghĩa "thoát" trong catalog là
  `thoát đột ngột` (chỉ dành riêng cho `.ended`).
- **"kept running" → `vẫn tiếp tục chạy`** · macOS AppKit `NSExceptionAlert`
  (`Chọn "Tiếp tục" để tiếp tục chạy trong trạng thái không nhất quán.`), đúng ngay ngữ cảnh hộp thoại sự cố · high. Nối
  bằng `nhưng vẫn` chứ không phải `và vẫn`: `nhưng vẫn` là kết hợp tự nhiên trong tiếng Việt và giữ nguyên nghĩa của
  "and kept running".
- **"in the background" → `ở chế độ nền`** · thuật ngữ Microsoft (`background` tính từ, định nghĩa "operating without
  interaction with the user while the user is working on another task" → `nền`; `background task` → `tác vụ nền`),
  `VIETNAMESE.tbx`, kiểm chứng 2026-08-23 · high. Pile KHÔNG có chuỗi `chế độ nền` nào, nhưng catalog Cmdr đã dùng
  `chạy ở chế độ nền` cho "run in the background", nên giữ cho nhất quán.
- **"Here''s a report…" (KHÔNG phải "a crash report") → `Đây là báo cáo kèm chi tiết có thể giúp khắc phục việc này.`**
  · đúng câu thứ hai của `.ended` sau khi bỏ chữ mang nghĩa "crash" (`báo cáo sự cố` → `báo cáo`) · high. Cả ba biến thể
  dùng chung câu này để hộp thoại chỉ khác nhau ở câu đầu.
- **Ghi chú của Apple để tham khảo về sau**: macOS AppKit dịch "unexpectedly quit" là `thoát bất ngờ` và "The last time
  you opened %@" là `Lần cuối cùng bạn mở %@` (`AppKitErrors.json`, 2026-08-23). Catalog Cmdr đang dùng `thoát đột ngột`
  và `Lần trước`; giữ nguyên cho nhất quán, ghi lại đây phòng khi có đợt chuyển sang từ của Apple.

Giá trị đã chốt:

- `.keptRunning` →
  `Lần trước Cmdr đã gặp sự cố ở chế độ nền nhưng vẫn tiếp tục chạy. Đây là báo cáo kèm chi tiết có thể giúp khắc phục việc này.`
- `.unknown` → `Lần trước Cmdr đã gặp sự cố. Đây là báo cáo kèm chi tiết có thể giúp khắc phục việc này.`

## Bổ sung: `thoát bất ngờ`, `ở chế độ nền`, và hai hướng đã loại

- **SỬA — "quit unexpectedly" → `thoát bất ngờ`** · `macOS/AppKit/AppKitErrors.json:90` ("ứng dụng thoát bất ngờ") ·
  high. Đây là lần duy nhất `bất ngờ` xuất hiện trong toàn bộ kho tham chiếu tiếng Việt ngoài Total Commander, và nó nằm
  đúng khái niệm của chúng ta. Giá trị cũ `thoát đột ngột` không có nguồn nào chứng thực, nên `.ended` đã đổi theo
  Apple. Phần còn lại của câu giữ nguyên.
- **"in the background" → `ở chế độ nền`, GIỮ NGUYÊN** · thuật ngữ Microsoft chốt phần đầu `nền` (`background task` →
  `tác vụ nền`, id=19019; `background` tính từ → `nền`, id=18758; `background printing` → `in dưới nền`, id=18908), và
  catalog của Cmdr đã dùng `ở chế độ nền` nhất quán ở chín chuỗi trở lên (`settings.indexing.enabled.description`,
  `ai.toast.downloadCloseTooltip`, `indexing.firstConnect.body`, `fileOperations.transferProgress.stallUnknown`, …).
  Cùng một từ đầu `nền`, nên đổi riêng một chuỗi sang `tác vụ nền` hay `dưới nền` chỉ phá vỡ tính nhất quán mà không
  được gì · high.
- ❌ **Không dùng `ngầm`**: cả hai lần xuất hiện trong kho thuật ngữ đều nói về thứ nằm dưới lòng đất (`khu vực ngầm`,
  `đi đường ngầm`), không phải tiến trình chạy nền.
- ❌ **Không dùng `hậu trường`**: không có lần xuất hiện nào trong toàn bộ kho.
- ⚠️ **Bẫy nghĩa "hình nền"**: hầu hết các lần `nền` xuất hiện trong kho là nghĩa THỊ GIÁC (`hình nền` = ảnh nền,
  `màu nền`, `màn hình nền`); cả 25 kết quả đầu từ `nautilus.po` đều là chuỗi về ảnh nền màn hình. Đừng lấy chúng làm
  bằng chứng cho nghĩa "chạy nền".
- **"kept running" → `vẫn tiếp tục chạy`** · `macOS/AppKit/NSExceptionAlert.json` ("Chọn "Tiếp tục" để tiếp tục chạy
  trong trạng thái không nhất quán") · high. **Thì không phải là vấn đề ở tiếng Việt**: tiếng Việt không chia động từ
  theo thì, nên câu quá khứ dùng chính động từ ấy với `Lần trước` ở đầu câu. Vấn đề "không nguồn nào có câu quá khứ nói
  ứng dụng sống sót" mà tiếng Đức gặp phải đơn giản là không tồn tại ở đây.
- **`sự cố` là "problem" chung, KHÔNG phải "crash"** · Finder ("Nếu bạn tiếp tục gặp sự cố, hãy gặp quản trị viên hệ
  thống của bạn"), AppKit ("Đã có sự cố khi truy xuất thông tin dịch vụ") — cả hai đều nói về vấn đề mà ứng dụng vượt
  qua được. Vì thế `crashReporter.dialog.privacyNote` giữ nguyên `đã gặp sự cố` dù tiếng Anh đã đổi từ "crashed" sang
  "ran into the problem": bản tiếng Việt vốn đã trung tính. Chỉ làm mới dấu vân nguồn.
- **`báo cáo sự cố` mới là "crash report"** · nên "a report" (không có "crash") phải là `báo cáo` trần. Tiêu đề và thông
  báo xác nhận cắt y hệt: `Gửi báo cáo sự cố?` / `Gửi báo cáo?`, `Đã gửi báo cáo sự cố. …` / `Đã gửi báo cáo. …` · high.
