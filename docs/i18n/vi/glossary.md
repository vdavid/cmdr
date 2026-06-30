# vi glossary

The living term glossary for translating Cmdr into this language: one entry per recurring term, in the
`chosen · sources · confidence` format. Build and extend it DURING translation, and read it before every pass.

- **Source every term from the reference pile, never guess.** Mine `_ignored/i18n/vi/` for how Apple, Microsoft, and
  GNOME/Xfce render the term and for similar sentences (recipes: `docs/i18n/reference-pile/how-to-mine.md`). Cite the
  source(s) and a confidence (`confirmed` / `high` / `tentative`).
- **This folder is this language home.** Capture new term decisions here, and other findings as sibling files.

Format, the confidence scale, and the full process: [i18n-translation.md](../../guides/i18n-translation.md).

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
  "hàng đợi", verb → "cho vào hàng"; adapted to `đưa vào hàng đợi` for the UI action). "Transfer queue" →
  `hàng đợi truyền`. `high`.
- **background / run in the background: `nền` / `chạy ở chế độ nền`** · MS terminology ("background task" → "tác vụ
  nền"). "Keep running in the background" → `giữ chạy ở chế độ nền`. `high`.
- **transfer (the operation, as a countable noun): `lần truyền`** · descriptive (`lần` = instance/occurrence + `truyền`
  = transfer); the queue lists individual copy/move/delete ops. The window/list heading "Transfers" → `Các lần truyền`.
  `tentative`.

Wave-1-prep phrasings settled (keep consistent): "Waiting" (queued status) → `Đang chờ`; "Running" → `Đang chạy`; "Done"
→ `Xong`; "Cancelled" → `Đã hủy`; "Couldn''t finish" (gentle failed wording) → `Chưa hoàn tất được` (negative-capability
framing per the error voice, avoids a bare "lỗi"/"thất bại"). "Cancel selected" → `Hủy mục đã chọn`. "Show transfer
queue" (command) → `Hiện hàng đợi truyền`.

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

Added during the dialog-polish pass (2026-06-30): four short field labels / tooltips in `fileOperations.json` (the
copy/move + delete dialogs). Reuses prior terms (scan/scanning → `quét`/`đang quét`, source → `nguồn`, destination →
`đích`, file ops → `Thao tác tệp`):

- **"Action:" (field label before the Copy/Move or Trash/Delete segmented control): `Thao tác:`** · the catalog''s
  established operation term (`Thao tác tệp` = file operations). Labels which operation to run; `thao tác` (operation,
  user-performed action) reads more natural here than MS''s `hành động` (action, behavioral sense) or macOS''s `tác vụ`
  (task; macOS uses it for "undo this action"). Catalog-consistent. `high`.
- **"Route:" (field label before a `source → destination` line): `Lộ trình:`** · `lộ trình` = route/itinerary, the
  origin-to-destination path sense (standard vi, e.g. maps navigation). NOT MS''s `định tuyến` (route a packet —
  networking sense, wrong here). Matches the evocative-but-clear English "Route". `tentative` (no direct pile string for
  this UI sense; networking source rejected).
- **"Scanning…" (spinner tooltip / SR label while counting): `Đang quét…`** · reuses the glossary''s "scan in progress"
  → `Đang quét`; ellipsis `…` kept. `high`.
- **"Scan complete" (checkmark tooltip / SR label once counting finished): `Đã quét xong`** · `đã…xong` completed
  aspect, parallel to `Đang quét` (in-progress) ↔ `Đã quét xong` (done). MS/macOS "complete/completed" →
  `hoàn tất`/`xong`; the concise completed-aspect form fits a tooltip. `high`.
- **"This folder doesn''t exist yet. Cmdr will create it during the copy/move." (yellow inline warning under the
  destination box): `Thư mục này chưa tồn tại. Cmdr sẽ tạo nó khi sao chép.` / `… khi di chuyển.`** · `chưa tồn tại`
  (not-yet-exist) is the precise "doesn''t exist yet" counterpart to the catalog''s `đã tồn tại` (already exists); GNOME
  Nautilus attests plain `không tồn tại` ("đích đến là "%s" không tồn tại") and `chưa` for "not yet". `tạo nó` (create
  it, inanimate pronoun) is attested in the pile (Nautilus "không có quyền tạo nó ở đích đến");
  `khi sao chép`/`khi di chuyển` (when copying/moving) renders "during the copy/move" concisely. Two literal sentences
  per the en `@key` (no ICU select; the verb is operation-specific). `high`.
