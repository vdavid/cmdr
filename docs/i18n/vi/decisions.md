# vi decisions

The rationale journal behind `terms.json`: why a term won, which catalog keys a ruling shaped, and the incidents that
encode a constraint. Not read by default; `pnpm i18n:brief` pulls the sections whose heading cites a batch's keys, so
keep citing keys in backticks in every heading. The term rulings themselves live in `terms.json` (one entry per concept
from `../concepts.json`, plus this locale's `concepts-proposed.json`); open questions for a native reviewer live in
`review-queue.md`. Style and voice: `style.md`.

Evidence tiers throughout: macOS Finder/AppKit/System Settings (Tier 1, from the pile or read live off the installed
bundles), Microsoft terminology `VIETNAMESE.tbx` (Tier 2), GNOME Nautilus / Xfce Thunar / KDE Dolphin / Total Commander
(Tier 3). macOS wins ties, and catalog consistency wins over a pile-ideal form that would fork a term mid-catalog.

## Tên mục Cài đặt, nhóm và phạm vi phím tắt (`settings.section.*`, `shortcuts.scope.*`, `fileExplorer.navigation.group*`)

Keep these identical everywhere a string names them:

- Volume-switcher groups: Favorites → `Mục ưa thích`, Volumes → `Ổ đĩa`, Cloud → `Đám mây`, Mobile →
  `Thiết bị di động`, Network → `Mạng`.
- Settings sections: Appearance → `Giao diện`, Behavior → `Hành vi`, File systems → `Hệ thống tệp`, Search →
  `Tìm kiếm`, Viewer → `Trình xem`, Advanced → `Nâng cao`, Keyboard shortcuts → `Phím tắt`, License → `Giấy phép`,
  Updates & privacy → `Cập nhật & quyền riêng tư`, Navigation & file ops → `Điều hướng & thao tác tệp` (joined with `&`
  like the Updates section), SMB/Network shares → `Mục chia sẻ SMB/mạng`.
- A path to a Settings section is `Cài đặt › <tên mục>` with the section name verbatim from `settings.section.*`. The AI
  family (`ai.translateError.*`, `ai.cloudConsent.*`, `askCmdr.gate.*`) and `fileExplorer.quickLookHint.configurable`
  keep EN's `>` (`Cài đặt > AI`, `Cài đặt > Nâng cao`, `Cài đặt > Phím tắt`), as their `@key` asks.
- View modes: Full → `Đầy đủ`, Brief → `Rút gọn`. Columns: Name → `Tên`, Ext → `Đuôi`.
- Shortcut scopes: App → `Ứng dụng`, Main window → `Cửa sổ chính`, File list → `Danh sách tệp`, Brief mode →
  `Chế độ rút gọn`, Full mode → `Chế độ đầy đủ`, Volume chooser → `Bộ chọn ổ đĩa`, Command palette → `Bảng lệnh`, About
  window → `Cửa sổ Giới thiệu`, Onboarding → `Thiết lập ban đầu`, Places → `Vị trí`, Servers → `Máy chủ`.
- The onboarding menu path keeps its ellipsis: `Cmdr > Thiết lập ban đầu…`. "What's new in Cmdr" (dialog title) →
  `Có gì mới trong Cmdr`. Settings > Updates (crash-toast button) → `Cài đặt > Cập nhật`.

## Giọng lỗi: các cụm đã chốt (`errors.*`, `updates.failure.*`, `fileExplorer.network.browser.status.error`)

- "Here's what to try:" → `Bạn có thể thử:`, in all 60 `errors.json` keys that carry it, including after an opening
  sentence (`errors.provider.*`). ❌ Not `Đây là những cách để thử:`: word-for-word and longer.
- couldn't / can't / unable to → `không thể` (GNOME "Không thể", Thunar "Không thể gắn kết"): the calm
  negative-capability frame, never a bare `lỗi` / `thất bại`.
- "Try again?" (several short error strings) → `Thử lại?`, a plain question with no softening particle, so the five
  strings that use it stay alike.
- "Something went wrong" → `Có gì đó không ổn` (`ai.cloud.genericError`, `licensing.error.generic`,
  `onboarding.cloudSetup.status.genericError`, `errors.mutation.unexpected`, `askCmdr.error.provider`).
- "Try again in a moment" → `Hãy thử lại sau giây lát.`
- An update check that didn't land (`updates.failure.check`) is whole sentences: `Cmdr không kiểm tra được bản cập nhật.
  {reason}`. No "Error:" prefix survives anywhere in the catalog.
- "Error" as a bare status cell (`fileExplorer.network.browser.status.error`) → `Sự cố`; its `@key` asks to avoid the
  word where the language has a friendlier one.
- "fixes" → `việc khắc phục`, never `sửa lỗi` (the voice avoids `lỗi`). The exception is David's own "helps me fix bugs"
  (`onboarding.stepBeta.openBeta`, `sửa lỗi`) against "spot bugs" (`feedbackIntro`, `phát hiện lỗi`), which is his
  first-person talk about software bugs.

## Trạng thái hàng đợi và nhật ký thao tác (`queue.row.status`, `operationLog.status.*`, `operationLog.outcome.*`, `operationLog.initiator.*`, `indexing.eta.*`)

One status vocabulary for both windows, since the queue shipped first and the log aligned to it:

- Waiting / Queued → `Đang chờ`; Running → `Đang chạy`; Paused → `Đã tạm dừng`; Done → `Xong`; Cancelled → `Đã hủy`;
  "Couldn't finish" (the gentle failed wording) → `Chưa hoàn tất được` (macOS attests `thao tác chưa hoàn tất`).
- Per-item outcomes: Skipped → `Đã bỏ qua` (past aspect, like the other completed outcomes), Rolled back →
  `Đã hoàn tác`.
- Initiators: You → `Bạn`, AI client → `Máy khách AI` (`máy khách`, the client-server counterpart of `máy chủ`), Agent →
  `Tác nhân`.
- "Cancel selected" → `Hủy mục đã chọn`. "and N more items" → `và thêm {countText} mục nữa`.
- ETA: "Almost done" → `Sắp xong`; `Ns left` / `Nm left` → `còn Ns` / `còn Nm` (`còn` leads, the unit letters stay
  attached).
- Summary verbs reuse the transfer past tense: `Đã sao chép`, `Đã di chuyển`, `Đã xóa`, `Đã chuyển … vào thùng rác`,
  `Đã đổi tên`, `Đã tạo`, `Đã nén`. Plurals collapse to one `other` branch.

## Nhãn macOS trong phần thiết lập ban đầu (`onboarding.stepFda.*`, `onboarding.stepOptional.*`, `onboarding.stepAi.*`)

- Quit & Reopen → `Thoát & Mở lại` (macOS "Reopen" → `Mở lại`); Applications → `Ứng dụng`; Documents → `Tài liệu`;
  Downloads → `Tải về`; Desktop → `Màn hình nền` (all macOS Finder).
- Full Disk Access (the pane name) → `Quyền truy cập đầy đủ vào ổ đĩa` (live bundle, see `full-disk-access` in
  `terms.json`); Local Network (the permission) → `Mạng cục bộ`; "Accepting incoming connections" →
  `Chấp nhận kết nối đến` (no pile string, best effort).
- "review and apply" / "at will" (the with/without-AI table) → `xem lại rồi áp dụng` / `tùy ý`.
- "Here is a report…" style lines and the checklist live in the onboarding rewrite section further down.

## Gợi ý bấm đúp vào nền khung (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`, `settings.behavior.doubleClickOnPaneNotificationSeen.*`)

Casual product voice, free copy (no pile source):

- "What just happened?" → `Chuyện gì vừa xảy ra?`; "Don't like it?" → `Không thích à?` (`à` softens); "Never do this
  again" → `Đừng làm vậy nữa`; "I like it" → `Tôi thích` (the USER speaking, so `Tôi`, not David's `mình`).
- Body: `Thao tác này đưa bạn đến thư mục cha`.
- The switch reads `Bấm đúp vào nền khung để lên thư mục cha` ("go up a folder" → `lên thư mục cha`), description
  `Đó là khoảng trống xung quanh danh sách tệp, không phải một hàng tệp.` Empty space in a list → `khoảng trống`; pane
  background → `nền khung`.

## Tệp quá lớn cho hệ thống tệp (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- too large (for X) → `quá lớn (đối với X)` (GNOME "Tập tin quá lớn đối với vị trí dán").
- "formatted as FAT32" → `được định dạng FAT32` (Finder Get Info "Định dạng:"); FAT32 / exFAT stay verbatim.
- a drive holding files → `chứa` (`không thể chứa các tệp lớn hơn {maxSize}`), which reads more natural than `lưu trữ`
  (archive) for capacity.
- "{name} is {size}" → `{name} có dung lượng {size}`; "files this large" → `các tệp lớn cỡ này`; "and N more files" →
  `và thêm {countText} tệp nữa`; "no such limit" → `không có giới hạn như vậy`.

## Hộp thoại sao chép, di chuyển và xóa (`fileOperations.transferDialog.*`, `fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`, `queue.row.label`)

- The control that picks which operation to run (`transferDialog.operationAria`) → `Thao tác`: the catalog's operation
  word, over MS `hành động` (behavioral) and macOS `tác vụ` (task).
- "Scanning…" (spinner / SR label while counting) → `Đang quét…`.
- "This folder doesn't exist yet. Cmdr will create it during the copy/move." →
  `Thư mục này chưa tồn tại. Cmdr sẽ tạo nó khi sao chép.` / `… khi di chuyển.` (`chưa tồn tại` pairs with `đã tồn tại`;
  two literal sentences per the `@key`, no ICU select).
- Queue row progress arms: `Đang đổi tên` / `Đang tạo thư mục` / `Đang tạo tệp`; archive edit → `Đang chỉnh sửa tệp nén`.
- The conflict-policy radios (`transferDialog.policySkip`, `.policyOverwrite`, `.policyOverwriteSmaller`,
  `.policyOverwriteOlder`, `.policyStop`) change their WORDING for a single clash ("Skip" vs "Skip all", "Ask later" vs
  "Ask for each"), so they carry an exact `=1 {…}` arm (`Bỏ qua`, `Ghi đè`, `Ghi đè nếu nhỏ hơn`, `Ghi đè nếu cũ hơn`,
  `Hỏi sau`) beside `other`. This is not the forbidden English-shaped `one` arm: the noun doesn't inflect, the meaning
  does, and without it one clash read "Skip all".
- The delete dialog's switch (`delete.trashSwitch`) → `Chuyển vào thùng rác`, identical to the
  `transferDialog.titleVerbOnly` arm so switch and button read as one pair; the confirm button with the switch off →
  `Xóa`. `Thùng rác` is capitalized only where a string names the Trash location itself ("check the Trash"), lowercase
  inside an action phrase.
- "From" / "To" headings (`transferDialog.sourceGroupTitle` / `.targetGroupTitle`) → `Từ` / `Đến` (Total Commander vi
  ships this exact pair, entries 662/663); the controls under them keep `Ổ đĩa đích` / `Đường dẫn đích`.

## Duyệt và chỉnh sửa tệp nén (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `settings.archives.*`, `fileOperations.archivePassword.*`, `errors.listing.archiveUnreadable.*`)

- A browsable zip/tar/7z is `tệp nén` (the catalog already said so in `settings.listing.sizeDisplay.description` and
  `settings.fileViewer.suppressBinaryWarning`). ❌ Not the archival `kho lưu trữ` (GNOME) or `Bộ lưu trữ` (macOS "iOS
  Package Archive"): both read as backup storage. "zip archives" → `tệp nén zip`, "archive format" → `định dạng nén`.
- "a fresh copy (of a file)" → `một bản mới` (`nhờ người đã gửi nó cung cấp một bản mới`).
- "What pressing Enter does" → `Nhấn Enter sẽ làm gì`; the Enter key name stays `Enter`. Row descriptions
  (`settings.archives.zip.description`, `.bundle.description`, `.ooxml.description`) share the frame
  `Nhấn Enter sẽ làm gì với tệp …, … hoặc ….`, with no comma before `hoặc`.
- The password dialog: body `… được bảo vệ bằng mật khẩu.` (TC/DC phrasing), input aria-label `Mật khẩu tệp nén`, button
  `Mở khóa`.

## Nén (`commands.fileCompress.*`, `fileOperations.transferDialog.toggleCompress`, `fileOperations.transferDialog.confirmCompress`, `fileOperations.transferDialog.pathErrorNotZip`, `settings.archives.compressionLevel.*`)

- compress → `Nén` (Finder "Nén các mục", `Compress ${sources}` → "Nén ${sources}"); progress → `Đang nén`; result toast →
  `Đã nén` (mirrors `Đã sao chép {phrase}`); `scanTitleCompress` → `Đang xác minh trước khi nén...`.
- The NEW archive the dialog is about to write is named `tệp lưu trữ` (Finder "Zip archive" → "Tệp lưu trữ Zip"), e.g.
  `pathErrorNotZip` = `Tên tệp lưu trữ phải kết thúc bằng ".zip".`; every archive the user browses stays `tệp nén`.
- replace (overwrite warning) → `thay thế` (Finder "Replace" → "Thay thế").
- compression level → `Mức nén`; slider ends Faster / Smaller → `Nhanh hơn` / `Nhỏ hơn` (TC "nén nhanh nhất (1)",
  "nén tối đa").

## Dán nội dung bảng nhớ tạm thành tệp (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- content (of the clipboard) → `nội dung`; text (content, not viewer lines) → `văn bản`, distinct from `dòng`.
- "as a file" → `thành tệp` / `thành {filename}` (transform-into; tighter than `dưới dạng`): `Dán nội dung bảng nhớ tạm
  thành tệp`. The `other` branch of `pastedAsFile` → `văn bản`.
- "Do nothing" (radio option) → `Không làm gì`.

## Nhật ký thao tác (`operationLog.*`, `commands.logOperationLog.*`)

- operation log → `Nhật ký thao tác` (dialog title and command label); "your operation history" →
  `lịch sử thao tác của bạn`.
- The rollback state set uses one verb: "Can't roll back" → `Không thể hoàn tác`, "Can roll back" → `Có thể hoàn tác`,
  "Rolling back" → `Đang hoàn tác`, "Rolled back" → `Đã hoàn tác`, "Partly rolled back" → `Đã hoàn tác một phần`, "roll
  them back" → `hoàn tác chúng`.
- item (generic logged item) → `mục`; the summary lines count `mục`.

## Ask Cmdr: trò chuyện, chi phí và công cụ (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- chat (noun) → `trò chuyện` (MS): "Chats" → `Trò chuyện` (rail button and sessions title alike), "New chat" →
  `Trò chuyện mới`, "Start a fresh chat" → `Bắt đầu trò chuyện mới`, "Back to chat" → `Quay lại trò chuyện`.
- message → `tin nhắn` ("Send message" → `Gửi tin nhắn`, "Load earlier messages" → `Tải tin nhắn trước đó`).
- archive / unarchive a chat → `Lưu trữ` / `Bỏ lưu trữ` (Finder `AR40`; the Gmail/Zalo pairing for the undo), never the
  zip sense `tệp nén`.
- "Remove attachment" → `Gỡ tệp đính kèm` (the attachment is unstaged, not deleted).
- database → `cơ sở dữ liệu`; dashboard → `bảng thông tin`; "bills you directly" → `thanh toán trực tiếp với bạn`; free
  (of charge) → `miễn phí` (❌ not MS's `tự do`); "free, on-device" → `miễn phí, cục bộ`.
- "couldn't reach the provider" is restructured around Finder's "Không thể kết nối máy chủ": `kết nối`, no literal
  "reach".
- estimate → `ước tính` (❌ not MS's `báo giá`, a sales quote); "about {amount}" → `khoảng {amount}`; "These are
  estimates" → `Đây chỉ là ước tính`; cost → `chi phí` (❌ not `giá vốn`); "cost unknown" → `chi phí không rõ`; spending
  → `chi tiêu`; usage → `mức sử dụng`.
- debugging → `gỡ lỗi`; the generic tool-call fallback "working" → `đang xử lý`; look up (a logged operation) →
  `tra cứu`; a request that wasn't possible → `Yêu cầu đó không khả dụng`.
- Ask Cmdr's "file history" tool reads the operation log, so "file history" → `lịch sử thao tác` ("Searching your file
  history" → `Đang tìm kiếm trong lịch sử thao tác của bạn`). If English ever splits the two, don't fork silently.
- Consent-screen items ("Sentence case, no period" per `@key`) keep no trailing period.

## Lập chỉ mục hình ảnh (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.*`)

- The feature-level word is `hình ảnh` (card `Tìm kiếm hình ảnh`, `Lập chỉ mục nội dung hình ảnh`, the whole
  `fileExplorer.imageIndex.*` status family); concrete per-drive photo counts say `ảnh` (`Đã lập chỉ mục … ảnh`,
  `ảnh trên {name}`). English splits them the same way.
- network drive → `ổ đĩa mạng`; a rarely browsed photo NAS ("photo archive") → `kho ảnh` (`kho`, the storage sense, not
  `tệp nén`); "gently" → `một cách nhẹ nhàng`; "at a limited speed" → `ở tốc độ giới hạn`; "so far" → `cho đến nay` (the
  live counters use `đến giờ`); mark (internal lists) → `đánh dấu`.
- status badge → `huy hiệu trạng thái`; status → `trạng thái`; "Waiting to be indexed" → `Đang chờ lập chỉ mục`; a
  feature off for a drive → `đang tắt` (`Tìm kiếm hình ảnh đang tắt cho ổ đĩa này.`); still working → `vẫn đang xử lý`;
  "of" in a count → `trên` (`settings.mediaIndex.progress.ofTotal`).
- Every count plural here is a single `other` branch, keeping both `{count}` and `{countText}`. `drive.indexing` fronts
  the drive (`Trên ổ đĩa này, …`) to avoid a double `trên`.

## Xem lại việc đổi tên hàng loạt (`askCmdr.renameReview.*`)

- review (verb and modal) → `xem lại` (AppKit "Review Changes…" → `Xem lại Thay đổi…`): title `Xem lại việc đổi tên
  tệp`, "this review" → `Lần xem lại này`, "needs attention" → `cần được xem lại` (no `chú ý` anywhere in macOS vi).
- Allow / Deny (per row) → `Cho phép` / `Từ chối`; Allow all / Deny all → `Cho phép tất cả` / `Từ chối tất cả`.
- "this rename" (one row) → `lần đổi tên này` (`lần`, the catalog's counter for one operation); the generic heading
  keeps the gerund `việc đổi tên tệp`.
- rename cycle (a → b → a) → `chu trình đổi tên` (graph-theory "cycle"; MS `chu kỳ` / `vòng tròn` are other senses);
  badge `(chu trình)`.
- The extension badge says `(đuôi tệp)`, matching the catalog's `đuôi tệp`.
- "Ask Cmdr to prepare it again" → `Hãy nhờ Cmdr chuẩn bị lại.` (`nhờ` asks a helper; `yêu cầu` read formal and turned
  the brand into an object).
- "on the next pass" → `ở lượt quét tiếp theo`.

## Lập chỉ mục ổ đĩa đang tắt (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- A flat present state takes `không`, never `chưa` ("not yet"): "no drive is indexed" →
  `không có ổ đĩa nào được lập chỉ mục` (the catalog's `không có + N + nào + được + V` shape). `chưa` only where the
  sentence really means "not yet" (`refusedIndexingOff`: `vẫn chưa được lập chỉ mục`).
- "off with X" (overridden by the master switch) → `Tắt theo X`: badge `Tắt theo lập chỉ mục ổ đĩa`.
- "turn this back on" (a settings toggle) → `bật lại mục này`, matching `nếu tắt mục này`.

## Tên tiện ích macOS, mục chia sẻ, liên kết mềm, beta công khai (`errors.listing.*`, `settings.network.*`, `settings.behavior.*Seen.*`, `queryUi.results.live.*`, `settings.mediaIndex.*`)

Rulings the whole catalog now follows, each one a place it had drifted:

- **Apple's utility names are localized in vi, so the error advice uses them**: Disk Utility → `Tiện ích ổ đĩa`, First
  Aid → `Sửa nhanh`, Activity Monitor → `Giám sát hoạt động` (verified on macOS 27.0, live bundles: `Disk Utility.app`
  `InfoPlist.loctable` `CFBundleDisplayName`, `MainMenu.loctable` "First Aid", `Activity Monitor.app`
  `InfoPlist.loctable`, 2026-09-24), in `errors.listing.couldntReadUnknown.suggestion`, `.ioSerious.suggestion`,
  `.diskReadProblem.suggestion`, `.unexpectedSystemResponse.suggestion`, `.notEnoughMemory.suggestion`,
  `.temporarilyUnavailable.suggestion`. Spotlight and Mission Control stay English (Apple keeps them). The same live read
  shows Preview → `Xem trước` and TextEdit → `TextEdit` (`settings.advanced.showSafeSaveFiles.description`); Force Quit
  → `Bắt buộc Thoát` and Character Viewer → `Biểu tượng & Ký hiệu` (pile, `vi/macOS/`) already shipped that way.
- **System Settings is `Cài đặt hệ thống`** wherever the catalog wrote it by hand (`downloads.fda.openSystemSettings`,
  `shortcuts.conflict.systemShortcut` with the Keyboard pane → `Bàn phím`); the runtime `{system_settings}` tokens stay
  tokens.
- **A share is `mục chia sẻ`**: the settings cluster that said `bản chia sẻ` (`settings.network.*`,
  `settings.section.smbNetworkShares`, `settings.summary.smbNetworkShares`, `settings.appearance.tintSmb.description`,
  `settings.indexing.askForEachDrive.description`, `settings.advanced.mountTimeout.description`,
  `errors.listing.remotePermissionDenied.*`) and the bare `Chia sẻ này` (`fileOperations.transferDialog.smbNativeNote`,
  `fileOperations.transferProgress.smbNativeNote`) now match the 37 other keys.
- **symlink → `liên kết mềm`** everywhere (`errors.listing.symlinkLoopErrno.*`, `errors.write.symlinkLoop.*`,
  `fileExplorer.entry.brokenSymlink`, `fileExplorer.selectionInfo.symlinkHint`), replacing `liên kết tượng trưng` and
  the bare loanword `symlink`.
- **open beta → `beta công khai`** (`onboarding.stepBeta.openBeta`, `.analyticsLede`,
  `settings.analytics.enabled.description`, alongside `licensing.about.version`), replacing `open beta` and `beta mở`.
- **The internal "has been shown" flags say `hiển thị`** (`settings.behavior.*Seen.*`,
  `settings.advanced.oldMacosNoticeShown.description`), and "show up here" says `xuất hiện`
  (`operationLog.dialog.empty`, `settings.askCmdr.spend.empty`).
- **A search-walk "scan" is `quét`** (`queryUi.results.live.foldersScanned`, `.waitingForAnotherWalk`,
  `.waitingOnPathAria`); "walking" keeps `duyệt` (`queryUi.results.live.walking`). "Waiting" is `Đang chờ`.
- **English-shaped `one` arms removed** from eight `settings.mediaIndex.*` plurals whose two arms were identical.
- `settings.fileExplorer.suppressQuickLookHint.description` quoted a button that doesn't exist (`Đừng hiện nữa`); it now
  quotes `fileExplorer.quickLookHint.dontShowAgain` verbatim (`Không hiển thị lại`).

## Các quyết định lẻ khác

Rulings with no concept in the registry, kept so they aren't relitigated:

- computer → `máy tính`; the user's machine in prose → `máy Mac` (`trên máy Mac này`, `Máy Mac này`), never a bare `Mac`
  as a noun phrase.
- theme options Light / Dark / System → `Sáng` / `Tối` / `Hệ thống`; binary / decimal (size base) → `nhị phân` /
  `thập phân`.
- sidebar (a macOS sidebar) → `thanh bên` (macOS); Cmdr's own side panel is `khung bên` (see `side-panel`).
- git: branch → `nhánh`, commit → `commit`, tag → `thẻ`, repo → `repo` (full "repository" → `kho`), worktree →
  `worktree`, a pack file / object stay English in `errors.git.*`.
- subscription → `đăng ký` (`Gói đăng ký thương mại`); renew → `gia hạn`; deactivate → `hủy kích hoạt`; valid /
  validity → `có hiệu lực` / `hiệu lực`; perpetual → `vĩnh viễn`; tiers Commercial / Personal → `Thương mại` /
  `Cá nhân`.
- regex → `Regex` as a mode label, `biểu thức chính quy` in prose; glob → `Glob`; pattern → `mẫu`; wildcard →
  `ký tự đại diện`.
- zoom level → `mức phóng`; pan / fit → `di chuyển` / `vừa khít`; `View > Zoom > 100%` in
  `commands.handler.zoomResetHintMenu` stays English (a literal menu path, per its `@key`).
- rate limit → `giới hạn tần suất`; hardlink → `liên kết cứng`; event → `sự kiện`; buffer → `bộ đệm`; channel →
  `kênh`; log bundle → `gói`; register (a shortcut) → `đăng ký`; bound to → `được gán cho`; key combination →
  `tổ hợp phím`; jump to → `nhảy đến`; byte → `byte`; cursor → `con trỏ`; toggle (prefix) → `bật/tắt`; recent →
  `gần đây`; offline → `ngoại tuyến`; agent → `tác nhân`; build folder → `thư mục build`.
- "Coming soon" → `Sắp ra mắt`; "Hide boring folders" → `Ẩn các thư mục nhàm chán`.
- click → `bấm` (Finder: 54 `bấm`, zero `nhấp`); press a key → `nhấn`; double-click → `bấm đúp`; a row → `hàng`, a text
  line → `dòng`.
- placeholder (a template token) → `phần giữ chỗ` (MS `chỗ dành sẵn` reads "reserved space"); bookmark → `dấu trang`;
  listing → `danh sách tệp`; preset → `tùy chọn đặt trước` ("back to presets" → `Quay lại tùy chọn đặt trước`), since
  bare `đặt trước` can read "reserved".
- Azure: deployment → `bản triển khai`, resource → `tài nguyên` (Microsoft's product, so MS outranks macOS); a model
  catalog → `thư viện`, qualified as `thư viện mô hình` where Finder's gallery `chế độ xem thư viện` could collide.
- a shell command → `lệnh` (`lệnh "adb"`); leave a field empty → `để trống`; "the usual way" →
  `theo cách thông thường`; "at any time" → `bất cứ lúc nào` (nine shipped strings), even though macOS writes
  `bất kỳ lúc nào`.
- "one" standing in for a just-named thing → `một cái` (`Hãy bật một cái trong cài đặt`, `Bấm để thiết lập một cái
  trong cài đặt`).

## Chỉ mục ổ đĩa: lượt kiểm tra thay đổi (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- **"Checking for changes" (run-kind header) → `Kiểm tra thay đổi`** · verb-phrase shape matching the sibling headers
  (`Quét toàn bộ lần đầu`, `Cập nhật nhanh`); `Kiểm tra` is macOS VI's checking verb (Finder BN9 "Kiểm tra nội dung
  của…"), `thay đổi` is catalog-settled (`các thay đổi gần đây`) and termbase-settled as the MS term · high.
- **"Update the file list" → `Cập nhật danh sách tệp`** · composed from the settled siblings `Lưu danh sách tệp` +
  `Cập nhật chỉ mục` · high.
- **"the check running right now" → `lần kiểm tra đang chạy ngay bây giờ`** · reuses `lần kiểm tra` as this catalog's
  settled phrase for a full check (`tooltipCoalesced`: "lần kiểm tra toàn bộ tiếp theo của Cmdr") and that string's
  closing `sẽ chỉnh lại cho đúng` · high.

## Lần truyền bị đứng yên: thông báo trên hộp thoại + hàng đợi (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

The seven stalled-transfer strings (`fileOperations.transferProgress.stall*` + `close`). Mined 2026-07-31 against
`_ignored/i18n/vi/` (macOS Finder/AppKit Tier 1, MS terminology Tier 2, GNOME Nautilus + Total Commander Tier 3). Reuses
settled terms (close → `đóng`, cancel → `hủy`, destination/source → `đích`/`nguồn`, log → `nhật ký`, transfer
(countable) → `lần truyền`, background → `chạy ở chế độ nền`, file → `tệp`).

- **progress (advancement, in "no progress"): `tiến triển`** · shared-root pick (mining gotcha 4): macOS renders the
  progress noun as `tiến trình` (Finder SD24 "Hiển thị tiến trình sao chép", PW60 "Hiển thị cửa sổ tiến trình") and MS
  terminology as `Tiến độ` (12×). Neither fits a negated "no progress": `tiến trình` is this locale's word for an OS
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
  `Chi tiết: `. Fronting `Chi tiết` keeps it short and puts the useful noun first. `high`.

Phrasings settled (keep consistent): "No progress for {duration}" → `Không có tiến triển trong {duration}` (one value on
`fileOperations.transferProgress.stallNotice`, no final period, shown on both the progress dialog and the queue row,
matching English); "Cancel it, or leave it running in the background." →
`Hãy hủy, hoặc để nó tiếp tục chạy ở chế độ nền.` (`tiếp tục chạy ở chế độ nền` composed from the catalog's
`Giữ chạy ở chế độ nền` + `Vẫn đang chạy ở chế độ nền`).

## Đường dẫn đã sao chép: xác nhận bảng nhớ tạm (`fileExplorer.clipboard.copiedPath`)

Một khóa: dòng thông báo thông tin sau ⌘⌥C. Đường dẫn hiện ngay bên dưới trên một dòng riêng với phông chữ đơn cách, nên
nó KHÔNG phải chỗ giữ chỗ trong câu: câu kết thúc bằng dấu hai chấm và phải đứng vững khi thiếu đường dẫn.

- **"Copied the path, it's now on your clipboard:" → `Đã sao chép đường dẫn vào bảng nhớ tạm:`** · dùng lại
  `clipboard → bảng nhớ tạm` và `path → đường dẫn` đã chốt trong termbase (macOS AppKit) · high. Mở đầu bằng `Đã` khớp
  các thông báo anh em (`Đã sao chép {countText} mục`). Gộp "it's now on your clipboard" vào cụm `vào bảng nhớ tạm`:
  dịch sát bằng đại từ `nó` sẽ lủng củng, và tiếng Việt không dùng sở hữu cho một bảng nhớ tạm duy nhất.

## Hàng đợi thao tác: "operation queue" (`queue.windowTitle`, `queue.heading`, `queue.list.aria`, `queue.row.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

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

## Huy hiệu tiến trình ở góc + thông báo chưa hoàn tất (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

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
- **"Show in operation queue" (the toast's button): `Hiển thị trong hàng đợi thao tác`** · `Hiển thị trong X` is the
  catalog's settled "Show in X" shape, on all six such keys (`commands.fileShowInFinder.mac.label`, `.other.label`,
  `menu.file.showInFinder`, `.showInFileManager`, `errorReporter.bundleSavedToast.reveal`, and this one). The verb comes
  from the `show → hiển thị, KHÔNG hiện` ruling in the 2026-08-30 drift audit below. `high`.
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
  Settings labels that name Cmdr's other chips already use it (`Hiển thị huy hiệu kho`,
  `Hiển thị huy hiệu trạng thái trên tệp hình ảnh`), so a future string that has to SAY "chip" should say `huy hiệu`.
  `tentative`.
- ETA / time-left inside `{detail}` is formatted elsewhere and arrives as the settled `còn {duration}`; these keys pass
  it through untouched.

## Lời nhắc xung đột đứng riêng: dòng ngữ cảnh + ghi chú tạm dừng (`fileOperations.operationConflict.context`/`.pausedNote`)

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
  `phản hồi` (respond): the termbase reserves `phản hồi` for a machine responding (macOS AppKit "ứng dụng không phản
  hồi"). `high` (on the parts); `tentative` (on `trả lời` for answering a dialog).

## Nút "Chạy nền": trạng thái hàng đợi trống của nút Hàng đợi (`fileOperations.transferProgress.background`/`.backgroundAria`)

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

## Hộp thoại thoát khi thao tác đang chạy (`main.quit.*`)

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
  NOT `Để sau` (the retired Ask Cmdr opt-in screen's "Not now", askCmdr.consent.decline) nor anything built on `sau` /
  `nhắc lại`: the countdown is deleted, not deferred, and the en `@key` forbids a postpone reading. The object
  `làm việc` is what keeps `Tiếp tục` from colliding with `queue.row.resume`'s bare `Tiếp tục` (Resume) — different
  surface, and the operations here are running, not paused. `high` (on the parts); `tentative` (on the whole label
  reading unambiguously as "you keep working" to a native ear).
- **"Still running" (heading over the operation rows): `Vẫn đang chạy`** · `Đang chạy` is `queue.row.status`'s Running
  verbatim, and `Vẫn đang chạy` already ships as the head of `fileOperations.transferProgress.backgroundedToast`
  (`Vẫn đang chạy ở chế độ nền.`). The heading and the rows below it now use the same words. `high`
  (catalog-consistent).
- **"half-written file": `tệp ghi dở`** (kept) · the shipped vi catalog already renders this exact English phrase in
  `settings.advanced.showStagingTempFiles.description` ("a crash can't leave a half-written file under a real name" →
  `sự cố không thể để lại tệp ghi dở dưới một tên thật`), so the dialog matches it rather than coining a second form.
  The termbase's `đã được ghi một phần` (`stallInFlight`) stays for the predicative "may already be partly written";
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
  already neutral. `đang được ghi` is the same `được` + write-verb passive the termbase settled in
  `đã được ghi một phần`. ⚠️ **Never open this clause with `Chỉ mục …`**: `chỉ mục` is this locale's word for an
  INDEX, so `Chỉ mục đang được ghi` would read "the index being written". `high`.
- **"Whatever's finished stays done": `Những gì đã xong vẫn được giữ nguyên.`** · `Xong` is `queue.row.status`'s Done,
  and `giữ nguyên` is the catalog's keep-as-is verb (`Đã giữ nguyên tên gốc`, `Cmdr giữ nguyên tên hiện tại`). NOT
  `Mọi thứ`, which scopes to the whole app (the trap the 2026-08-09 conflict-prompt pass recorded for "Everything else
  is paused"). `high`.
- **"Quitting in {secondsText} seconds": `Sẽ thoát sau {secondsText} giây`** · subject-drop under a sentence-initial
  `Sẽ` is the catalog's own shape for a future statement about Cmdr (`servers.paneState.retryKeepsTrying` =
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

## Usage stats: bỏ "ẩn danh", nêu rõ "một mã định danh ngẫu nhiên" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, `onboarding.stepBeta.emailNote`)

English dropped "anonymous" (the stats carry a stable per-install random id, so they were never anonymous) and now says
plainly what they're tied to. The English stays deliberately everyday, so ❌ never `bút danh` / `giả danh` — that jargon
is exactly what the copy avoids.

- **usage stats → `thống kê sử dụng`** · the settings label's existing term; only the `ẩn danh` adjective was cut ·
  high. Every key that names the stats uses it, `onboarding.stepBeta.emailNote` included; ❌ not `số liệu sử dụng`.
- **a random id → `một mã định danh ngẫu nhiên`** · MS terminology (random → `ngẫu nhiên`, identifier → `mã định danh`)
  · high. ❌ Not a bare `mã ngẫu nhiên`: `mã` alone reads as a code (a coupon, a PIN), which loses the identifier sense.
- **tied to → `gắn với`** · plain everyday Vietnamese for the relation; `liên kết với` (used in
  `onboarding.stepBeta.emailNote`) is the heavier, more technical register and is kept for the linking sense · high

## Câu hỏi làm dừng một hàng trong hàng đợi + hộp thoại hoàn tác (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

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
  can be any dialog. "bring this one up" → `hiện thao tác này lên`, the phrasal `hiện … lên` (bring up) rather than the
  standalone Show verb, which is `Hiển thị` on the row's button (`queue.row.foreground`) · high

## Đổi tên liên tiếp: thông báo gộp khi nhiều tệp giữ nguyên tên (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

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

## Đổi tên không xác nhận được + tên không dùng được (`fileExplorer.rename.unconfirmed`/`.unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

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

## Thao tác được đề xuất: hộp thoại cho những gì Ask Cmdr đề xuất (`suggestedOps.*`, `commands.suggestedOpsShow.*`, `askCmdr.decision.*`)

- ops (các thao tác tệp do tác nhân đề xuất) → `thao tác`; tiêu đề là `Thao tác được đề xuất` · theo thuật ngữ sẵn có
  ("File operations" → "Thao tác tệp") · high
- approve → `Phê duyệt` · chuẩn; chọn thay cho `Chấp nhận` của macOS, vốn dành cho việc nhận tệp qua AirDrop · high
- reject → `Từ chối` · macOS Finder, cặp Chấp nhận/Từ chối trong bảng AirDrop (Tier 1) · high
- "This can't be undone" → `Bạn không thể hoàn tác việc này` · macOS Finder ("Bạn không thể hoàn tác tác vụ này") · high
- pattern → `mẫu` · đã có trong `queryUi.json` · high

## Nhân bản: lệnh sao chép ngay trong cùng thư mục (`commands.fileDuplicate.*`)

- **duplicate (lệnh sao chép mục đã chọn ngay trong thư mục của nó) → `Nhân bản`** · macOS Finder `vi`, menu "Tệp > Nhân
  bản" (`N154`), cùng "Nhân bản các mục" và "Nhân bản các mục trong vị trí hiện tại của chúng" (kiểm chứng trên macOS
  26.6.1, `Finder.app/Contents/Resources/vi.lproj`, 2026-08-19) · high. Không trùng với `Sao chép` (F5) hay `Di chuyển`
  (F6).
- **"Make a copy of the selected files in the same folder" → `Tạo bản sao của các tệp đã chọn trong cùng thư mục`** ·
  theo các mô tả lân cận ("Sao chép các tệp đã chọn…"); `bản sao` là từ đã chốt cho "copy" (danh từ), và "cùng thư mục"
  là thư mục các tệp đang nằm sẵn · high.

## Menu gốc: thanh menu, menu chuột phải, tiêu đề cửa sổ (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

Nguồn cho cả nhóm này: macOS 26.5.2 Finder (`Finder.app/Contents/Resources/vi.lproj`, `MenuBar.strings` +
`LocalizableMerged.strings`) là Tier 1 và quyết định gần như mọi thứ; phía tiếng Anh đọc từ `en_GB.lproj`, vì
`Base.lproj` chỉ chứa nib đã biên dịch. Safari 26 (`MainMenu.strings`) cung cấp từ vựng về tab, còn thuật ngữ Microsoft
bù vào chỗ Apple không đặt tên. Họ RAW: **dấu nháy đơn**, một `''` sẽ hiện thành hai dấu trên menu.

- **Thanh menu → `Tệp`, `Sửa`, `Xem`, `Đi`, `Cửa sổ`, `Trợ giúp`, `Dịch vụ`** · macOS Finder và Safari `vi` · high.
- **tab (thẻ giao diện) → `tab`, không phải `thẻ`.** macOS Finder `vi` viết „Tab mới”, „Hiển thị Tất cả Tab”, và
  Safari `vi` viết „Tab mới”, „Đóng tab”, „Ghim tab” (kiểm chứng trên macOS 26.5.2, 2026-08-19). `thẻ` là thẻ Finder
  (tag). Bằng chứng và đợt quét toàn catalog: § Rà soát trôi thuật ngữ toàn catalog.
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
- **pin / unpin tab → `Ghim tab` / `Bỏ ghim tab`** · Safari `vi` („Ghim tab”) · high.
- **Màu nhãn Finder → `Đỏ, Cam, Vàng, Lục, Lam, Tía, Xám`** · macOS Finder (`TG_COLOR_*`) · high.
- **Hàng thẻ (`menu.tag.rowLabel`, `menu.tag.addNamed`, `menu.tag.removeNamed`) → `Thẻ`, `Thêm “{color}”`,
  `Gỡ bỏ “{color}”`** · macOS Finder (`TG5` = `Thêm “^0”`, `N169.37` = `Thẻ`) · high. `TG6` là `Xóa “^0”`, nhưng dùng
  `Gỡ bỏ`: gỡ thẻ không xóa gì cả, và catalog đã nói vậy (`commands.tagsToggleRed.description` “Thêm hoặc gỡ bỏ thẻ”),
  đúng quy tắc `remove → gỡ bỏ`.
- **busy (ổ đĩa đang được dùng) → `(đang bận)`** · thuật ngữ Microsoft (`bận`) · high.
- **Eject → `Tháo`, Disconnect → `Ngắt kết nối`, Remove (khỏi một danh sách) → `Gỡ bỏ`** · macOS Finder và Thunar `vi` ·
  high. `Gỡ bỏ` tránh nghe giống `Xóa` (xóa tệp).
- **Giống hệt tiếng Anh có chủ đích** (`sameAsSourceJustification`): `menu.zoom.percent*` và `menu.view.askCmdr`.

## Thông báo dự phòng khi phải dùng kết nối SMB của macOS (`fileExplorer.network.osMountFallback.*`)

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

## Lỗi khi đổi tên / tạo mới (`errors.mutation.*`, `errors.volume.*`)

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
  `commands.fileGetInfo.mac.label` mà catalog đã ship (`Lấy thông tin`) · high. Mọi khóa `errors.*` nhắc tới bảng này
  đều dùng `Lấy thông tin` (xem mục ngay dưới về `Get Info` và `Locked`).
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

## Lỗi khi chuyển vào Thùng rác (`errors.mutation.trash*`)

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

## Ba biến thể phần thân của hộp thoại báo cáo sự cố (`crashReporter.dialog.body.*`)

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

## `thoát bất ngờ`, `ở chế độ nền`, và hai hướng đã loại (`crashReporter.dialog.body.ended`, `crashReporter.dialog.privacyNote`, `crashReporter.dialog.title*`)

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

## Mô tả cài đặt gửi báo cáo giờ đúng cho cả hai kết cục (`settings.updates.crashReports.description`)

Công tắc này vẫn gửi báo cáo khi một panic ở chế độ nền KHÔNG làm ứng dụng thoát, nên phần trợ giúp không thể chỉ nói
tới việc Cmdr thoát nữa. Mọi thành phần lấy từ mục hộp thoại báo cáo sự cố ở trên:

- **`khi Cmdr thoát bất ngờ`** lấy từ `crashReporter.dialog.body.ended` (đã theo Apple, `AppKitErrors.json`) · high.
- **`gặp sự cố ở chế độ nền`** lấy từ `.keptRunning` · high. Tiếng Việt không chia thì, nên câu ở đây dùng đúng động từ
  ấy, không cần đổi gì.
- **`báo cáo` trần**, không phải `báo cáo sự cố`, vì câu bao cả hai kết cục — đúng phép cắt như `.title.report` · high.
  ❌ NHÃN `settings.updates.crashReports.label` giữ nguyên `Gửi báo cáo sự cố`: đó là tên của cài đặt.
- **Câu thứ hai lấy từ `crashReporter.dialog.privacyNote`** (`phần mã nào đã gặp sự cố`), thay cho `vị trí sự cố`, vốn
  chỉ đúng khi ứng dụng thật sự đã thoát · high.

## Lỗi khi tháo ổ đĩa / ngắt kết nối (`errors.eject.*`)

Chín khóa mới, tất cả đều rơi vào thông báo nhỏ ở góc trên bên phải, sau dấu hai chấm của
`fileExplorer.pane.ejectFailedToast` (`Không thể tháo {volumeName}: …`) hoặc `.disconnectFailedToast`
(`Không thể ngắt kết nối: …`). Vì vế đầu đã nói "không thể", vế sau chỉ nêu lý do và bước tiếp theo, không lặp lại lời
than. Dùng lại từ đã chốt: eject → `tháo`, disconnect → `ngắt kết nối`, drive/volume → `ổ đĩa`, device → `thiết bị`,
share → `mục chia sẻ`, `thao tác` cho hạng mục thao tác tệp.

- **"in use" (ổ đĩa đang bị chiếm) → `đang sử dụng`** · macOS Finder Tier 1, đúng ngay ngữ cảnh tháo ổ đĩa: `NE66`
  ("Không thể tháo ổ đĩa vì ổ đĩa đang được sử dụng."), `NE31`, `NE79`, `NE80`, kiểm chứng trong
  `_ignored/i18n/vi/macOS/`, 2026-08-23 · high. `errors.eject.unmountRefused` giữ chủ ngữ chung chung `thứ gì đó` (giống
  `errors.volume.alreadyExists`) vì bản tiếng Anh cố tình không đoán đó là ứng dụng nào.
- **"once that finishes" → `khi việc đó hoàn tất`** · macOS Finder `LA17` ("Thử lại khi tác vụ hiện tại đã hoàn tất.") ·
  high.
- **"removable" (ổ đĩa tháo rời được) → `có thể tháo`** · macOS Finder Tier 1: `KIND_FORMATTER_28_1` "Removable" →
  `Có thể tháo`, `KIND_FORMATTER_28_0`/`GV3.1` "Removable Volume" → `Ổ đĩa có thể tháo`, kiểm chứng 2026-08-23 · high.
  Câu phủ định lấy khung LOẠI ổ đĩa (`Ổ đĩa này không phải loại có thể tháo`) chứ không nói `không tháo được`: cách sau
  nghe như macOS vừa từ chối một lần, trong khi khóa này nói ổ đĩa vốn không bao giờ tháo được. Thunar `vi` dịch
  "Removable Drive" là `Đĩa di động`, nhưng Tier 1 thắng.
- **"moving files" (nghĩa rộng: sao chép, di chuyển, hoặc xóa) → `chuyển tệp`** · ❌ đừng dùng `di chuyển tệp`:
  `di chuyển` là từ đã chốt riêng cho thao tác Move (`fileOperations.transferDialog.toggleMove`), nên nó sẽ thu hẹp câu
  xuống đúng một thao tác trong khi `@key.description` nói cả ba. `chuyển` trần là động từ chung mà catalog đã dùng
  (`Ghi đè và chuyển tệp cũ vào thùng rác`, `macOS đã từ chối chuyển mục này…`) · high (nhất quán catalog).
- **"wouldn't close its connection" → `đã từ chối đóng kết nối`** · `từ chối` là từ Apple dùng cho việc khước từ (xem
  đợt `errors.mutation.trashRefused`), và ở đây chủ ngữ là thiết bị · high.
- **"unplug" → `rút … ra`** · catalog đã ship `rút` ở bốn chỗ MTP (`errors.listing.deviceReconnecting.suggestion` "Bạn
  không cần rút thiết bị ra", `errors.provider.macDroid.*` "Rút rồi cắm lại cáp USB", `mtp.permissionDialog.helpText`) ·
  high (nhất quán catalog). Kho tham chiếu KHÔNG có chuỗi "unplug" nào, nên bằng chứng duy nhất là chính catalog. Lưu ý
  cái bẫy ngược ở `errors.volume.deviceSessionReset`: ở đó ❌ cấm dùng `rút`; ở khóa này bản tiếng Anh bảo rút thật nên
  `rút` mới đúng.
- **"once it's idle" → `khi không còn bận`** · kho tham chiếu không có "idle" cho thiết bị (`indexing.enrich.pausedIdle`
  dùng `rảnh` nhưng nói về NGƯỜI DÙNG rảnh, không dùng lại được cho máy). Phủ định `bận` (thuật ngữ Microsoft, đã chốt ở
  `menu.volume.ejectBusy` → `(đang bận)`) là cách nói tự nhiên nhất · `tentative`.
- **`errors.eject.timedOut` KHÔNG phải là thất bại**, giống hệt `errors.mutation.timedOut`: việc tháo chưa bị hủy và vẫn
  có thể xong. Vế đầu dùng lại nguyên văn vế đầu của khóa anh em (`Ổ đĩa vẫn chưa phản hồi`), vế sau đổi kết quả:
  `nên việc tháo vẫn có thể tự hoàn tất.` · high (nhất quán catalog).
- **`errors.eject.unexpected` có bản tiếng Anh GIỐNG HỆT `errors.mutation.unexpected`**, nên dùng chung một bản dịch
  nguyên văn: `Có gì đó không ổn, và Cmdr không rõ là gì.` · high.
- **`errors.eject.volumeNotFound` đi theo khung của `errors.mutation.volumeGone`** (`Ổ đĩa đó không còn … nữa, nên …`),
  chỉ đổi `khả dụng` thành `kết nối` vì bản tiếng Anh nói "isn't connected any more" · high.

### Apple CÓ dịch "Get Info" và "Locked" sang tiếng Việt (`errors.write.fileLocked.suggestion.mac`, `errors.write.permissionDenied.suggestion.deleteMac`)

Hai khóa cũ (`errors.write.fileLocked.suggestion.mac`, `errors.write.permissionDenied.suggestion.deleteMac`) để nguyên
`Get Info` và `Locked` bằng tiếng Anh giữa câu tiếng Việt. Đó là lỗi: macOS `vi` dịch cả hai, nên người dùng mở Finder
ra sẽ không thấy chữ nào khớp.

- **Get Info → `Lấy thông tin`** · macOS Finder Tier 1: `N165`, `TL22`, và khóa `"Get Info"` trong `Localizable.json`
  đều là `Lấy thông tin`, kiểm chứng trong `_ignored/i18n/vi/macOS/Finder/`, 2026-08-23 · high.
- **Locked (ô đánh dấu trong bảng Lấy thông tin) → `Đã khóa`** · macOS Finder Tier 1: `AXNODE1` (tên trợ năng của chính
  ô đánh dấu) là `Đã khóa`; `NE18` dựng nguyên câu tương đương của chúng ta —
  `Chọn Tệp > Lấy thông tin, bỏ chọn “Đã khóa” rồi thử lại.` — và `NE43`, `BN43` lặp lại cùng cách viết, kiểm chứng
  2026-08-23 · high.
- Giữ dấu nháy cong `“…”` quanh `Đã khóa` đúng như Apple viết. Ghi chú "dùng nháy thẳng ASCII" ở đợt trước chỉ áp dụng
  cho dấu nháy quanh `{path}`/`{name}`, không áp dụng cho tên một thành phần giao diện được trích dẫn.
- **`@key.description` bên `en` đã được sửa** (2026-08-24): bốn mô tả từng ghi "do NOT translate" nay nói rõ Apple có
  dịch `Get Info`, `Locked` và `Sharing & Permissions`, nên hãy dùng cách viết trong Finder của ngôn ngữ mình.

## `Get Info` và `Sharing & Permissions` (`errors.listing.noPermissionErrno.suggestion`, `errors.listing.permissionDenied.suggestion`)

`errors.listing.noPermissionErrno.suggestion` và `errors.listing.permissionDenied.suggestion` là ổ tiếng Anh còn sót lại
sau đợt trên; chúng viết `chọn Get Info, và xem mục Sharing & Permissions`. Nay là
`chọn Lấy thông tin, và xem phần Chia sẻ & quyền`.

- **Sharing & Permissions → `Chia sẻ & quyền`** · macOS Finder Tier 1, đối chiếu trực tiếp theo khóa:
  `InfoWindowPermissionsView.strings` `6.title` (en_GB `Sharing & Permissions:` → vi `Chia sẻ & quyền:`) và trong câu
  chạy `LocalizableMerged` `N30`/`N32`/`NE43` ("check the Sharing & Permissions section" →
  `kiểm tra phần Chia sẻ & quyền`). Kiểm chứng macOS 26.5.2 (`sw_vers`), 2026-08-24 · high. Giữ nguyên dấu `&` như Apple
  viết.
- **`phần`, không phải `mục`** · Apple dùng `phần` cho đúng chỗ này trong đúng câu này (`N30`/`N32`/`NE43`); `mục` là từ
  catalog dùng cho một hạng mục trong danh sách · high.

## Toast thùng rác: hoàn tác và đi tới thùng rác (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

Toast hiện ngay sau khi tệp được chuyển vào thùng rác, với hai nút `Hoàn tác` và `Đi tới thùng rác`, cùng lệnh cùng tên
trong bảng lệnh. Dùng lại `thùng rác`, `ổ đĩa`, `tệp`, `mục`. Các quyết định mới:

- **put back (đưa ra khỏi thùng rác, về chỗ cũ) → `đưa trở lại`** · macOS Finder `vi` Tier 1: `N153.1` (`Put Back` →
  "Đưa trở lại"), và cả câu `PE130_V1`/`PE130_V2` ("Không thể đưa trở lại “^1”", "Không thể đưa trở lại ^0 mục"), kiểm
  chứng trong pile 2026-08-27 · `high`. Đây đúng là mục menu của chính Finder cho cùng thao tác, nên Tier 1 thắng. ❌
  Không dùng `đặt lại`: catalog đã dành từ đó cho việc trả lại TÊN cũ (`askCmdr.renameUndo.*`), còn ở đây tệp thật sự di
  chuyển về chỗ cũ. ❌ Không dùng `khôi phục`/`lấy lại` của Nautilus `vi`: đó là "Restore", một khái niệm rộng hơn, và
  Nautilus là Tier 3.
- **undo (nút trên toast) → `Hoàn tác`** · macOS `vi` `ME13`/AppKit (`Undo` → "Hoàn tác"), và catalog đã có
  `askCmdr.renameUndo.undo` = `Hoàn tác` · `high`. Không đụng với `Hủy` (Cancel).
- **go to trash → `Đi tới thùng rác`** · họ `Đi tới` của catalog (`commands.navGoToPath.label`,
  `commands.downloadsGoToLatest.label`) và macOS `vi` "Đi tới thư mục chính" (`TL_HELP_HOME`) · `high`. ⚠️ 16 ký tự so
  với 11 của tiếng Anh; toast hẹp nên hãy kiểm tra tràn chữ cho cặp `Hoàn tác` / `Đi tới thùng rác`.
- **"stayed in the trash" → `{skippedText} {skipped, plural, other {mục}} vẫn ở trong thùng rác`** · `mục` là từ đếm
  macOS `vi` dùng cho item ("^0 mục", `PE130_V2`); `vẫn` là từ catalog đã dùng cho trạng thái còn nguyên · `high`.
  `{skipped}` là số nguyên đi kèm `{skippedText}`, nhưng tiếng Việt không biến đổi theo số nên chỉ cần một nhánh
  `other`.
- **"the drive you're browsing" → `ổ đĩa bạn đang duyệt`** · `askCmdr.empty.hint` của catalog ("những gì bạn đang
  duyệt") · `high`.
- **"This drive doesn't keep a trash." → `Ổ đĩa này không có thùng rác.`** · cùng khung với câu chị em
  `fileOperations.delete.archiveWarningStrong` ("Không có thùng rác bên trong tệp nén.") · `high`. Là một câu nói về ổ
  đĩa, không phải lời trách người dùng.

## Thêm ghi chú vào báo cáo đã gửi (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

Cmdr tự gửi báo cáo (người dùng đã chọn bật), rồi toast "Đã gửi báo cáo sự cố" cho một nút mở hộp thoại xem lại đúng
những gì đã gửi và viết thêm ghi chú **vào chính báo cáo đó**; không có lần tải lên thứ hai. Mười một khóa dùng lại
`báo cáo` / `báo cáo sự cố`, `ghi chú`, `ID tham chiếu`, `nhóm`, `đính kèm`, `email` đã có. Quyết định mới:

- **add / add to X → `thêm` / `Thêm vào X`** · macOS `vi` Tier 1: `Thêm`, `Thêm vào Dock`, `Thêm vào Mục ưa thích`,
  `Thêm vào Thanh bên`; Xfce Thunar `vi` "Thêm vào bảng Địa điểm"; thuật ngữ Microsoft ("add" → `thêm`). Kiểm chứng
  trong pile 2026-08-28 · `high`. Cả họ dùng chung một gốc từ: tiêu đề `Thêm vào báo cáo sự cố của bạn`, nút
  `Thêm vào báo cáo`, trạng thái `Đang thêm…`, toast `Đã thêm ghi chú vào báo cáo`.
- **"Couldn''t add your note. {reason}" → `Không thể thêm ghi chú của bạn. {reason}`** · đúng khuôn macOS `vi` "Không
  thể thêm máy chủ ^0 vào mục ưa thích của bạn"; câu dẫn đứng riêng và kết thúc bằng dấu chấm, rồi `{reason}` (một hoặc
  hai câu đã dịch sẵn) theo sau, cùng khuôn với `errorReporter.dialog.sendFailedToast` · `high`. Không có chữ
  "lỗi"/"thất bại", đúng yêu cầu của bản tiếng Anh.
- **"That report can''t take a note any more." → `Không thể thêm ghi chú vào báo cáo đó nữa.`** · `high`. Chọn
  `Không thể … nữa` (khuôn tự nhiên nhất) thay vì dịch sát "báo cáo đó không nhận được ghi chú": tiếng Việt đặt việc bất
  khả thi lên trước đọc xuôi hơn, và câu vẫn nói về báo cáo chứ không trách người dùng. Khuôn `không còn … nữa` của
  macOS (`bạn không còn có quyền … nữa`) cũng đúng nhưng dài hơn cho một dòng thay chỗ ô nhập.
- **"To get your notes to the team" → `Để nhóm nhận được ghi chú của bạn`** · `high`. Cố ý KHÔNG dùng `chuyển` (style
  guide dành `chuyển`/`di chuyển` cho thao tác Move) và tránh lặp `gửi … gửi` trong cùng một câu.
- **"from the Help menu" → `từ menu Trợ giúp`** · trùng khớp từng chữ với câu đã có trong catalog
  (`settings.updates.errorReports.description`: "Bạn luôn có thể gửi báo cáo thủ công từ menu Trợ giúp") và với
  `menu.bar.help` = `Trợ giúp` (macOS Finder `vi`) · `high`. ❌ Không dịch "Help" thành `Giúp đỡ`/`Hỗ trợ`.
- **"What was sent" → `Những gì đã được gửi`** · cặp song sinh với `errorReporter.dialog.detailsToggle`
  (`Những gì sắp được gửi`, "What''s about to be sent"): chỉ đổi `sắp` → `đã`, nên hai hộp thoại đọc như một · `high`.
- **"View or add notes to the report" → `Xem hoặc thêm ghi chú vào báo cáo`** · `xem` = macOS AppKit/thuật ngữ Microsoft
  ("view" → `xem`) · `high`. Giữ đủ **cả hai vế** (xem + thêm) như bản tiếng Anh David tự viết. 33 ký tự so với 31 của
  tiếng Anh, nên không nở thêm; ⚠️ vẫn kiểm tra tràn chữ vì nút đứng cạnh `Đổi cài đặt` trong một toast hẹp.
- **`báo cáo` trần trong hộp thoại, `báo cáo sự cố` ở tiêu đề** · đúng phép cắt đã ghi ở mục crash-report: tiếng Anh chỉ
  nói "error report" ở tiêu đề (`Add to your error report`), còn phần thân nói "this report" · `high`.
- **Bỏ một `của bạn` trong `amendedToast.message`** · tiếng Anh có hai ("your report", "Your reference ID"), nhưng lặp
  `của bạn` hai lần trong một dòng toast ngắn đọc rất nặng. Giữ `ID tham chiếu của bạn là` y hệt
  `errorReporter.sentToast.message` (hai toast hiện cùng một chỗ, phải khớp nhau), và bỏ ở vế `báo cáo`, nơi quyền sở
  hữu đã hiển nhiên · `high`.
- **"and it''ll join what the team already has" → `và nó sẽ được thêm vào chính báo cáo mà nhóm đã có`** · `tentative`.
  Kho tham chiếu không có câu nào tương đương; `chính … mà` là cách nêu bật "vẫn là báo cáo đó, không gửi cái thứ hai",
  đúng ý `@key.description`.

## Chọn và bỏ chọn: hộp thoại Select / Deselect (`selection.*`)

- **select → `chọn`, deselect → `bỏ chọn`** · macOS 26.6.2 Finder `vi`
  (`Finder.app/Contents/Resources/vi.lproj/MenuBar.strings`, `172.title` = `Chọn tất cả`, `300488.title` =
  `Bỏ chọn tất cả`; kiểm chứng 2026-08-29) · `high`. Catalog đã dùng đúng cặp này ở `menu.select.files` /
  `menu.select.deselectFiles` (`Chọn tệp…` / `Bỏ chọn tệp…`) VÀ ở `commands.selectionSelectFiles.label` /
  `commands.selectionDeselectFiles.label`, nên tiêu đề hộp thoại lấy nguyên: `Chọn tệp` / `Bỏ chọn tệp`. Không có chỗ
  nào lệch nhau trong bản `vi`.
- **focused pane → `khung đang hoạt động` trong hai tooltip này**, KHÔNG dùng `khung đang chọn` · `tentative`.
  `commands.navGoToPath.description` và `commands.favoritesAdd.description` đang dịch "focused pane" là
  `khung đang chọn`, nhưng đặt vào chính hộp thoại chọn tệp thì câu thành "Chọn các tệp này trong khung đang chọn": ba
  chữ `chọn` trong một dòng, và `đang chọn` đọc mơ hồ (khung đang chọn cái gì?). Hai tooltip nút chân hộp thoại song
  sinh (`search.action.showAll.tooltip`, `.goToFile.tooltip`) đã dùng `trong khung đang hoạt động`, nên lấy theo chúng:
  cùng loại chuỗi, cùng vị trí trên màn hình. ⚠️ Ghi lại cho lượt sau: nếu muốn thống nhất một cách dịch duy nhất cho
  "focused pane", `khung có tiêu điểm` là bản có nguồn Tier 1 (macOS Finder `vi` `300766.title` = "Đặt tiêu điểm vào
  trường tìm kiếm"; thuật ngữ Microsoft: focus → `tiêu điểm`), còn `khung đang chọn` thì không có nguồn nào.
- **Tooltip KHÔNG bắt buộc phải chứa nhãn** (`selection.action.*`). Tên trợ năng của nút lấy từ khóa nhãn
  (`QueryDialog.svelte`: `aria-label={config.primaryAction.ariaLabel ?? config.primaryAction.label}`), còn tooltip là
  một `use:tooltip` trên `span` bên trong, nên WCAG 2.5.3 đã thỏa mãn sẵn. Tiền lệ trong catalog cũng vậy:
  `search.action.showAll.label` (`Hiển thị tất cả trong cửa sổ chính`) và `.tooltip`
  (`Mở kết quả tìm kiếm trong khung đang hoạt động`) cố ý dùng chữ khác nhau. Tooltip chỉ cần gọi đúng thao tác của nhãn
  và nói rõ thay đổi rơi vào khung đang hoạt động.
- **Dù vậy hai tooltip vẫn mở đầu bằng nhãn**, vì đó là khuôn của hai khóa anh em gần nhất
  (`Mở tệp trong khung đang hoạt động`): `Chọn các tệp này` / `Bỏ chọn các tệp này` + ` trong khung đang hoạt động`.
  Tiếng Việt không biến hình nên câu đọc tự nhiên, không phải gò ép.
- **`selection.runHint` theo khuôn `search.runHint`**: `Nhấn Enter để lọc` (song sinh với `Nhấn Enter để tìm kiếm`). Tên
  phím giữ nguyên `Enter`, đúng quy ước `nhấn Enter` đã ghi ở mục trên.
- **`selection.recent.*` là bản sao của `queryUi.recent.*`**, chỉ đổi "tìm kiếm" thành `lựa chọn`:
  `Hiển thị tất cả lựa chọn gần đây`, `Tất cả lựa chọn gần đây`, `Lọc các lựa chọn gần đây`,
  `Không có lựa chọn gần đây nào khớp với bộ lọc đó.`, `Lựa chọn gần đây` (bản tiếng Anh cố ý để popover và listbox
  trùng nhau). `applyAria` theo khuôn `search.recent.runAria` (`Chạy lại tìm kiếm {mode} gần đây: {query}`):
  `Áp dụng lựa chọn {mode} gần đây: {query}`. `{query}` là chữ người dùng tự gõ nên đặt cuối câu, sau dấu hai chấm, để
  chứa được bất cứ thứ gì.

## Rà soát trôi thuật ngữ toàn catalog

Đợt rà soát theo `docs/guides/i18n-translation.md` § "Auditing a finished locale for term drift". Kiểm tra tự động
(`desktop-i18n-term-consistency`) đi từ **24 xuống 3**, và ba mục còn lại là ba ranh giới cố ý ghi ở cuối mục này. Phần
thủ công (ba lượt quét mà kiểm tra không thấy được, vì tiếng Anh khác nhau) mới là phần bắt được nhiều nhất: bảy đợt
quét toàn catalog bên dưới, tổng cộng 221 giá trị đã đổi.

Mọi bằng chứng Tier 1 trong mục này lấy từ máy Mac đang chạy, **macOS 26.6.2 (build 25G83), kiểm chứng 2026-08-30**:
`Finder.app/Contents/Resources/{vi,en_GB}.lproj/*.strings` (nhãn theo nib), `LocalizableMerged.strings` (chuỗi ngắn), và
các `*.loctable` của AppKit / các app hệ thống (macOS 26 đã chuyển phần lớn chuỗi ra `.loctable`, xem
`docs/i18n/reference-pile/how-to-mine.md`).

### Bảy đợt quét toàn catalog

- **size → `kích cỡ`, KHÔNG BAO GIỜ `kích thước`** (52 chuỗi) · macOS `vi` dùng `kích cỡ` **33 lần** và `kích thước` **0
  lần** trong toàn bộ Finder + AppKit + System Settings. Đúng ngay bề mặt của Cmdr: tiêu đề cột danh sách
  (`ListView.strings 54.headerCell.title` → `Kích cỡ`), `Sort By > Size` (`MenuBar 300891.title`), `Get Info`
  (`InfoWindowGeneralView evJ-mS-qpx.title` → `Kích cỡ:`), `LocalizableMerged N223`. Thuật ngữ Microsoft đồng ý (`size`
  → `kích cỡ`, id=1413833); Nautilus và Thunar dịch "Size" là `Kích cỡ`, chỉ Dolphin nói `Kích thước` · `high`. Trước
  đợt này catalog chia 52/18 nghiêng về `kích thước`, và cột `Size` của khung tệp (`Kích cỡ`) mâu thuẫn ngay với cột
  `Size` của bảng kết quả tìm kiếm (`Kích thước`). Ngoại lệ duy nhất giữ nguyên: **`Cỡ chữ`** cho "Text size" (cùng gốc
  `cỡ`, đã dùng nhất quán ở `settings.appearance.textSize.label` và `settings.summary.zoomAndDensity`).
- **show → `hiển thị`, KHÔNG `hiện`** (41 chuỗi) · macOS `vi` viết `Hiển thị` **147 lần** và **không lần nào** dùng
  `hiện` làm động từ "show": mọi `hiện` trong kho đều là `hiện tại` / `hiện có` (= "current") hoặc đuôi của `thực hiện`
  / `xuất hiện`. Bằng chứng đúng chuỗi: "Show in Finder" và "Reveal in Finder" đều là `Hiển thị trong Finder` (Photos
  `IPXMain.loctable`, Music `MainMenu.loctable`), "Show" trần là `Hiển thị` (AppKit `Common.loctable`), "Show Package
  Contents" là `Hiển thị nội dung gói` (`MenuBar 300771.title`) · `high`. Đây cũng là lý do ngữ nghĩa: một nhãn mở đầu
  bằng `Hiện` đọc thành "hiện tại…". `Show less` giữ `Thu gọn` (cặp co/giãn quen thuộc), và "show up here" / "the
  one-time hint" dùng `xuất hiện` vì tiếng Anh nói "appear", không phải "display".
- **download → `tải về`, KHÔNG `tải xuống`** (17 chuỗi) · macOS `vi`: `tải về` **35 lần**, `tải xuống` **0 lần**, cả ở
  nghĩa động từ (`Đang tải về`) lẫn danh từ (`Xóa bản tải về`, `Bản tải về Không có tiêu đề`). Thuật ngữ Microsoft nói
  `nội dung tải xuống`, nhưng đó là quy ước Windows và Cmdr là ứng dụng macOS (nguyên tắc "macOS thắng Microsoft" trong
  `docs/guides/i18n-translation.md`) · `high`. Một mục đã tải về là **`bản tải về`**, theo đúng `Xóa bản tải về` của
  Finder.
- **search → `tìm kiếm`; find → `tìm`** (15 chuỗi) · AppKit `vi` chia đúng như vậy: `Search` → `Tìm kiếm`, `Searching` →
  `Tìm kiếm`, còn `Find` → `Tìm` (và Finder `MenuBar 300783.title` "Find" → `Tìm`) · `high`. Nên chỗ nào tiếng Anh viết
  "Search" thì tiếng Việt viết đủ `tìm kiếm`: `Tìm kiếm lệnh…`, `Tìm kiếm trong`, `Tìm kiếm ảnh theo mô tả`.
- **tab (thẻ giao diện) → `tab`; tag (thẻ Finder) → `thẻ`** (28 chuỗi) · đây là đợt quét mà `style.md` đã ghi nợ. Bằng
  chứng Tier 1: Finder `vi` viết `Hiển thị Tất cả Tab` (`MenuBar WhJ-Wn-z5P.title`) và `Ẩn Thanh Tab`
  (`Wup-0E-2Ap.title`), Safari `vi` viết `Tab mới` / `Đóng tab` / `Ghim tab`; trong khi menu thẻ của Finder là `Thẻ…`
  (`300949.title`) và `Thêm thẻ…` (`InfoWindowTaggingHeaderView POw-SX-zUH`) · `high`. Trước đợt này `thẻ` mang **ba**
  nghĩa trong cùng một catalog: tab, tag, và `thẻ nhớ SD` (trong `errors.listing.readOnly.suggestion`); giờ còn hai và
  hai nghĩa đó không bao giờ đứng cạnh nhau. `menu.bar.tab` trùng tiếng Anh nên đã kèm `sameAsSourceJustification`.
- **Màu: Blue → `Lam`, Green → `Lục`, Purple → `Tía`, Cyan → `Xanh ngọc`** (11 chuỗi) · macOS dùng **cùng một bộ tên**
  cho thẻ Finder và cho màu nhấn, nên ở đây không có ranh giới nào để giữ: `LocalizableMerged TG_COLOR_2/4/3` (thẻ
  Finder) và ba bảng màu của AppKit — `AccentColorNames.loctable`, `HighlightColorNames.loctable`,
  `IconAppearanceTintColorNames.loctable` — đều nói `Lam` / `Lục` / `Tía`; `Cyan` → `Xanh ngọc`
  (`NSColorPanelExtras.loctable`) · `high`. Catalog vốn đã khớp Tier 1 ở 5 trong 8 tên chung (`Đỏ`, `Cam`, `Vàng`,
  `Hồng`, `Nâu`), chỉ ba tên kia trôi sang cách nói đời thường `Xanh dương` / `Xanh lá` / `Tím`, thứ **không có mặt một
  lần nào** trong toàn kho tham chiếu tiếng Việt. Hệ quả nhìn thấy được: `menu.tag.blue` là `Lam` còn
  `commands.tagsToggleBlue.*` (cùng một thẻ Finder!) là `Xanh dương`. Thuật ngữ Microsoft nói `xanh lam` / `xanh lục` /
  `tím`, đã bỏ vì Tier 1 thắng. Bốn màu Cmdr có mà macOS không có (`Hổ phách`, `Xanh chanh`, `Xanh mòng két`, `Chàm`)
  giữ nguyên, không nguồn nào nói tới.
- **remove → `gỡ bỏ`; delete → `xóa`** (18 chuỗi) · một mục **rời khỏi danh sách mà vẫn còn nguyên** thì là `gỡ bỏ`
  (Thunar dịch `_Remove` trần là `Gỡ bỏ`; Nautilus có `Gỡ bỏ ổ đĩa một cách an toàn`, `Gỡ biểu tượng tự chọn`); còn khi
  dữ liệu thật sự mất đi thì là `xóa` · `high`. macOS gộp cả hai vào `Xóa` (`RN30`, `Xóa khỏi thanh bên`) vì Finder
  không bao giờ đặt hai lệnh này cạnh nhau — Cmdr thì có (menu ổ đĩa có cả `Xóa` lẫn `Remove`), nên phép chia này đáng
  giữ. Trước đợt này một English "Remove" duy nhất có ba bản dịch (`Xóa` / `Gỡ bỏ` / `Gỡ`), trong đó `Xóa` nằm đúng ở
  nút xác nhận gỡ máy chủ khỏi danh sách — nghe như đang xóa cái gì đó thật.
  - **Ngoại lệ có nguồn: "Remove download" → `Xóa bản tải về`** · Finder có đúng chuỗi này (`N153.4_V1`), và ở đây bản
    sao cục bộ thật sự bị xóa (bản trên đám mây vẫn còn), nên nó rơi vào vế `xóa` của chính quy tắc trên · `high`.
  - **Chỉ một dạng `gỡ bỏ`, kể cả khi có tân ngữ.** `Gỡ bỏ {folder} khỏi việc lập chỉ mục`, không phải `Gỡ {folder}…`:
    `settings.mediaIndex.chosenFolders.removeAria` phải chứa nguyên nhãn `Gỡ bỏ` của nút (WCAG 2.5.3), nên hai khóa này
    là một cặp và dạng ngắn `Gỡ` không dùng nữa. Văn xuôi trong `errors.*` vẫn dùng `gỡ bỏ` theo nghĩa "removal" chung.

### Các mục lệch trong danh sách tự động

- **"About Cmdr" → `Giới thiệu về Cmdr`** · Finder `MenuBar 300680.title` "About Finder" → `Giới thiệu về Finder` ·
  `high`. `licensing.about.srTitle` (tên trợ năng của hộp thoại Giới thiệu) thiếu chữ `về`, nên trình đọc màn hình gọi
  hộp thoại khác với mục menu mở ra nó.
- **"Go to X" → `Đi tới X`** · Finder `GotoWindow 1.title` "Go to Folder" → `Đi tới thư mục` · `high`. Catalog có sẵn 16
  chỗ `đi tới`, nên ba biến thể lạc (`Đến đường dẫn` ở tiêu đề + nút của chính hộp thoại `commands.navGoToPath` mở ra,
  `Mở thư mục chính` ở màn hình lỗi, `Đến tệp tải về mới nhất` ở cài đặt) đã về một mối. Nhãn trợ năng của ô nhập cũng
  theo: `Đường dẫn cần đi tới`.
- **"Go back" / "Go forward" (lịch sử duyệt) → `Trở lại` / `Tiếp theo`** · Finder `MenuBar 211.title` / `249.title` ·
  `high`. `commands.navBack.label` đang là `Quay lại` và `commands.navForward.label` đang là `Đi tới` — cái sau còn đụng
  thẳng vào họ "go to" ở trên. Bảng lệnh và thanh menu giờ gọi cùng một hành động bằng cùng một từ. Ranh giới với
  `Quay lại` ghi ở cuối mục này.
- **"Put back" → `đưa trở lại`** · Finder `LocalizableMerged N153.1` "Put Back" (đúng lệnh khôi phục từ Thùng rác) →
  `Đưa trở lại`, và `PE130_V1/V2` "could not be put back" → `Không thể đưa trở lại` · `high`.
  `fileOperations.trash.undone` vốn đã đúng; hai khóa `askCmdr.renameUndo.*` dùng `Đã đặt lại` nên đã đổi theo. Tiếng
  Anh cố ý dùng chung một câu cho cả hai, ta giữ y vậy.
- **"error report" → `báo cáo trục trặc`; "crash report" → `báo cáo sự cố`** (10 chuỗi) · mục "Bổ sung:
  `thoát bất ngờ`…" ở trên đã chốt `báo cáo sự cố` = crash report, và Tier 1 xác nhận: Problem Reporter `vi` dịch
  "Problem Report for %@" là `Báo cáo Sự cố cho %@` · `high`. Nhưng chín khóa `errorReporter.*` + menu + bảng lệnh vẫn
  gọi **error report** là `báo cáo sự cố`, tức là hai luồng báo cáo khác nhau của Cmdr mang y hệt một tên: menu Trợ giúp
  nói `Gửi báo cáo sự cố…` còn hộp thoại sự cố nói `Gửi báo cáo sự cố?`. Tệ hơn, `settings.updates` lại giữ đúng phép
  chia (`Gửi báo cáo sự cố` cho crash, `Tự động gửi báo cáo trục trặc` cho error), nên Cài đặt mâu thuẫn với menu. Giờ
  `trục trặc` dành riêng cho error report, và hai công tắc cạnh nhau trong Cài đặt lại phân biệt được.
- **"Unreachable" → `không tới được`** · không nguồn nào trong kho có từ "unreachable" tiếng Việt, nên chọn theo chính
  catalog: `errors.listing.networkUnreachable.title` và `.hostUnreachable.title` đã là `Không tới được mạng` /
  `Không tới được máy chủ` · `tentative`. Hai ứng viên kia đều đụng khái niệm khác: `không kết nối được` đụng hành động
  connect/disconnect (`kết nối` / `ngắt kết nối`), `không truy cập được` đụng quyền truy cập (`quyền truy cập`). Ba khóa
  đã theo: hai ô trạng thái và `fileExplorer.navigation.volumesStillUnreachable`.
- **"Low disk space" → `Sắp hết dung lượng đĩa`** · macOS `PhotosUI.loctable` "Low Disk Space" →
  `Sắp hết dung lượng ổ đĩa` · `high`. `Thiếu dung lượng đĩa` nói "không đủ", còn tiếng Anh (và macOS) nói "sắp cạn".
- **"Searching..." → `Đang tìm kiếm...`** · theo mục `search` ở trên; `viewer.search.searching` là chỗ duy nhất còn
  `Đang tìm...`.
- **"Refresh network hosts" → `Làm mới các máy chủ mạng`** · nhãn trợ năng của nút quét lại vốn dài hơn nhãn lệnh cùng
  nội dung; lấy theo `commands.networkRefresh.label`. "Refresh listing" cũng về `Làm mới danh sách tệp` thay cho
  `Tải lại…` (termbase: refresh → `làm mới`).
- **"Tab limit reached" → `Đã đạt giới hạn số tab`** · `số` giữ lại vì giới hạn là ở SỐ LƯỢNG tab; `giới hạn tab` trần
  đọc mơ hồ (giới hạn _của_ một tab?).
- **Hai câu đăng ký beta** · `onboarding.stepBeta.signup.*` và `settings.updates.email*` là hai bản của cùng một câu.
  Lấy bản onboarding: **`hộp thư đến`** (Tier 1 — Mail `vi` dịch "Inbox" là `Hộp thư đến`), và **`chung tay`** cho
  "helping out" (ấm hơn `giúp đỡ`, hợp giọng biết ơn của câu). Câu hỏng lấy `chưa đăng ký được cho bạn`, đặt `được` ngay
  sau động từ.
- **"Modified" (nghĩa ngày sửa) → `Đã sửa đổi`** · Finder `InfoWindowGeneralView fBB-wo-vu6.title` "Modified:" →
  `Đã sửa đổi:` và Nautilus dịch cột "Modified" là `Đã sửa đổi` · `high`. (Finder dịch "Date Modified" là
  `Ngày sửa đổi`, nhưng tiếng Anh của Cmdr chỉ có một chữ "Modified".) Bốn khóa `queryUi.*` dùng `Sửa đổi` đã theo cột
  của khung tệp.

### Ba ranh giới cố ý (cần mục allowlist, đừng "sửa" lại)

- **`Back` → `Trở lại` (menu Đi) VÀ `Quay lại` (nút lùi trong một luồng)** · macOS chia đúng như vậy: Finder
  `MenuBar 211.title` (Go > Back, tức lịch sử duyệt) là `Trở lại`, còn nút "Back" của các trợ lý cài đặt và bảng hệ
  thống là `Quay lại` (`Buddy.loctable` = Setup Assistant, `Localizable-AppleID`, `Localizable-iCloud`,
  `Localizable-Age-Attestation`, `GKDefaultPrivacyViewController_OSX`) · `high`. Quy tắc: **đi lùi trong LỊCH SỬ thư mục
  → `Trở lại`; quay về MÀN HÌNH hoặc bước trước trong một luồng → `Quay lại`.** Nên `menu.go.back` +
  `commands.navBack.label` + `fileExplorer.errorPane.goBack` là `Trở lại`, còn bốn nút của trình duyệt mạng và trình
  hướng dẫn nhập môn (`fileExplorer.network.back`, `.share.backArrow`, `fileExplorer.networkMount.back`,
  `onboarding.wizard.back`) là `Quay lại`, cùng với `Quay lại danh sách máy chủ` / `Quay lại trò chuyện`.
- **`Error` → `Sự cố` (ô trạng thái)** · `fileExplorer.network.browser.status.error` bảo "tránh chữ 'error' nếu ngôn ngữ
  có cách nói thân thiện hơn" · `high`. Không còn tiền tố chẩn đoán `Lỗi:` nào trong catalog: khi kiểm tra cập nhật
  không xong, Cmdr nói bằng câu hoàn chỉnh (`updates.failure.check`). Đúng luôn với quy tắc giọng văn ở `style.md`: đừng
  dùng `lỗi` làm nhãn trạng thái trần.
- **`Modified` → `Đã sửa đổi` (ngày sửa) VÀ `Đã thay đổi` (phím tắt người dùng đã đổi)** ·
  `shortcuts.section.filterModified` không nói về ngày: nó lọc ra những lệnh mà người dùng đã đổi phím tắt · `high`.
  Tiếng Anh dùng lại một chữ cho hai khái niệm; tiếng Việt tách ra thì rõ hơn, và gộp lại sẽ khiến bộ lọc phím tắt đọc
  như một bộ lọc theo ngày.

## Menu wording, System Settings panes, email placeholder (`menu.app.hideOthers`, `commands.appHideOthers.label`, `errors.git.*`, `errors.provider.*`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`, `askCmdr.renameUndo.undone`/`.partial`)

Fallout from four `en` self-inconsistency fixes. Evidence is macOS 26.6.2 (build 25G83), read live off the installed
bundles with the `.loctable` / `MenuBar.strings` recipes in `docs/i18n/reference-pile/how-to-mine.md`, 2026-08-30, plus
`vi/microsoft-terminology/VIETNAMESE.tbx` from the pile.

- **`Hide others` (app menu) → `Ẩn các mục khác`** · Tier 1, three independent bundles agree: Finder `MenuBar.strings`
  `300729.title`, TextEdit `Edit.loctable` `515.title`, Preview `MainMenu.loctable` `145.title`. ❌ Not
  `Ẩn các ứng dụng khác`, which the catalog shipped: it's a fair gloss of the English, but the user's own menu bar says
  `mục`, and this item must read exactly like the one macOS puts one row above it. Applies to both `menu.app.hideOthers`
  and `commands.appHideOthers.label`. · `confirmed`
- **`Show all` (app menu) → `Hiển thị tất cả`** · Finder `300730.title` and TextEdit `517.title`; already shipped,
  unchanged. Preview's `150.title` title-cases it (`Hiển thị Tất cả`), but Cmdr's menu bar is sentence case and two of
  three sources agree with that. · `high`
- **System Settings pane names macOS does localize** · `General` → `Cài đặt chung` (SystemSettings `GENERAL`),
  `Login Items & Extensions` → `Mục đăng nhập & Phần mở rộng` (`LoginItems.appex/Localizable.loctable`), `Apple Account`
  → `Tài khoản Apple` (`ClassKitSettings.loctable` `APPLE_ID`). The catalog left all three in English inside the
  provider errors; a Vietnamese-language Mac shows the Vietnamese names, so the English ones sent the user looking for
  panes that aren't there. · `confirmed`
- **System Settings panes via tokens in the git and provider errors** · the eight `errors.git.*` / `errors.provider.*`
  suggestions now carry `{system_settings}` / `{privacy_and_security}` / `{files_and_folders}`, the same
  runtime-resolved placeholders the `errors.listing.*` family already used. Never hand-translate them, and never attach
  a suffix to one; write `trong {system_settings}`. · `high`
- **Example email placeholder → `ban@example.com`** (all three of `settings.updates.emailPlaceholder`,
  `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`) · Microsoft Vietnamese both TRANSLATES and
  ASCII-folds the local part of a sample address: `someone@example.com` → `ai_do@example.com` and `user@example.com` →
  `nguoi_dung@example.com` (`VIETNAMESE.tbx`, entries `6613_114525_1684760` and `6613_114525_1939170`, both
  `geographicalUsage VNM`). Cmdr's English says `you@`, and `bạn` is this catalog's pronoun for "you", so the Vietnamese
  form is `ban@` under the same diacritic-folding convention Microsoft uses. `example.com` stays: RFC 2606 reserves it,
  so it can never be a real person's address. The old `sameAsSourceJustification` ("locale-neutral") is dropped, since
  the pile shows Vietnamese does localize these. · `high`
- **"Put the old names back on N files" → `Đã đặt lại tên cũ cho {countText} tệp.`** (`askCmdr.renameUndo.undone` /
  `.partial`) · English used to share one sentence with `fileOperations.trash.undone` and now names the OBJECT (the old
  name), so the old Vietnamese (`Đã đưa trở lại …`, the trash verb) said the wrong thing: nothing moves, only the name
  changes back. Reuses `đặt lại tên cũ`, already the family's verb in `askCmdr.renameUndo.undoing` ("Đang đặt lại tên
  cũ…") and `skipReason.failed.*` ("Cmdr không đặt lại được tên cũ"). `fileOperations.trash.undone` keeps
  `Đã đưa trở lại …`. One `other` branch, as Vietnamese has no plural split. · `high`
- **`settings.indexing.enabled.description`** · English switched "directory sizes" → "folder sizes"; Vietnamese says
  `thư mục` for both senses and already read `kích cỡ thư mục`, so this was a restamp only. · `high`

- **Two `errors.listing.*` pane paths joined the same pass** · `errors.listing.networkDown.suggestion` wrote `Network`
  and `errors.listing.diskFullErrno.suggestion` wrote `General > Storage` in English, which left `General` reading two
  different ways inside one file once the provider errors were localized. Now `Mạng` (System Settings
  `Localizable.loctable`, `SECTION_NETWORK`), `Cài đặt chung` (same table, `GENERAL_SECTION`), and `Dung lượng`
  (`StorageSettingsIntentsExtension.appex/AppIntents.loctable`, `SETTINGS_DEEPLINKS.ROOT.TITLE`), all on macOS 26.6.2
  (build 25G83), verified 2026-08-30. No restamp: `en` didn't change, the Vietnamese was just incomplete. · `high`

## Thao tác mới hoàn tác được một nửa: làm tiếp cho hết (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- **`Finish rolling back` → `Tiếp tục hoàn tác`** · giữ nguyên họ từ `hoàn tác` đã chốt (§ roll back / rollback) và lấy
  khuôn của macOS Finder `vi` cho một thao tác tệp còn dở: `Tiếp tục sao chép`, cùng câu “Đã phát hiện thấy bản sao một
  phần của thư mục “^0”. Bạn có muốn tiếp tục sao chép không?” (Tier 1, đúng tình huống của ta: một thao tác dở dang
  được làm tiếp) · `high`. `Tiếp tục X` chỉ có nghĩa làm tiếp cái đã bắt đầu, không bao giờ đọc thành bắt đầu một lượt
  mới.
- **`Tiếp tục` dùng lại có chủ ý, không phải lỡ trùng.** `queue.row.resume`, `queue.toolbar.resumeAll` và
  `licensing.dialog.continue` đều đang là `Tiếp tục`. Hai bề mặt khác nhau (cửa sổ hàng đợi và hộp thoại nhật ký thao
  tác), và cả hai nghĩa đều là “làm tiếp cái đang dở”, nên dùng chung là trung thực. `i18n-terms` chỉ cảnh báo khi MỘT
  chuỗi tiếng Anh có hai bản dịch, chứ không cảnh báo hai chuỗi tiếng Anh dùng chung một bản dịch.
- **❌ Không dùng `Hoàn tất việc hoàn tác`.** Khuôn của Finder `vi` cho việc làm nốt một thao tác tệp là `Hoàn tất` +
  danh động từ (`Hoàn tất sao chép`, `Đang hoàn tất nén`), nên xét nguồn thì nó hợp lệ. Nhưng `hoàn tất` và `hoàn tác`
  chỉ khác nhau một dấu, đứng cạnh nhau thành líu lưỡi và rất dễ đọc nhầm, nhất là khi `queue.row.status` đã có
  `Chưa hoàn tất được` ngay trong cùng tính năng. Vì thế chọn `Tiếp tục`.
- **Hai khóa `finishRollBack` phải giống nhau từng byte.** `operationLog.dialog.finishRollBack` và
  `fileOperations.rollbackConfirm.finishRollBack` cùng một chuỗi tiếng Anh và cùng một hành động (nút ở hàng mở đúng hộp
  thoại đó), nên `i18n-terms` sẽ cảnh báo nếu chúng lệch nhau. Sửa thì sửa cả hai.
- **`Finish rolling this back?` → `Tiếp tục hoàn tác thao tác này?`** · đúng khuôn câu hỏi của khóa anh em
  `fileOperations.rollbackConfirm.title` (`Hoàn tác thao tác này?`), chỉ thêm `Tiếp tục` ở đầu · `high`.
- **Nút `Tiếp tục hoàn tác` và nhãn `Đã hoàn tác` không đụng nhau.** Chúng không bao giờ nằm cùng một hàng: hàng đó hoặc
  mang nhãn `Đã hoàn tác một phần` kèm nút, hoặc mang `Đã hoàn tác` mà không có nút.
- **Dòng chú thích lấy lại chữ của `fileOperations.rollbackConfirm.bodyUndoByDeleting`** ·
  `Cmdr đã hoàn tác những gì có thể và để nguyên phần còn lại. Việc tiếp tục sẽ chạy thêm một lượt nữa và bỏ qua những gì Cmdr vẫn không chắc chắn.`
  `bỏ qua những gì … không chắc chắn` là nguyên văn của `bodyUndoByDeleting`, `để nguyên` giữ giọng của
  `rollbackConfirm.leaveAsIs` (`Cứ để nguyên`), còn `lượt` là từ catalog đã dùng cho một lượt chạy
  (`fileExplorer.navigation.driveIndex.queuedBehindScan` = `Lượt quét bạn yêu cầu`) · `high`. Câu này cố ý không hứa
  hoàn tác trọn vẹn.
- **`vẫn` đặt ở `không chắc chắn`, không đặt ở `bỏ qua`** · tiếng Anh nói “still isn’t sure about”, tức là cái chưa chắc
  thì vẫn chưa chắc, chứ không phải “vẫn bỏ qua” · `high`.
- **`in {folder}` → `trong {folder}`** · giới từ catalog vẫn dùng cho việc làm gì đó bên trong một thư mục
  (`fileOperations.operationConflict.context` = `Đang xử lý trong {destination}`,
  `fileExplorer.navigation.driveIndex.deferredRescan` = `Cmdr đang tìm kiếm trong {name}`) · `high`. Cả hàng đọc thành
  `Đang xóa những gì đã tạo trong Backup`, đúng ý cần: xóa thứ nằm TRONG thư mục đó, không phải xóa chính thư mục. Tiếng
  Việt không biến hình nên tên thư mục nào cũng vừa, và không cần thêm dấu ngoặc kép.

## Toast sau khi hoàn tác thao tác đang chạy (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

Người dùng bấm `Hoàn tác` trên một lần sao chép/di chuyển đang chạy, lượt hoàn tác chạy xong, và toast này báo nó làm
được tới đâu. Toast xếp tối đa ba phần: một dòng tiêu đề (`doneDeleting` / `doneMovingBack` / `someDeleted` /
`someMovedBack` / `stoppedDeleting` / `stoppedMovingBack`), rồi `leftBehind`, rồi danh sách gạch đầu dòng `reason.*`.
Giọng xuyên suốt: **Cmdr đã làm phần cẩn thận**, không xin lỗi, không báo động.

- **Cả bộ `reason.*` đi theo họ `Giữ nguyên` của `askCmdr.renameUndo.skipReason.*`** · tiếng Anh của hai bộ gần như
  trùng nhau ("Left {name} alone: …"), cùng một khái niệm (Cmdr từ chối động vào thứ nó không đối chiếu được), nên hai
  bộ phải đọc như một tính năng · `high`. Hai khóa `folderNotEmpty.*` có chuỗi tiếng Anh **giống hệt** bên `renameUndo`,
  nên giá trị vi dùng lại **nguyên văn**: `Giữ nguyên thư mục {name}: bây giờ trong đó đã có thứ gì.` và
  `Giữ nguyên {countText} {count, plural, other {thư mục}}: bây giờ trong đó đã có thứ gì.` Sửa thì sửa cả hai bộ.
- **`mục`, không phải `tệp`, trong bộ này** · tiếng Anh của `cancelRollback` đếm "item" (lượt hoàn tác xóa cả thư mục nó
  đã tạo), còn `renameUndo` đếm "file". Đây là khác biệt duy nhất giữa hai bộ, đừng san phẳng · `high`.
- **`Left X where it is` gộp vào `Giữ nguyên`, không tách riêng** · tiếng Anh đổi "alone" → "where it is" ở
  `spotTaken.*` để nhắc rằng mục vẫn nằm ở đích. `Giữ nguyên` trong tiếng Việt đã mang sẵn nghĩa "để yên tại chỗ", nên
  tách ra sẽ dựng lên một phân biệt tiếng Việt không cần; vế sau (`nơi nó xuất phát bây giờ đã có thứ khác`) đã nói rõ
  chỗ nào bị chiếm · `high`.
- **"something else now sits where it came from" → `nơi nó xuất phát bây giờ đã có thứ khác`** · `nơi … xuất phát` lấy
  nguyên của `fileOperations.rollbackConfirm.bodyUndoByMovingBack` (`đưa các tệp về lại nơi chúng xuất phát`), còn
  `đã có thứ khác` là cách nói của `renameUndo.skipReason.nameTaken` (`tên cũ đã có thứ khác dùng`). Finder `vi` có đúng
  tình huống này ở `PE1.1` (`Vị trí mà bạn đang khôi phục “^0” đến đã có một mục với cùng tên.`), nhưng tiếng Anh của
  Cmdr cố tình nói mơ hồ ("something else"), nên giữ `thứ khác` thay vì `một mục với cùng tên` · `high`.
- **"after Cmdr put it there" → `sau khi Cmdr đưa nó tới đó`** · động từ `đưa` là gốc chung của cả họ hoàn tác trong
  catalog (`đưa trở lại`, `Đang đưa các tệp về chỗ cũ`), và nó phủ được cả hai thao tác mà câu này dùng chung: bản sao
  thì Cmdr ghi tệp tới đó, bản di chuyển thì Cmdr mang tệp tới đó. ❌ Đừng viết `ghi nó vào đó`: chỉ đúng cho lượt sao
  chép · `high`.
- **`failed.*` cố ý ra khỏi họ `Giữ nguyên`** · năm lý do kia là lựa chọn có chủ ý, riêng lý do này thì không (ổ đĩa từ
  chối), nên tiếng Anh đổi khung câu và tiếng Việt cũng đổi: `Không hoàn tác được {name}.` Bỏ chủ ngữ đúng như tiếng
  Anh, và `không … được` giữ giọng nhẹ, tránh hẳn `lỗi`/`thất bại` · `high`. Vế sau lấy chữ có sẵn: `chưa được kết nối`
  (`fileOperations.trash.undoUnavailable`) và `chỉ đọc` (`errors.volume.readOnly` = `Ổ đĩa này là chỉ đọc.`), nên viết
  `Ổ đĩa của nó có thể chưa được kết nối hoặc là chỉ đọc.`
- **`leftBehind` mở bằng `bỏ qua`, đúng như các hộp thoại xác nhận** · `rollbackConfirm.body` và ba khóa `bodyUndo*` đều
  hứa `Cmdr bỏ qua những gì nó không chắc chắn`, và tiếng Anh dùng cùng một động từ ở cả hai bề mặt, nên tiếng Việt cũng
  vậy: một lời hứa thì một chữ · `high`. Trọn câu:
  `Cmdr bỏ qua những gì nó không chắc chắn, nên những mục sau vẫn ở nguyên chỗ:` ❌ Đừng dùng `giữ nguyên` ở đây: đó là
  chữ của các gạch đầu dòng bên dưới (`Giữ nguyên {name}: …`), còn dòng này phải nối lại với hộp thoại người dùng vừa
  đọc.
- **Mạo từ "trọn vẹn" của tiếng Anh → `cả`** · `doneDeleting`/`doneMovingBack` nói "the N items" để hàm ý tất cả, còn
  `someDeleted`/`someMovedBack` cố tình bỏ mạo từ vì còn sót lại. Tiếng Việt không có mạo từ, nên khác biệt đó nằm ở
  `cả`: `Đã đưa trở lại cả {countText} mục.` (trọn vẹn) so với `Đã đưa trở lại {countText} mục.` (một phần). Catalog đã
  dùng đúng chữ này cho "all N" ở `askCmdr.renameUndo.undoJob` (`Hoàn tác cả {countText} lô`) · `high`. ❌ Đừng thêm
  `tất cả`: dài hơn và đọc như một nút chọn hết.
- **"put back" (bộ move) → `đưa trở lại`**, đúng động từ Tier 1 của `fileOperations.trash.undone` · `high`. Nó cùng gốc
  `đưa` với tiêu đề đang chạy mà người dùng vừa nhìn (`Đang đưa các tệp về chỗ cũ`), nên hai màn hình khớp nhau.
- **"Stopped after …" → `Đã dừng sau khi …`** · `Dừng` là động từ dừng của macOS `vi` (Finder `Dừng xóa`, `Dừng nén`,
  `MT14`/`AR30`) · `high`. Đây là trần thuật một việc đã xảy ra nên dùng `Đã dừng`, khác với nhãn nút `Dừng lại` đã chốt
  cho tooltip hoàn tác. Không đụng `Đã hủy` (Cancel).
- **"The rest" → `Các mục còn lại`** · Finder `vi` có đúng cụm này (`bỏ qua chúng và sao chép các mục còn lại`), AppKit
  có `phần còn lại của câu` · `high`. `stoppedMovingBack` nói rõ chúng nằm đâu:
  `Các mục còn lại vẫn ở nơi thao tác di chuyển đã đưa chúng tới.`
- **`rollbackConfirm.body` viết lại** · tiếng Anh nay thêm lời hứa thứ ba, nên bản vi nối vế "ghi đè" vào câu đầu và
  mượn **nguyên văn** câu đuôi của `bodyUndoByDeleting`:
  `Cmdr bỏ qua những gì nó không chắc chắn, nên có thể còn sót lại vài thứ.` Bốn khóa `body*` là bốn biến thể của một
  hộp thoại, nên phần đuôi chung phải giống nhau từng chữ · `high`.
- Cả 18 giá trị đều khác tiếng Anh nên không cần `sameAsSourceJustification`; không giá trị nào chứa dấu nháy đơn nên
  không phát sinh `''` của ICU; mọi `{countText}` / `{count}` / `{name}` giữ nguyên, và mỗi `plural` chỉ có một nhánh
  `other`.

### `cancelRollback.stagedLeftover.*` (phần Cmdr tự để lại ở đích)

Thêm ngày 2026-09-02. Hai dòng về một tệp làm việc do chính Cmdr tạo ra và không xóa được khỏi đích. Chúng KHÔNG thuộc
danh sách `reason.*`: ở đó Cmdr bảo vệ tệp của người dùng, còn đây là phần thừa của chính Cmdr.

- **`unfinished copy` → `bản sao chưa hoàn chỉnh`** · `bản sao` là danh từ trong macOS `NE111` ("giữ lại bản sao có thể
  tiếp tục"), `hoàn chỉnh` là từ Apple dùng cho "complete" (macOS `LA33`: "không hoàn chỉnh"); dùng `chưa` thay cho
  `không` vì việc sao chép chỉ mới dừng giữa chừng · `high`
- **`at the destination` → `ở đích`** · từ của catalog (`conflictsUnknown`) · `high`
- **`transfer` (danh từ) → `lần truyền`** · catalog đã dùng (`errors.listing.deviceReconnecting.explanation`: "sau một
  lần truyền bị hủy hoặc bị gián đoạn") · `high`
- ⚠️ **`trong một lần truyền sau`, ❌ không bao giờ "lần tới".** Việc dọn dẹp của Cmdr bỏ qua mọi thứ chưa đủ một giờ,
  nên thử lại ngay sẽ không dọn được gì. Một lời hứa không giữ được chính là lỗi mà dòng này sinh ra để loại bỏ.

## Màn hình chặn khi WebKit quá cũ (`main.oldWebkit.*`)

Ba chuỗi Cmdr hiển thị thay cho giao diện khi Safari của máy Mac quá cũ. Chúng nằm trong lớp vỏ HTML chứ không nằm trong
app, nên đây là thứ duy nhất người dùng đó thấy được của Cmdr.

- **`Software Update` → `Cập nhật phần mềm`** · tên bảng trong Cài đặt hệ thống của macOS; dấu vết Tier 1 từ Finder xác
  nhận cụm từ (`Apple Device Software Update File` → `Tệp cập nhật phần mềm thiết bị Apple`) · `high`.
- **`Quit` → `Thoát`** · đã có trong termbase, khớp với macOS AppKit · `high`.
- **`Safari`, `Mac` và `15.4` giữ nguyên.** `Safari` nay nằm trong `BRAND_WORDS`.
- Dấu thanh đầy đủ ở mọi chữ, theo quyết định trong `style.md`.

## Thông báo về macOS cũ (`main.oldMacos.*`)

Hộp thoại hiện đúng một lần trên máy Mac thấp hơn macOS 12: Cmdr vẫn chạy, nhưng nằm ngoài dải đã kiểm thử. Giọng thành
thật và thoải mái, không xin lỗi và không cảnh báo, vì ứng dụng vẫn chạy được.

- **`supported` → `hỗ trợ`** · macOS Finder (`… vì thao tác không được hỗ trợ.`) · `high`.
- **`X and up` → `X trở lên`** · macOS SystemSettings (`… yêu cầu OS X %@ trở lên.`) · `high`.
- **`best effort` → `chỉ cố hết sức thôi`** · pile không có thuật ngữ này (chỉ có định nghĩa QoS mạng) · `high` cho cách
  diễn đạt. Cố ý không dịch sát.
- **`fixes` → `việc khắc phục`, không phải `sửa lỗi`** · `lỗi` thuộc thanh ghi mà giọng nói tránh; `khắc phục` nói đúng
  việc mà không mang chữ đó.
- **Câu cuối là David ở ngôi thứ nhất, dùng `mình`**, đúng như `onboarding.stepBeta.greeting`; người dùng vẫn là `bạn`.

## Ask Cmdr xem bên trong tệp: hai nhãn công cụ + hai khóa đồng ý (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

Năm khóa cho công cụ `inspect_file` (Ask Cmdr đọc một phần có giới hạn của tệp khi được hỏi) và màn hình đồng ý viết lại
quanh nó. Dùng lại các thuật ngữ đã chốt (tệp nén → `tệp nén`, văn bản → `văn bản`, dòng → `dòng`, ảnh → `ảnh`, thẻ →
`thẻ`, nhà cung cấp → `nhà cung cấp`, đề xuất → `đề xuất`, phê duyệt → `phê duyệt`). Hai câu cuối của `contentsRule`
(tìm kiếm ảnh; đề xuất chờ phê duyệt) lấy **nguyên văn** từ khóa cũ `askCmdr.consent.noContents`, và câu thứ hai của
đoạn giới thiệu tính năng mới đã gỡ (askCmdr.consent.whatsNew.body) giữ nguyên. Thuật ngữ mới:

- **thumbnail → `hình thu nhỏ`** · macOS AppKit `WindowTabs` ("thumbnail of the tab picker image" →
  `hình thu nhỏ của hình ảnh bộ chọn tab`), thuật ngữ Microsoft (`thumbnail` → `hình thu nhỏ`), KDE Dolphin và Xfce
  Thunar (`Thumbnails` → `Hình thu nhỏ`); chỉ GNOME Nautilus nói `ảnh thu nhỏ` · `high`. Khóa `noContents` cũ (nay đã
  bỏ) từng viết `ảnh thu nhỏ`, và không chỗ nào khác trong catalog dùng từ này, nên đổi theo phe macOS + MS không gây
  lệch. Thêm một lý do ngữ nghĩa: trong câu `toàn bộ tệp, ảnh hay hình thu nhỏ`, chữ `hình` tách "thumbnail" khỏi
  "photo" (`ảnh`) đứng ngay trước nó.
- **camera (của một bức ảnh) → `máy ảnh`; "camera details" → `thông tin máy ảnh`** · macOS AppKit
  (`NSStillCameraTemplate` → `máy ảnh`), thuật ngữ Microsoft (`camera` → `máy ảnh`), GNOME Nautilus (`Camera Brand` →
  `Nhãn hiệu máy ảnh`, `Camera Model` → `Kiểu máy ảnh`); catalog đã có `daemon máy ảnh của macOS` (`mtp.connectedToast`)
  · `high`. Xfce Thunar nói `Hiệu máy` (bỏ chữ ảnh) nhưng là thiểu số. "details" ở đây là thông tin EXIF, nên viết
  `thông tin` (cùng chữ với `Thông tin tệp, không phải nội dung` ở `askCmdr.renameReview.evidence.metadata`), không phải
  `chi tiết` (macOS `Hiển thị Chi tiết` là nút mở rộng một hộp thoại, nghĩa khác).
- **location / "where it was taken" (của ảnh) → `vị trí chụp`** · `vị trí` là "location" của macOS Finder (`Location` →
  `Vị trí`, `Get Location` → `Lấy vị trí`) và Microsoft (`location` → `vị trí`, nghĩa địa lý → `vị trí địa lý`); `chụp`
  (chụp ảnh) nói rõ đây là nơi bấm máy, không phải đường dẫn tệp (catalog dùng `vị trí` cho đường dẫn/thư mục ở
  `fileExplorer.navigation.locationUnreachableToast`, `commands.paneCopyPath*`) · `high`. Dùng cùng một cụm cho cả
  "location" (`item.contents` và đoạn tính năng mới đã gỡ) lẫn "including where it was taken" (`kể cả vị trí chụp` trong
  `contentsRule`), để ba khóa trên một màn hình gọi cùng một thứ bằng cùng một tên.
- **page (của PDF) → `trang`** · macOS AppKit `Printing` (`Page %ld` → `Trang %ld`, `No pages from the document` →
  `Chưa chọn trang nào`), thuật ngữ Microsoft (`page` → `trang`), KDE Dolphin (`Page Count` → `Số trang`) · `high`. "PDF
  pages" → `vài trang PDF`; "a few pages of a PDF" → `vài trang của một tệp PDF`. `PDF` giữ nguyên (tên chuẩn).
- **title and author (của PDF) → `tiêu đề và tác giả`** · title: macOS AppKit `Common` (`Title` → `Tiêu đề`), Microsoft
  (`Title` → `Tiêu đề`), Dolphin (`Tiêu đề`); catalog đã dùng `Tiêu đề trò chuyện`. author: Microsoft (`author` →
  `tác giả`, biến thể `người tạo` là nghĩa "creator"), Dolphin (`Author` → `Tác giả`); macOS không có chuỗi "author"
  trong kho · `high`.
- **"what's inside an archive" → `những gì bên trong một tệp nén`; "the list of files inside an archive" →
  `danh sách các tệp bên trong một tệp nén`** · `bên trong` là cách catalog đã nói "inside"
  (`Cmdr không đọc gì bên trong tệp này`, `văn bản bên trong hình ảnh`), và `những gì` là khung của khóa chị em
  `item.envelope` (`Những gì bạn đang xem ngay bây giờ`) · `high`.
- **"look inside files" (nhãn công cụ) → `Đang xem bên trong tệp` / `Đã xem bên trong tệp`** · cùng khuôn
  `Đang … / Đã …` với các nhãn chị em (`Đang liệt kê một thư mục`, `Đang đọc nội dung trong ảnh của bạn`), cùng chữ
  `bên trong` như trên; `tệp` trần vì tiếng Việt không đánh dấu số, nên nhãn trung tính cho một hay 200 tệp · `high`.
  Cùng gốc `xem bên trong` được dùng lại ở đoạn tính năng mới đã gỡ (`có thể xem bên trong tệp mà bạn hỏi đến`).
- **"a photo's …" → `… của một bức ảnh`** · catalog dùng `ảnh` trần cho "photo", nhưng `thông tin máy ảnh của ảnh` lặp
  chữ `ảnh` hai lần liền và đọc rối; loại từ `bức` là cách tiếng Việt chuẩn đếm một tấm ảnh, và Photos của Apple viết
  `Chọn ảnh` / `Cắt ảnh` (không loại từ) chỉ ở nhãn nút, không ở câu văn · `tentative` (không có nguồn kho cho loại từ;
  chỉ dùng khi "ảnh" đứng cạnh "máy ảnh"). Ở chỗ "photos" không đứng cạnh "máy ảnh" thì vẫn `ảnh` trần
  (`toàn bộ tệp, ảnh hay hình thu nhỏ`, `Tìm kiếm ảnh`, `các ảnh khớp`).
- **"whole files" → `toàn bộ tệp`** · catalog đã có `toàn bộ đĩa`, `toàn bộ đường dẫn`,
  `bỏ qua toàn bộ {skippedText} tệp` · `high`. Không dùng `chính các tệp` của khóa cũ: câu mới không còn hứa "không gửi
  nội dung tệp", nên `toàn bộ` (whole) là chữ mang đúng ranh giới mới.
- **"a limited part of it" → `một phần có giới hạn của tệp đó`** · `có giới hạn` đã có trong catalog
  (`một hệ thống tệp có giới hạn`) · `high`. "some lines of text" → `vài dòng văn bản`; "some text" / "some of its text"
  → `một ít văn bản` / `một phần văn bản của tệp` (không có nguồn kho cho lượng từ; chữ thường ngày) · `tentative`.
- **"works the same way" → `cũng hoạt động theo cách đó`** · `hoạt động` là "work/operate" của Microsoft; viết
  `cũng … theo cách đó` để nối vào câu trước · `tentative` (không có mẫu câu trong kho).
- Không giá trị nào chứa dấu nháy đơn ASCII nên không phát sinh `''` của ICU; không giá trị nào giống tiếng Anh nên
  không cần `sameAsSourceJustification`. Dấu phẩy trước `và` trong danh sách bỏ đi theo các khóa chị em
  (`thư mục hiện tại, con trỏ, mục đã chọn và các ổ đĩa`).
- **Hai câu "chỉ xem bên trong khi bạn hỏi" (`askCmdr.empty.hint`, `settings.askCmdr.intro`)** · câu đầu của cả hai giữ
  nguyên; câu thứ hai viết lại theo tiếng Anh mới: "looks inside a file only when you ask about it" →
  `chỉ xem bên trong một tệp khi bạn hỏi về tệp đó` (cùng gốc `xem bên trong` với nhãn công cụ ở trên), "never changes a
  file without your approval" → `không bao giờ thay đổi một tệp khi chưa được bạn phê duyệt` (`phê duyệt` là chữ của
  `contentsRule`) · `high`. ❌ Bỏ hẳn `chỉ đọc` / `không bao giờ đọc nội dung tệp` /
  `không bao giờ thay đổi bất cứ điều gì` của bản cũ: tiếng Anh mới không còn hứa những điều đó.

## Hai chú giải của nút Rollback (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

Bề mặt mới: chú giải của nút giờ nói rõ lần hoàn tác NÀY làm gì với các tệp, và nút bị tắt ngay khi một lần di chuyển
giữa hai hệ thống tệp bước sang chặng cuối (xóa các bản gốc, khi mọi thứ đã ở đích).

- **`rollbackTooltipStopAndMoveBack` → `Dừng lại và đưa trở lại mọi tệp đã di chuyển đến giờ`** · khung câu lấy từ khóa
  cùng cặp `rollbackTooltip` (`Dừng lại và …`), còn `đưa trở lại` là cách nói đã chốt cho việc về chỗ cũ
  (`cancelRollback.doneMovingBack`: „Đã đưa trở lại…”) · `high`. ❌ Không dùng `xóa`: hoàn tác một lần di chuyển không
  xóa gì.
- **`rollbackAlreadyLandedTooltip`** · câu đầu lặp lại hình ảnh của `cancelRollback.moveAlreadyLanded` („đều đã ở
  đích”), `hoàn tác` là từ đã chốt cho rollback (`rollbackUnavailableTooltip`), và `Hủy` là nhãn của nút bên cạnh
  (`fileOperations.button.cancel`) nên giữ nguyên · `high`.

## “Mở terminal tại đây” và bộ chọn ứng dụng (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

Bề mặt mới: một thẻ trong `Hành vi & thao tác tệp` (`Hành vi > Điều hướng & thao tác tệp`) để chọn ứng dụng terminal mà
lệnh sẽ mở. macOS dựng danh sách; ở đây chỉ dịch các nhãn.

- **terminal (loại ứng dụng) → `terminal`; Terminal (ứng dụng của Apple) → `Terminal`** · macOS tiếng Việt giữ nguyên
  tên tiếng Anh (`Mở trong Terminal`, khóa `N67` trong `macOS/Finder/LocalizableMerged.json`), và từ chung trong tiếng
  Việt cũng là từ mượn đó · `high`. Vì vậy tiêu đề thẻ `settings.navigationAndFileOps.card.terminal` mang
  `sameAsSourceJustification`: nó giống hệt tiếng Anh một cách có chủ ý.
- **Open terminal here (tên lệnh) → `Mở terminal tại đây`** · dựa trên `Mở trong Terminal` của Apple, thêm `tại đây` cho
  vị trí · `high`. Bản dịch của chính lệnh đó (menu, bảng lệnh) phải dùng đúng dạng này.
- **Choose an app… → `Chọn ứng dụng…`** · đúng nguyên văn `Choose Application…` của Apple (khóa `N137`) trong Finder
  tiếng Việt · `confirmed`.

## `Sort by relevance`: chú giải của cột kết quả tìm kiếm (`fileExplorer.columns.sortByRelevance`)

Bề mặt mới: chú giải hiện ra khi rê chuột lên tiêu đề cột đang sắp xếp của khung kết quả tìm kiếm. Cú nhấp tiếp theo đưa
các hàng về đúng thứ tự của bộ tìm kiếm, kết quả khớp nhất lên đầu.

- **relevance (mức khớp giữa một kết quả và truy vấn) → `mức độ liên quan`** · Apple WorkflowKit
  (`Relevance (WFSearchSortOrder)` → `Mức độ liên quan`) và Automator (`%1$[Mức độ liên quan]@ …`); Nhạc và TV rút gọn
  thành `Liên quan`, tức là ba trên bốn nguồn dùng chung gốc `liên quan`, nên chọn dạng đầy đủ · `high`. AppStoreKit
  (`SEARCH_FACET_RELEVANCE`) lại nói `Độ phù hợp`, một gốc khác, ghi lại ở đây để lần sau khỏi phải tra lại. (kiểm chứng
  trên macOS 26.6.2, bản dựng 25G83, kết xuất `plutil` các bản địa hóa đi kèm, 2026-09-06)
- **Khung câu → `Sắp xếp theo mức độ liên quan`** · đúng khuôn của các khóa anh em trong `commands.json`
  (`Sắp xếp theo tên`, `Sắp xếp theo kích cỡ`) · `high`. Không cần `sameAsSourceJustification`, và giá trị không có dấu
  nháy đơn.

## `Documents and packages`: hàng OOXML mới (`settings.archives.ooxml.*`)

Bề mặt mới: một hàng trong cùng thẻ với `Tệp nén zip`, nằm trên thẻ `Gói ứng dụng`. Hàng này cố ý bao cả HAI: tài liệu
Office (.docx, .xlsx, .pptx) và gói ứng dụng (.jar, .apk), nên ngay cả bản tiếng Anh cũng không nêu tên Office.

- **documents (loại tệp) → `Tài liệu`** · macOS Finder (`TL6`/`GROUP_DOCUMENTS` → `Tài liệu`; tên loại `Tài liệu RTF`,
  `Tài liệu văn bản thuần túy`), khớp mục termbase `document → tài liệu` · `high`.
- **packages (chung, không chỉ ứng dụng) → `gói`** · macOS Finder (`Hiển thị nội dung gói`) và mục termbase
  `app bundle → gói ứng dụng` · `high`. Giữ `gói` trần để hàng này rộng hơn thẻ `Gói ứng dụng` bên dưới, đúng như tiếng
  Anh tách `packages` với `app bundles`.
- **Khung câu → `Nhấn Enter sẽ làm gì với tệp …, … hoặc ….`** · đúng khuôn của các khóa anh em
  `settings.archives.zip.description` và `settings.archives.bundle.description`, nhưng bỏ dấu phẩy trước `hoặc` theo quy
  ước đã chốt · `high`. Cả ba hàng (`zip`, `bundle`, `ooxml`) giờ cùng dấu câu.

## Trung tâm máy chủ: khung trạng thái kết nối + quên máy chủ / mật khẩu (`servers.refusal.*`, `servers.paneState.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`)

Bề mặt mới: một khung riêng cho máy chủ (SMB/SFTP/WebDAV) đang kết nối hoặc bị từ chối, cộng với hàng máy chủ trong bộ
chuyển ổ đĩa (chấm trạng thái, nút ngắt kết nối ở đúng chỗ mà ổ rời hiện `Tháo`, hai hộp thoại xác nhận `Quên`).

Nguồn: kho tham chiếu KHÔNG có trên máy này (hộp M1), nên toàn bộ dẫn chứng Apple lấy trực tiếp từ bundle macOS đang cài
(`.loctable`, `plistlib.load(f)['vi']` so với `['en']`), macOS 26.6.2 build 25G83, 2026-09-06. Cách làm: xem
`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" và ghi chú cùng tên trong `style.md`.

### Thuật ngữ chốt trong đợt này

- **disconnect (động từ, nhãn nút) → `Ngắt kết nối`; disconnect from X → `ngắt kết nối khỏi X`** · macOS `IOBluetoothUI`
  (`Disconnect` → `Ngắt kết nối`, `Disconnect from Network` → `Ngắt kết nối khỏi Mạng`), và catalog đã dùng đúng từ này
  ở `fileExplorer.unreachable.disconnect`, `fileExplorer.pane.disconnectFailedToast`, `servers.paneState.disconnect` ·
  `high`.
- **the connection dropped (mất ngoài ý muốn) → `Đã mất kết nối`** · macOS `CFNetwork`
  (`The network connection was lost.` → `Đã mất kết nối mạng.`) · `high`. ❗ Đừng viết `Kết nối đã bị ngắt`: `ngắt` là
  từ dành cho hành động CHỦ Ý của người dùng (`Ngắt kết nối`), nên dùng nó cho một cú rớt mạng sẽ khiến hai trạng thái
  khác hẳn nhau đọc y như nhau.
- **signed out → `Đã đăng xuất`; sign in again → `đăng nhập lại`** · macOS `StoreKit` (`Sign Out` → `Đăng xuất`),
  `AppleAccount` (`…require you to sign in again.` → `…yêu cầu bạn đăng nhập lại.`); `Đăng nhập` đã có sẵn ở
  `fileExplorer.network.signIn` · `high`.
- **sign-in method → `cách đăng nhập`** · macOS dịch `Authentication Method` là `Phương thức xác thực`
  (`CoreAudioKit/NetworkMIDILocalizable`, `SingleSignOnService`), nhưng tiếng Anh ở đây cố tình chọn từ đời thường
  ("sign-in method", không phải "authentication method"), nên tiếng Việt cũng đi từ `đăng nhập` chứ không lên giọng
  thuật ngữ · `high`. Dùng `phương thức xác thực` nếu sau này có chuỗi thật sự nói về cơ chế xác thực.
- **key (khóa máy chủ SSH) → `khóa`; khóa của X → `khóa của {host}`** · macOS không có chuỗi "host key" nào trong
  bundle; `khóa` là từ đã chốt cho key nói chung (`khóa khôi phục` = recovery key trong `AppleAccountUI`, và catalog
  dùng `khóa API`, `khóa` cho license key) · `high`. ❌ Đừng dùng `dấu vân tay` (Apple dành từ đó cho fingerprint sinh
  trắc học).
- **trust / untrusted → `tin cậy` / `không tin cậy`** · macOS `Security.framework` (`“%@” certificate is not trusted` →
  `Chứng nhận “%@” không được tin cậy`) · `high`.
- **certificate → `chứng nhận`, không phải `chứng chỉ`** · macOS dùng `chứng nhận` xuyên suốt
  (`Security/Certificate.loctable`, `SecurityInterface`, Truy cập chuỗi khóa) · `high`. `chứng chỉ` là lối Microsoft.
- **Keychain Access (tên ứng dụng) → `Truy cập chuỗi khóa`** · chính bundle của ứng dụng
  (`Keychain Access.app/…/InfoPlist.loctable`: `Keychain Access` → `Truy cập chuỗi khóa`) · `confirmed`. Viết thường chữ
  `chuỗi` như bundle của ứng dụng và như catalog đã có (`ai.secretError.keychainBody`), dù `SecurityInterface` có chỗ
  viết hoa `Chuỗi khóa`.
- **compromised → `bị xâm phạm`; revoked → `bị thu hồi`** · macOS `PassKit/VirtualCard` (`has been compromised` →
  `đã bị xâm phạm`) và `StoreKit` (`Certificate Revoked` → `Chứng nhận bị thu hồi`) · `high`. `hostKeyRevoked` nói
  "marked as compromised", nên dùng `bị xâm phạm`, không phải `bị thu hồi`.
- **forget (bỏ một mục đã lưu khỏi danh sách) → `Quên`** · macOS `WiFiSettingsKit` (`Forget` → `Quên`,
  `Forget Wi‑Fi Network “%@”?` → `Quên mạng Wi‑Fi “%@”?`), và catalog đã có `menu.network.forgetServer` =
  `Quên máy chủ`, `menu.network.forgetSavedPassword` = `Quên mật khẩu đã lưu` · `high`. Đây là ngoại lệ có chủ ý của
  luật `xóa` / `gỡ bỏ` trong `style.md`: khi tiếng Anh gọi hành động là "Forget", tiếng Việt gọi là `Quên` cả trong tiêu
  đề, thân hộp thoại, lẫn toast hỏng (`Cmdr không thể quên {name}.`).
- **operations in progress → `thao tác đang chạy`** · `queue.row.status` đã dịch `Running` là `Đang chạy`, nên nút bị
  tắt phải dùng đúng từ đó thay vì `đang diễn ra` / `đang được tiến hành` của macOS (`FSKit/Errors`) · `high`.
- **didn't answer in time → `đã không phản hồi kịp thời`** · lặp lại nguyên văn khung câu đã ship ở
  `fileExplorer.unreachable.detailTimeout` (`didn't respond in time` → `đã không phản hồi kịp thời`); macOS cũng dùng
  `không phản hồi` cho "did not respond" (`PrintCore/cups`) · `high`.
- **reach a SERVER → `kết nối được tới`; reach a PATH / drive → `truy cập`** · catalog đã tách sẵn hai lối:
  `licensing.error.network` ("couldn't reach the license server") → `không kết nối được tới máy chủ giấy phép`, còn
  `fileExplorer.unreachable.title` và `.locationUnreachableToast` ("couldn't reach {path}" / "that location's drive") →
  `Không thể truy cập …` · `high`. `servers.refusal.unreachable` nói về một máy chủ nên theo lối thứ nhất.
- **This can take a few seconds. → `Việc này có thể mất vài giây.`** · macOS `PassKit/Transit_Localizable`
  (`may take a few seconds` → `có thể mất vài giây`) · `high`.

### Ghi chú theo chuỗi

- **`servers.refusal.authenticationRejected` → `Mật khẩu đó không đúng với {username}.`** · giữ nguyên khung của chuỗi
  chị em đã ship `errors.volume.passwordRejected` ("That password didn't work." → `Mật khẩu đó không đúng.`) và chỉ thêm
  người dùng. Hai chuỗi tả cùng một sự kiện nên phải đọc như một.
- **`servers.refusal.notAWebdavServer` → `Không có gì ở địa chỉ này phản hồi giao thức WebDAV.`** · thêm `giao thức`
  (protocol) vì `phản hồi WebDAV` trần trụi không thành câu tiếng Việt, và vì người đọc cần biết WebDAV là một giao
  thức. `WebDAV` giữ nguyên.
- **`servers.refusal.invalidUrl` → `Cái này trông không giống địa chỉ máy chủ.`** · lấy nguyên khung của
  `common.attachEmailInvalid` ("That doesn't look like an email address" → `Cái này trông không giống địa chỉ email`) ·
  `high`.
- **`fileExplorer.navigation.forgetServerConfirm`: "stops listing it" → `không hiển thị máy chủ này nữa`** · nói về việc
  biến mất khỏi bộ chuyển ổ đĩa, nên `hiển thị` (từ đã chốt cho "show") tự nhiên hơn `liệt kê`. Câu cuối
  (`Các tệp của bạn vẫn ở trên máy chủ.`) là câu trấn an quan trọng nhất; đừng rút gọn.
- **`disconnectPlaceAriaLabel` → `Ngắt kết nối {name}`** · tên phụ trợ chứa nguyên văn nhãn nhìn thấy được
  `Ngắt kết nối` (`servers.paneState.disconnect`, `fileExplorer.unreachable.disconnect`), đúng WCAG 2.5.3. Cùng khuôn
  với chuỗi anh em `ejectVolumeAriaLabel` = `Tháo {name}`.
- **`on this Mac` → `trên máy Mac này`** · macOS `IOBluetoothUI`, `FileProvider` · `high`.
- Không khóa nào trong 28 khóa mang `sameAsSourceJustification`; không giá trị nào chứa dấu nháy đơn.

## Trung tâm máy chủ: bảng máy chủ + hàng "Máy chủ" trong bộ chọn ổ đĩa (`servers.hub.*`, `commands.servers*`, `fileExplorer.navigation.server*Toast`, `shortcuts.scope.servers`, `shortcuts.scope.places`)

Bề mặt: hàng `Network` cũ trong bộ chọn ổ đĩa (chỉ liệt kê máy chủ SMB mDNS thấy ngay lúc đó) nay là hàng **`Servers`**
mở ra một cái bảng: mọi máy chủ đã lưu (SFTP, WebDAV, SMB) cộng với những máy tìm thấy quanh đó, các cột Name / Type /
Address / Status / Last used, và hàng cuối `Add server…`. NHÓM chứa hàng đó vẫn tên là `Network` (`Mạng`).

Nguồn: kho tham chiếu KHÔNG có trên máy này (hộp M1), nên mọi dẫn chứng Apple lấy trực tiếp từ bundle macOS đang cài
(`.loctable` + `.lproj/*.strings`, `plistlib.load(f)['vi']` so với `['en']`), macOS 26.6.2 build 25G83, 2026-09-06. Cách
làm: `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?".

### Thuật ngữ chốt trong đợt này

- **server (trong tên bề mặt, không chỉ trong câu văn) → `Máy chủ`** · Finder vi gọi `Connect to Server…` là
  `Kết nối với máy chủ…` (`LocalizableMerged` `N84`), `Connected servers` là `Máy chủ được kết nối` (`SD13`),
  `Server Volumes` là `Ổ đĩa máy chủ` (`FF22.2`) · `high`. Hai khóa tiếng Anh giống hệt nhau
  (`fileExplorer.navigation.networkVolume`, `shortcuts.scope.servers`) nên phải chung một giá trị;
  `desktop-i18n-term-consistency` bắt lỗi nếu lệch.
- **Name / Type / Address / Status (tiêu đề cột) → `Tên` / `Loại` / `Địa chỉ` / `Trạng thái`** · Finder vi list view
  (`N220` Name → `Tên`, `N224` Kind → `Loại`), macOS `CGImageSource` (Status → `Trạng thái`), Finder `A20` ("server at
  the address you specified" → `máy chủ tại địa chỉ bạn đã chỉ định`) · `high`. Cả bốn từ đã có trong catalog ở khóa
  khác cùng tiếng Anh (`fileExplorer.columns.name`, `queryUi.ai.filter.type`, `licensing.section.labelStatus`), nên đây
  cũng là ràng buộc của `desktop-i18n-term-consistency`.
- **Last used → `Dùng cuối`; và ô rỗng của nó, Never → `Chưa từng`** · nguồn khớp ĐÚNG bề mặt: bảng quyền ứng dụng trong
  Cài đặt hệ thống > Quyền riêng tư & bảo mật có y hệt một cột `Last Used` với giá trị `Never`
  (`Security.prefPane/Localizable.loctable` và `SecurityPrivacyExtension.appex`: `Last Used` → `Dùng cuối`, `Never` →
  `Chưa từng`), `PrinterScannerSettings.appex` cũng nói `Dùng cuối` · `high`. ❌ Đừng lấy `Không` hay `Không bao giờ`:
  đó là `Never` của một Ô CHỌN lịch/lặp lại (Calendar, Mail, Wi‑Fi), nghĩa là "đừng bao giờ làm", không phải "tới giờ
  vẫn chưa xảy ra". Cột này nói về quá khứ, nên `Chưa từng`. Ghi chú thêm: Finder gọi cột `Last Opened` là
  `Mở lần cuối`, cùng một khuôn `<động từ> + lần cuối / cuối`.
- **Connected (nhãn trạng thái) → `Đã kết nối`** · macOS `Localizable.loctable` (`Connected` → `Đã kết nối`), và catalog
  đã có ở `fileExplorer.network.browser.status.connected`, `ai.cloud.connected` · `high`.
- **Found nearby → `Tìm thấy ở gần`** · `nearby` là `ở gần` (`MCBrowserViewController` `Nearby` → `Ở gần`,
  `CopresenceCore` `Nearby Device` → `Thiết bị ở gần`) và `tìm thấy` là từ Apple dùng cho "discovered/findable"
  (`BTLEMIDILocalizable`: `discoverable` → `có thể tìm thấy`) · `high`. Đây là một trong bốn nhãn của cột Trạng thái,
  cạnh `Đã kết nối` / `Đã lưu` / `Đã đăng xuất`; nó cố tình KHÔNG mở đầu bằng `Đã` vì máy chủ này chưa hề được lưu hay
  kết nối, chỉ là Cmdr đang nhìn thấy nó.
- **pin / unpin (đưa một mục vào hoặc ra khỏi một danh sách) → `Ghim` / `Bỏ ghim`** · macOS thống nhất trên nhiều bundle
  (`MapKit`, `Shortcuts.app`, `Maps.app`, `WorkflowEditor`, `VideosUI`), và catalog đã có `menu.tab.pinTab` =
  `Ghim tab`, `menu.tab.unpinTab` = `Bỏ ghim tab` · `high`. ❌ Không dùng `Gỡ ghim` (Notes.app có, nhưng thiểu số).
- **volume switcher → `bộ chọn ổ đĩa`** · tiếng Anh giờ có HAI tên cho cùng một bề mặt ("volume chooser" ở
  `shortcuts.scope.volumeChooser` và ba khóa `commands.*VolumeChooser`, "volume switcher" ở hai toast mới), tiếng Việt
  chỉ dùng một: `bộ chọn ổ đĩa`, đúng như bốn khóa đã ship · `high`. ❌ Đừng nghĩ ra `bộ chuyển ổ đĩa` cho các chuỗi
  mới; hai tên tiếng Việt cho một danh sách sẽ khiến người đọc đi tìm hai thứ khác nhau.
- **Places (tên nhóm phím tắt cho danh sách "nơi chốn" bên trong một máy chủ) → `Vị trí`** · Finder vi gọi mục
  `Locations` ở khung bên là `Vị trí` (`LocalizableMerged` `SD5`, `FI9`) và `Recent Places` là `Vị trí gần đây` (`FI1`)
  · `high`. ❌ KHÔNG dùng `Địa điểm`: Apple dành riêng từ đó cho nghĩa ĐỊA LÝ (Photos, Maps, Journal đều dịch `Places`
  là `Địa điểm`), còn ở đây "places" là các bản chia sẻ SMB hôm nay và các bucket lưu trữ sau này. Trùng chữ với
  `vị trí` = chỗ của một tệp trong catalog là chấp nhận được: cả hai đều là "chỗ để đi tới", và Apple cũng dùng chung
  một từ.
- **Edit (động từ, nhãn lệnh) → `Sửa`** · macOS `MainMenu.loctable` (menu `Edit` → `Sửa`), và catalog đã có
  `shortcuts.window.editInSettings` = `Sửa phím tắt trong Cài đặt` · `high`.
- **Add server → `Thêm máy chủ`** · macOS `WiFiSettingsKit/Localizable.loctable` (`Add Server` → `Thêm máy chủ`) ·
  `high`.

### Ghi chú theo chuỗi

- **`servers.hub.discoveryOff` → `Việc tìm máy chủ trên mạng cục bộ đang tắt.`** · "local network discovery" không có
  một tên riêng trong bundle macOS; cái Apple đặt tên là quyền `Local Network` = `Mạng cục bộ` (catalog đã dùng ở
  `settings.network.*`). Nên câu này tả việc chứ không dịch thuật ngữ: `Việc <làm gì> đang tắt.`, đúng khuôn đã ship ở
  `Lập chỉ mục ổ đĩa đang tắt trong Cài đặt` (`driveIndex.tooltipIndexingOff`). Dùng `tìm` (find) chứ không `tìm kiếm`
  (search): `style.md` đã tách hai từ này, và `tìm kiếm` trong catalog thuộc về việc tìm tệp.
- **`servers.hub.discoveryOffLink` → `Bật trong Cài đặt`** · tân ngữ của `bật` bỏ trống vì câu ngay trước đã nêu chủ
  thể; catalog đã ship đúng lối này ở `driveIndex.tooltipDisabled` (`Turn it on to see…` → `Hãy bật để xem…`). Không
  thêm `Hãy`: đây là nhãn liên kết, không phải lời hướng dẫn. `Settings` là cửa sổ Cài đặt của chính ứng dụng, catalog
  gọi là `Cài đặt` xuyên suốt.
- **`servers.hub.status.waitingForKey` → `Đang chờ bạn kiểm tra khóa`** · `khóa` (không phải `dấu vân tay`) đã chốt ở
  đợt trước cho SSH host key; `Đang chờ` theo `fileExplorer.network.browser.status.waitingForNetwork` = `Đang chờ mạng`.
  Tiếng Anh xưng hô trực tiếp nên tiếng Việt giữ `bạn`.
- **`servers.hub.emptyMessage`** · `NAS` giữ nguyên (catalog đã dùng `một NAS ở nhà`, `phần cứng NAS gia đình`),
  `máy Mac` theo lối catalog + macOS. `sẽ tìm thấy nó` lặp lại `tìm thấy` của nhãn trạng thái `Tìm thấy ở gần`, để hai
  chuỗi cùng nói về một việc bằng một từ.
- **`fileExplorer.navigation.serverUnpinnedToast`** · câu thứ hai (`Máy chủ này vẫn được lưu.`) là câu trấn an, viết đủ
  chủ ngữ chứ không rút thành `Vẫn được lưu.`, vì toast có thể đọc rời khỏi ngữ cảnh.
- **`servers.hub.rowCount`** · `vi` chỉ có nhánh `other`, nên `{count, plural, other {{countText} máy chủ}}`; danh từ
  không biến đổi theo số.
- Không khóa nào trong 28 khóa mang `sameAsSourceJustification`; không giá trị nào chứa dấu nháy đơn.

## Trung tâm máy chủ: tấm thêm/sửa máy chủ + câu hỏi tin cậy khóa SSH (`servers.sheet.*`, `servers.hostKey.*`, `goToPath.dialog.opensServer`, `goToPath.dialog.addsServer`, `commands.serversConnect.label`)

Bề mặt: một tấm (sheet) duy nhất dùng cho cả ba việc — thêm một máy chủ mới (SMB / SFTP / WebDAV), đăng nhập lại vào một
máy chủ đang hỏi mật khẩu, và sửa một máy chủ đã lưu — cộng bước hỏi có tin cậy khóa SSH của máy chủ hay không (lần đầu,
và khi khóa đã đổi), hai khung trạng thái tương ứng trong khung tệp, và hai dòng xem trước dưới ô Đi tới đường dẫn.

Nguồn: kho tham chiếu KHÔNG có trên máy này (hộp M1), nên mọi dẫn chứng Apple lấy trực tiếp từ bundle macOS đang cài
(`.loctable` + `.lproj/*.strings`, `plistlib.load(f)['vi']` so với `['en']`), macOS 26.6.2 build 25G83, 2026-09-07. Cách
làm: `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?". Nguồn giàu nhất cho đợt này là hộp thoại
Kết nối với máy chủ của chính Apple (`NetAuthAgent.app/…/{AuthDialog,Localizable}.loctable`), vốn có gần đủ mọi nhãn mà
tấm của Cmdr cần.

### Thuật ngữ chốt trong đợt này

- **protocol → `Giao thức`** · macOS `AddPrinter/IP.plugin` (`Protocol:` → `Giao thức:`, và `100257`
  `ibExternalAccessibilityDescription` `Protocol` → `Giao thức` — đúng y bề mặt của ta, một tên phụ trợ cho bộ chọn) ·
  `high`. Catalog đã dùng `giao thức` ở `servers.refusal.notAWebdavServer`.
- **sign in to X → `Đăng nhập vào X`** · macOS Setup Assistant (`Sign In to iCloud` → `Đăng nhập vào iCloud`), CloudKit
  (`Sign in to %1$@.` → `Đăng nhập vào %1$@.`) · `high`. `servers.sheet.signInTitle` (`Sign in to {name}`) theo đúng
  khuôn này: `Đăng nhập vào {name}`, không thêm dấu nháy quanh tên máy chủ.
- **passphrase → `cụm từ mật khẩu`; "Key passphrase" → `Cụm từ mật khẩu của khóa`** · macOS Certificate Assistant
  (`Enter Passphrase:` → `Nhập Cụm từ mật khẩu:`), `DiskManagement` và `DiskImages2` dùng `cụm từ mật khẩu` xuyên suốt ·
  `high`. Thêm `của khóa` vì tiếng Anh cố tình phân biệt nó với mật khẩu của TÀI KHOẢN ở ngay trên; bỏ đi thì hai ô
  trong cùng một biểu mẫu đọc như nhau.
- **fingerprint (của một khóa hay chứng nhận) → `dấu vân tay`** · macOS `Security.framework/Certificate.loctable`
  (`Fingerprints` → `Dấu vân tay`, chính là mục dấu vân tay của chứng nhận trong Truy cập chuỗi khóa) · `high`. ❗ **Đây
  là đính chính cho ghi chú của đợt 1** ("❌ đừng dùng `dấu vân tay`"): ghi chú đó nói về việc dịch chữ **key**, chứ
  không phải chữ **fingerprint**. Apple dùng `dấu vân tay` cho cả dấu vân tay sinh trắc học lẫn dấu vân tay mật mã, nên
  hai từ tiếng Anh vẫn tách nhau đúng như bản gốc: key = `khóa`, fingerprint = `dấu vân tay`.
- **trust (nhãn nút) → `Tin cậy`** · macOS Setup Assistant `To Another Mac View` (`89.label` `Trust` → `Tin cậy`), khớp
  với `tin cậy` / `không tin cậy` đã chốt ở đợt 1 · `high`.
- **Keychain (tính năng, trong một ô đánh dấu) → `chuỗi khóa`** · macOS Certificate Assistant (`Keychain` →
  `Chuỗi khóa`, `in your keychain` → `trong chuỗi khóa của bạn`), NetAuthAgent (`Remember this password in my keychain`
  → `Nhớ mật khẩu này trong chuỗi khóa của tôi`) · `high`. Nhãn ô đánh dấu `servers.sheet.remember` là
  `Ghi nhớ trong chuỗi khóa`, và `servers.sheet.needsStoredSecret` trích lại đúng từng chữ, nên hai chuỗi phải đi cùng
  nhau (`desktop-i18n-term-consistency` bắt lỗi nếu lệch).
- **Advanced (mục gập lại) → `Nâng cao`** · macOS Finder `vi.lproj/PreferencesWindow.strings` (`Advanced` → `Nâng cao`),
  và catalog đã có `settings.section.advanced` cùng tiếng Anh · `high`.
- **Browse… (nút mở bộ chọn tệp) → `Duyệt…`** · macOS `StandardAdditions.osax/ChooseApplication` (`Browse...` →
  `Duyệt...`, đúng một nút mở bộ chọn tệp) và NetAuthAgent (`Browse` → `Duyệt`); catalog đã có
  `settings.archives.opt.browse` = `Duyệt` · `high`. Giữ dấu … (U+2026) như bản gốc.
- **remote → `từ xa`; "Remote folder" → `Thư mục từ xa`** · macOS Finder `LocalizableMerged` (`Remote` → `Từ xa`,
  `Remote Volume` → `Ổ đĩa từ xa`) · `high`. Cùng khuôn với `Ổ đĩa từ xa` nên người đọc nhận ra ngay đây là chỗ trên máy
  chủ, không phải trên máy mình.
- **hostname → `tên máy chủ`** · macOS Certificate Assistant (`Hostname mismatch` → `Tên máy chủ không khớp`),
  AddPrinter (`Enter host name or IP address.` → `Nhập tên máy chủ hoặc địa chỉ IP.`) · `high`.
- **owner (của một máy chủ) → `chủ sở hữu`** · macOS iCloud `CloudKitVetting` (`the owner stopped sharing it` →
  `chủ sở hữu đã dừng chia sẻ`) · `high`. `chủ sở hữu máy chủ` dài nhưng rõ; ❌ đừng rút thành `chủ máy chủ`, nghe như
  một danh từ ghép lạ.
- **stopped <làm gì> → `đã dừng <động từ>`** · macOS Keychain First Aid (`Repair stopped by user` →
  `Người dùng đã dừng sửa chữa`), iCloud (`stopped sharing it` → `đã dừng chia sẻ`) · `high`. Nên
  `Cmdr stopped connecting to {name}` → `Cmdr đã dừng kết nối tới {name}`. ❌ Đừng dùng `ngắt`: `ngắt` thuộc về hành
  động chủ ý của NGƯỜI DÙNG (`Ngắt kết nối`), còn ở đây chính Cmdr là bên dừng lại.
- **signed out of X → `đã đăng xuất khỏi X`** · macOS Erase Assistant (`Failed to sign out of iCloud` →
  `Không thể đăng xuất khỏi iCloud`, `signed out of Apple Music…` → `đăng xuất khỏi Apple Music…`) · `high`. Khớp với
  `Đã đăng xuất` đã chốt ở đợt 2.
- **reinstalled → `cài đặt lại`** · macOS StartupDisk (`needs to be reinstalled` → `cần được cài đặt lại`), StorageUI ·
  `high`.
- **reconnect automatically → `Tự động kết nối lại`** · catalog đặt trạng từ lên trước xuyên suốt
  (`settings.updates.autoCheck.label` = `Tự động kiểm tra cập nhật`, `settings.updates.errorReports.label` =
  `Tự động gửi báo cáo trục trặc`, `fileExplorer.navigation.spaceRetryingAuto` = `Đang tự động thử lại…`) · `high`.
- **"check X against Y" → `đối chiếu X với Y`; "check" trần → `kiểm tra`** · macOS dùng `đối chiếu` cho việc so khớp
  (TouchID `matching accuracy` → `độ chính xác khi đối chiếu`), còn `kiểm tra` là từ catalog đã chọn cho chính việc này
  ở `servers.hub.status.waitingForKey` = `Đang chờ bạn kiểm tra khóa` · `high`. Hai từ chia nhau theo cấu trúc câu tiếng
  Anh: có vế "against/với cái gì" thì `đối chiếu`, không có thì `kiểm tra`.
- **"something is sitting between you and it" → `có gì đó đang nằm giữa bạn và máy chủ`** · macOS không có chuỗi nào nói
  về tấn công xen giữa, nên đây là lối tả bằng tiếng Việt đời thường, giữ nguyên hình ảnh của bản gốc thay vì gọi tên
  thuật ngữ (`tấn công xen giữa`) mà người đọc phổ thông không cần · `tentative`.
- **guest → `khách`; "Connect as guest" → `Kết nối với tư cách khách`** · macOS `SystemFolderLocalizations` (`Guest` →
  `Khách`), loginwindow (`logged in as a guest user` → `đăng nhập với tư cách người dùng khách`); catalog đã ship
  `khách` ở `fileExplorer.network.browser.status.guest` (`Guest` → `Khách`) · `high`.

### Ghi chú theo chuỗi

- **`servers.sheet.addressHelp` →
  `Một tên máy chủ, một địa chỉ bạn đã sao chép, hoặc cả một dòng lệnh ssh. Cmdr tự chọn giao thức.`** · `ssh` giữ
  nguyên chữ thường như `@key.description` yêu cầu; `dòng lệnh ssh` chứ không `dòng ssh`, vì `lệnh` là từ đã chốt cho
  một câu lệnh shell (`style.md`) và bỏ nó đi thì `dòng ssh` không thành tiếng Việt. Câu hai viết ở thì hiện tại, thêm
  `tự` để nói rõ người dùng không phải chọn.
- **`servers.sheet.needsStoredSecret`** · trích dẫn phải khớp TỪNG CHỮ nhãn ô đánh dấu ngay phía trên, nên trong ngoặc
  kép cong là `“Ghi nhớ trong chuỗi khóa”`, không phải một biến thể mới. `Việc tự kết nối lại` mở đầu bằng `Việc` để câu
  có chủ ngữ, cùng khuôn với `Việc tìm máy chủ trên mạng cục bộ đang tắt.` của đợt 2.
- **`servers.hostKey.changedBody` và `servers.paneState.hostKeyChangedHint` dùng chung một câu giữa** · tiếng Anh viết
  gần y hệt (`This can mean…` / `That can mean…`), nên tiếng Việt dùng đúng một câu:
  `Có thể máy chủ đã được cài đặt lại, hoặc có gì đó đang nằm giữa bạn và máy chủ.` Người đọc gặp cùng một cảnh báo ở
  hai bề mặt thì phải thấy cùng một chữ, đúng luật "chuỗi chị em" trong `style.md`. Bỏ `Điều này có nghĩa là` vì tiếng
  Việt không cần vế dẫn; `Có thể …` đã mang đủ sắc thái phỏng đoán và ngắn hơn hẳn.
- **`servers.hostKey.changedDisclosure` → `Tôi đã kiểm tra rồi`** · ngôi thứ nhất, chính người dùng nói, đúng như bản
  gốc. Dùng `kiểm tra` (không phải `đối chiếu`) để nối thẳng với `Đang chờ bạn kiểm tra khóa` ở cột Trạng thái: đây là
  câu trả lời cho đúng lời nhắc đó.
- **`servers.paneState.hostKeyChangedHint` mở đầu bằng `Khóa của máy chủ đã thay đổi.`** · tiếng Anh dùng đại từ
  (`Its key changed.`), tiếng Việt viết rõ `của máy chủ` vì tiêu đề ngay trên đã nêu tên riêng và một đại từ trống ở đây
  sẽ đọc lửng.
- **`goToPath.dialog.opensServer` / `.addsServer` → `Mở {name}` / `Thêm một máy chủ`** · dòng xem trước, ngôi thứ ba thì
  hiện tại; tiếng Việt không chia động từ nên chỉ còn động từ trần, đúng lối macOS Installer
  (`Installs %@ for the first time.` → `Cài đặt %@ lần đầu tiên.`). Giữ `một` ở chuỗi thứ hai để nó đọc như một lời mô
  tả chứ không phải nhãn nút `Thêm máy chủ…` ở bảng máy chủ.
- **`commands.serversConnect.label` → `Kết nối với máy chủ…`** · tiếng Anh trùng khít
  `settings.network.permissionIntroConnectLink`, và Finder vi gọi `Connect to Server…` đúng như vậy (`LocalizableMerged`
  `N84`). Ràng buộc của `desktop-i18n-term-consistency`.
- **Bốn khóa mang `sameAsSourceJustification`**: `servers.sheet.protocolSmb` / `.protocolSftp` / `.protocolWebdav` (tên
  giao thức, macOS vi giữ nguyên: NetAuthAgent `WEBDAV_PASSWORD` → `Mật khẩu WebDAV`, `FTP_PASSWORD` → `Mật khẩu FTP`)
  và `servers.sheet.addressPlaceholder` (`nas.local`, một tên máy ví dụ: `nas` là chữ viết tắt catalog giữ nguyên,
  `.local` là hậu tố mDNS).

## Trung tâm máy chủ: khung đang kết nối lại + dòng "không có gì để nhập" (`servers.paneState.reconnecting`, `.signedOutNothingToAsk`)

Bề mặt: hai chuỗi trong cùng khung trạng thái máy chủ của đợt 3. Một là tiêu đề của khung Cmdr TỰ tìm lại kết nối đã rớt
(dưới nó là vòng quay, đồng hồ đếm ngược tới lần thử sau, và ba nút Thử lại ngay / Hủy / Ngắt kết nối). Hai là dòng thay
chỗ nút `Đăng nhập…` khi máy chủ nhận khóa SSH (hoặc danh tính ssh-agent) chứ không hỏi mật khẩu, nên người dùng thật sự
không có gì để điền.

Nguồn: kho tham chiếu KHÔNG có trên máy này (hộp M1), nên mọi dẫn chứng Apple lấy trực tiếp từ bundle macOS đang cài
(`.loctable` + `.lproj/*.strings`, `plistlib.load(f)['vi']` so với `['en']`), macOS 26.6.2 build 25G83, 2026-09-07. Cách
làm: `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?".

### Thuật ngữ chốt trong đợt này

- **Reconnecting… (tiêu đề đang chạy) → `Đang kết nối lại…`** · macOS `ScreenSharing.framework/ScreenSharing.loctable`
  (`Reconnecting…` → `Đang kết nối lại…`, đúng bề mặt của ta: một phiên tới máy ở xa bị rớt và đang được nối lại) và
  `ConversationKit.loctable` (`Reconnecting` → `Đang kết nối lại`) · `high`. Khớp với `kết nối lại` đã chốt cho động từ
  reconnect và với `Đang kết nối lại với thiết bị` (`errors.listing.deviceReconnecting.title`).
- **rather than / instead of → `thay vì`** · catalog đã dùng xuyên suốt
  (`errors.listing.crossDeviceOperation.suggestion` `Hãy sao chép mục này tới đích thay vì di chuyển.`,
  `errors.write.readOnlyDevice.source.suggestion`, `settings.appearance.useAppIconsForDocuments.description`), và macOS
  dịch đúng chữ "rather than" như vậy (`PrintCore/cups`: `Purge jobs rather than just canceling` →
  `Thanh lọc các tác vụ thay vì chỉ hủy`; `Security/SecErrorMessages`: `a CA rather than an end-entity` →
  `một CA thay vì một thực thể cuối`) · `high`. ❌ Đừng đổi sang `chứ không phải` cho chuỗi mới: `thay vì` là chữ
  catalog đã ship ở sáu chỗ.
- **type / enter (điền vào một ô) → `nhập`, không phải `gõ`** · macOS `Security/SecurityAgent`
  (`Enter your password to allow this.` → `Nhập mật khẩu của bạn để cho phép việc này.`), và catalog đã có
  `fileExplorer.network.browser.tooltip.requiresLogin` = `Bấm đúp để nhập thông tin đăng nhập.` · `high`. Apple để dành
  `gõ ký tự` cho việc gõ trên BÀN PHÍM (`IOBluetoothUI`: `to type on this Mac` → `để gõ ký tự trên máy Mac này`), còn ở
  đây nói về nội dung của một ô, nên `nhập`. Chuỗi mới lặp âm `nhập` ngay sau `đăng nhập`; đó là chuyện bình thường và
  catalog đã ship y vậy ở chuỗi tooltip trên.
- **"there's nothing to X" → `nên không có gì để <động từ>`** · khung câu catalog đã ship ở
  `errors.eject.volumeNotFound` (`Ổ đĩa đó không còn kết nối nữa, nên không có gì để tháo.`) và
  `errors.eject.notAnSmbVolume`; Apple cũng dùng đúng lối này (`AppleIDSetup`: `Nothing to copy` →
  `Không có gì để sao chép`) · `high`.

### Ghi chú theo chuỗi

- **`servers.paneState.reconnecting` → `Đang kết nối lại tới {name}…`** · giới từ là `tới`, lấy thẳng từ chuỗi chị em
  cùng khung `servers.paneState.connecting` = `Đang kết nối tới {name}…`, và từ lối catalog dùng cho MÁY CHỦ
  (`kết nối tới máy chủ`, `Đang kết nối tới {hostName}...`). macOS thiên về `với` / `vào` (`Đang kết nối với “%@”…`,
  `kết nối lại vào Nguồn của bạn`), nhưng hai tiêu đề này là hai trạng thái của cùng một khung nên phải đọc như một cặp;
  ưu tiên chuỗi chị em. Giữ dấu … (U+2026).
- **`servers.paneState.signedOutNothingToAsk` →
  `Máy chủ này dùng khóa thay vì mật khẩu để đăng nhập, nên không có gì để nhập. Hãy mở lại máy chủ để thử lại.`** · ❗
  **Máy chủ không thể làm chủ ngữ của `đăng nhập`.** Tiếng Anh viết được "This server signs in with a key", nhưng
  `đăng nhập bằng X` trong tiếng Việt luôn có NGƯỜI làm chủ ngữ (macOS Setup Assistant:
  `bạn sẽ cần đăng nhập bằng tên tài khoản và mật khẩu`), nên `Máy chủ này đăng nhập bằng khóa` sẽ đọc thành "máy chủ
  này đi đăng nhập ở đâu đó". Đổi động từ chính thành `dùng` và để `đăng nhập` xuống vế mục đích: máy chủ DÙNG khóa,
  việc đăng nhập là mục đích. Đặt `thay vì mật khẩu` ngay sau `khóa` để phần đối lập dính vào đúng danh từ.
- **Câu hai viết rõ `máy chủ` chứ không bỏ trống tân ngữ** · cùng lý do đã ghi cho `paneState.hostKeyChangedHint`: một
  `Hãy mở lại để thử lại.` trần có thể đọc thành mở lại ỨNG DỤNG. `Hãy` giữ nguyên vì đây là lời hướng dẫn, không phải
  nhãn nút (đối lập với `Bật trong Cài đặt`). Hai chữ `lại` liền nhau (`mở lại … thử lại`) là bình thường; macOS Setup
  Assistant viết y vậy: `Hãy kết nối lại vào mạng và thử cập nhật lại.`
- Không khóa nào trong hai khóa mang `sameAsSourceJustification`; không giá trị nào chứa dấu nháy đơn, nên không có dấu
  nháy nào phải nhân đôi.

## Trung tâm máy chủ: ghim/bỏ ghim + trang Cài đặt cho máy chủ và ADB (`menu.network.pinToSwitcher`, `.unpin`, `servers.pinHint.*`, `settings.servers.*`, `settings.adb.*`, `settings.section.servers`, `settings.section.adb`, `settings.summary.servers`, `settings.summary.adb`, `settings.appearance.tintSmb.*`)

Bề mặt: (1) hai mục menu chuột phải trên hàng máy chủ trong bộ chọn ổ đĩa; (2) một thông báo một lần khi nhóm `Mạng`
trong bộ chọn có tới năm máy chủ; (3) hai tiểu mục mới của Cài đặt > Hệ thống tệp (`Máy chủ (SFTP, WebDAV)` và
`Android (ADB)`) cùng toàn bộ nội dung của chúng; (4) nhãn phủ màu khung máy chủ, nay phủ cả SFTP và WebDAV chứ không
riêng SMB.

Nguồn: kho tham chiếu KHÔNG có trên máy này (hộp M1). Mọi dẫn chứng Apple lấy trực tiếp từ bundle macOS đang cài
(`.loctable` qua `plistlib.load(f)['vi']` so với `['en']`, và `.lproj/*.strings` qua `plutil -convert json`), macOS
26.6.2 build 25G83, 2026-09-07. Cách làm: `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?" và §
"Menu-bar labels".

### Thuật ngữ chốt trong đợt này

- **pin / unpin → `ghim` / `bỏ ghim`** · macOS AppKit `MenuCommands.loctable` (`Pin Tab` → `Ghim tab`, `Unpin Tab` →
  `Bỏ ghim tab`) — đây là bộ lệnh menu chuẩn của hệ thống, nên nó thắng các biến thể lẻ trong Notes (`Gỡ ghim`),
  StorageUI (`Hủy ghim`) hay Maps (`Gỡ ghim tuyến`) · `high`. Khớp luôn với chuỗi catalog đã ship
  `commands.serversTogglePin.label` = `Ghim / bỏ ghim máy chủ`. ❌ Đừng viết `gỡ ghim` hay `hủy ghim` cho khóa mới: ba
  bề mặt (mục menu, thân thông báo, mô tả cài đặt nội bộ) phải đọc y hệt nhau.
- **group (một nhóm mục trong danh sách) → `nhóm`** · macOS Finder `vi.lproj/LocalizableMerged.strings` khóa `TL29`
  (`Group` → `Nhóm`) và `vi.lproj/AFPUserGroupSheet.strings` khóa `ze7-Ht-Q14.title` · `high`. Tiêu đề nhóm trong bộ
  chọn giữ nguyên chữ của `fileExplorer.navigation.groupNetwork` = `Mạng`, nên cả cụm là `nhóm Mạng`.
- **Browse… (nút mở bộ chọn tệp) → `Duyệt…`** · macOS Finder `vi.lproj/ConnectToWindow.strings` khóa `48.title`
  (`Browse` → `Duyệt`) — đúng bề mặt: nút `Duyệt` trong cửa sổ "Kết nối tới máy chủ" · `high`. Catalog đã ship
  `servers.sheet.browse` = `Duyệt…`; giữ dấu … (U+2026).
- **Not found (giá trị của một ô trạng thái) → `Không tìm thấy`** · macOS `PhotosGraph.framework` khóa
  `PGErrorFormatNotFound`, `Stickies.app` khóa `NOT_FOUND`, và AppKit `AppKitErrors.loctable` dùng đúng động từ này ở
  mọi câu "was not found" (`… vì không tìm thấy tệp.`) · `high`.
- **Check Again → `Kiểm tra lại`** · macOS `Mail.app/ConnectionDoctor.loctable` khóa `100017.title` (`Check Again` →
  `Kiểm tra lại`); `SoftwareUpdate.framework` khóa `CheckAgain` viết hoa `Lại` nhưng Cmdr dùng sentence case nên lấy
  dạng của Mail · `high`. Dùng cho `settings.adb.recheck`, và `settings.adb.install.intro` phải nhắc lại ĐÚNG chữ này.
- **trust (động từ) → `tin cậy`; trusted X → `X được tin cậy`; đã tin cậy rồi → `Đã tin cậy`** · macOS Setup Assistant
  `To Another Mac View.loctable` khóa `89.label` (`Trust` → `Tin cậy`), Find My `Localizable-MOONDRAGON.loctable`
  (`Trusted Locations` → `Vị trí được tin cậy`), Certificate Assistant (`Trusted Root` → `Gốc được Tin cậy`) · `high`.
  Catalog đã ship `servers.hostKey.trustAndConnect` = `Tin cậy và kết nối`, nên `Trusted host keys` thành
  `Khóa máy chủ được tin cậy`, còn tiền tố đứng trước ngày (`Trusted 2026-09-07`) thành `Đã tin cậy`, cùng khuôn với
  `Đã kết nối` / `Đã lưu` / `Đã ghim` trong catalog.
- **host key → `khóa máy chủ`, và trong câu thì chỉ là `khóa`** · catalog đã đặt lối này ở đợt 3
  (`servers.refusal.hostKeyUntrusted` = `Cmdr chưa tin cậy khóa của {host}.`, `servers.hostKey.fingerprintLabel` =
  `Dấu vân tay của khóa`) · `high`. `fingerprint` giữ `dấu vân tay`. Không có chuỗi macOS nào nói "host key", nên đây là
  nhất quán catalog chứ không phải dẫn chứng Apple.
- **Got it (nút đóng thông báo) → `Đã hiểu`** · ba chuỗi chị em đã ship trong catalog (`ai.toast.gotIt`,
  `main.oldMacos.gotIt`, `updates.moveToApplicationsDialog.gotIt`) · `high`. ❌ Đừng nghĩ ra `Đã rõ` cho khóa mới:
  `Đã rõ` chỉ dùng ở `whatsNew.optOutToast`, nơi tiếng Anh là "Got it, no more…" trong một câu chứ không phải nhãn nút.

### Ghi chú theo chuỗi

- **`menu.network.pinToSwitcher` → `Ghim vào bộ chọn ổ đĩa`** · tiếng Anh gọi danh sách này là "switcher", nhưng tiếng
  Việt chỉ có MỘT tên cho nó, `bộ chọn ổ đĩa` (luật đã ghi trong `style.md`: một bề mặt, một tên tiếng Việt). Nhãn dài
  hơn tiếng Anh, đó là mức giãn bình thường của tiếng Việt; menu chuột phải không bị bó chiều ngang như ô trong hàng.
- **`servers.pinHint.body` →
  `Bấm chuột phải vào một máy chủ rồi chọn Bỏ ghim, hoặc dùng “{command}” trong bảng lệnh. Máy chủ vẫn nằm trong danh sách Máy chủ.`**
  · `Bỏ ghim` phải khớp TỪNG CHỮ với `menu.network.unpin`; `bảng lệnh` là chữ catalog dùng cho command palette
  (`commands.appCommandPalette.label` = `Mở bảng lệnh`); `danh sách Máy chủ` lấy tên hàng trong bộ chọn từ
  `fileExplorer.navigation.networkVolume` = `Máy chủ`. Dấu nháy quanh `{command}` là nháy kép cong “…” theo `style.md`,
  không phải `"` thẳng của bản tiếng Anh. Câu cuối viết rõ chủ ngữ `Máy chủ` thay vì "It" trần, vì một `Nó vẫn nằm…` có
  thể đọc thành cái nhóm chứ không phải máy chủ vừa bỏ ghim.
- **`settings.behavior.serversPinHintSeen.*` đi theo khuôn của cặp chị em đã ship**
  (`settings.behavior.openTerminalHereToastSeen.label` = `Đã hiện gợi ý về “Mở terminal tại đây”` / `.description` =
  `Gợi ý một lần về việc chọn ứng dụng terminal đã hiện hay chưa.`), nên nhãn là `Đã hiện gợi ý về nhóm Mạng dài` và mô
  tả là `Gợi ý một lần về việc bỏ ghim máy chủ đã hiện hay chưa.` Hai khóa này không bao giờ hiện trên giao diện.
- **`settings.section.servers` → `Máy chủ (SFTP, WebDAV)`** · phần trong ngoặc là tên giao thức nên giữ nguyên, nhưng
  `Servers` thì dịch: nó là tên một tiểu mục Cài đặt, đứng cạnh `Bản chia sẻ SMB/mạng` và `MTP (Android/Kindle/máy ảnh)`
  vốn đã dịch phần mô tả.
- **`settings.section.adb` giữ nguyên `Android (ADB)`** và mang `sameAsSourceJustification`: cả tên sản phẩm lẫn từ viết
  tắt đều thuộc danh sách không dịch, y như `settings.section.git` và `settings.section.ai`. Đây là khóa DUY NHẤT của
  đợt này giống hệt bản tiếng Anh.
- **`settings.summary.adb` → `Duyệt điện thoại Android đang bật gỡ lỗi qua USB.`** · `gỡ lỗi qua USB` là chữ AOSP tiếng
  Việt dùng cho chính công tắc đó (nguồn ở khối 2026-09-07 cuối tệp; luật cũng đã ghi trong `style.md`), và catalog ship
  đúng cụm `một điện thoại Android đang bật gỡ lỗi qua USB` ở `settings.fileOperations.adbEnabled.description`.
- **`settings.adb.status.watching` / `.notWatching` → `Đang theo dõi để phát hiện điện thoại.` /
  `Hiện không theo dõi để phát hiện điện thoại.`** · `theo dõi` là chữ catalog dùng cho "watch"
  (`common.downloadsFdaHint`, `settings.advanced.card.fileWatching` = `Theo dõi tệp`), nhưng một `theo dõi điện thoại`
  trần đọc thành "giám sát cái điện thoại"; thêm `để phát hiện` (chữ của `settings.summary.mtp` =
  `Phát hiện thiết bị Android…`) là đủ để trả lại nghĩa "chờ điện thoại xuất hiện". Không nhắc tới máy chủ ADB, đăng ký
  hay socket, đúng yêu cầu của `en`.
- **`settings.adb.pathPlaceholder` → `Tìm adb theo cách thông thường`** · dùng lại nguyên văn cụm đã ship ở
  `settings.fileOperations.adbBinaryPath.description` (`Cmdr tìm adb theo cách thông thường`), vì ô này chính là ô mà mô
  tả kia nói tới. `adb` giữ chữ thường.
- **`settings.servers.trustedHostKeys.forget` → `Quên`** · ngoại lệ có chủ ý của luật `xóa` / `gỡ bỏ`, đã ghi trong
  `style.md`; phải khớp với `menu.network.forgetServer` = `Quên máy chủ`. Tiêu đề xác nhận là `Quên khóa này?`.
- **`settings.servers.trustedHostKeys.confirm` → `Lần tới khi bạn kết nối tới {host}, …`** · giới từ `tới` cho MÁY CHỦ,
  theo đúng `servers.paneState.connecting` và `servers.hostKey.firstContactTitle` = `Lần đầu kết nối tới {host}`.
  `{host}` là địa chỉ người dùng tự gõ, nên câu không giả định gì về độ dài hay dạng của nó.
- **`settings.appearance.tintSmb.*` dịch lại theo bản tiếng Anh MỚI** (phủ màu nay tính cả SFTP và WebDAV, không riêng
  SMB): nhãn `Phủ màu cho khung máy chủ (SMB, SFTP, WebDAV)` giữ khuôn của hai nhãn chị em
  (`Phủ màu cho khung ổ đĩa cục bộ`, `Phủ màu cho khung MTP`), mô tả
  `Màu nền phủ lên các khung hiển thị bản chia sẻ SMB, máy chủ SFTP, hoặc máy chủ WebDAV.` giữ khuôn liệt kê của
  `settings.appearance.tintMtp.description`. `bản chia sẻ` là chữ catalog dùng cho "share".
- Ngoài `settings.section.adb`, không khóa nào trong đợt này mang `sameAsSourceJustification`, và không giá trị nào chứa
  dấu nháy đơn nên không có dấu nháy nào phải nhân đôi. Hai khóa `menu.*` thuộc họ RAW (không ICU) và cũng không có dấu
  nháy.

## Điện thoại Android qua ADB: khung kết nối, chú giải bộ chọn ổ đĩa, dòng gợi ý (`adb.*`, `settings.behavior.adbHintDismissed.*`)

Nguồn của đợt này khác thường: máy dịch (M1 agent box) không có `_ignored/i18n/vi/`, nên bằng chứng macOS lấy trực tiếp
từ bundle đang cài (macOS 26.6.2 build 25G83, quét `.loctable`, 2026-09-07), còn hai thuật ngữ của Android thì lấy từ
chính bản dịch tiếng Việt của AOSP.

- **USB debugging: `gỡ lỗi qua USB`** · AOSP `frameworks/base/packages/SettingsLib/res/values-vi/strings.xml`,
  `enable_adb` = `Gỡ lỗi qua USB` (và `enable_adb_summary` = `Bật chế độ gỡ lỗi khi kết nối USB`); hộp thoại trên máy là
  `usb_debugging_title` = `Cho phép gỡ lỗi qua USB?` (`frameworks/base/packages/SystemUI/res/values-vi/strings.xml`),
  nhánh `main`, kiểm chứng 2026-09-07. `high`. ❌ Không viết `gỡ lỗi USB`: người đọc phải tìm đúng công tắc bằng đúng
  chữ trên máy mình, và cả `settings.fileOperations.adbEnabled.description` lẫn `settings.summary.adb` đều ship dạng có
  `qua`.
- **Allow (nút trên hộp thoại của Android): `Cho phép`** · AOSP `usb_debugging_allow` = `Cho phép`. Viết hoa chữ đầu và
  không đặt trong ngoặc kép, đúng như Android tiếng Việt tự nhắc tới nút của mình
  (`Nhấn vào Cài đặt để kiểm soát dịch vụ.`). `high`.
- **tap (chạm vào một nút có tên): `nhấn vào`** · AOSP tiếng Việt dùng `Nhấn vào <Tên>` khi nhắc tới một nút hay một mục
  (`Nhấn vào Menu để được trợ giúp.`, `Nhấn vào Cài đặt…`); `chạm vào` để dành cho bề mặt phần cứng
  (`Chạm vào cảm biến`). `high`.
- **Android platform tools: `bộ công cụ nền tảng Android`** · nâng từ `tentative` lên `high`: tài liệu tiếng Việt của
  chính Google gọi "SDK Platform-Tools" là `bộ công cụ nền tảng SDK`
  (`developer.android.com/tools/releases/platform-tools?hl=vi`, kiểm chứng 2026-09-07), nên `bộ công cụ nền tảng` là cụm
  mô tả đúng; `Android` và tên lệnh `adb` giữ nguyên.
- **wake (đánh thức thiết bị đang ngủ): `đánh thức`** · macOS vi `PhotosUICore.loctable` ("everytime you wake from
  sleep" → `mỗi lần bạn đánh thức từ chế độ ngủ`), `DIErrors.loctable` ("wake failed" → `đánh thức không thành công`).
  `high`.
- **reseat the cable: `rút và cắm lại cáp`** · macOS vi ("Please unplug and replug the %@…" →
  `Vui lòng rút và cắm lại %@…`), và catalog đã ship đúng cụm ở `mtp.permissionDialog.helpText`
  (`hãy rút và cắm lại thiết bị`). `high`.
- **How (liên kết một chữ, "làm thế nào?"): `Cách làm`** · macOS vi dịch "How to X" thành `Cách X` (`How to Use` →
  `Cách sử dụng`, `How to pair` → `Cách ghép đôi`, `How to open it` → `Cách mở`), và catalog đã có
  `servers.sheet.connectionModeLegend` = `Cách kết nối`. Không nguồn nào có một chữ "How" trần, nên `Cách làm` là dạng
  ngắn nhất giữ được nghĩa. `tentative`.

Các quyết định theo từng chuỗi:

- **`adb.connect.*` đi theo giọng của `servers.refusal.*` / `servers.paneState.*`**, vì cùng một bề mặt (cả khung thay
  cho danh sách tệp). `adb.connect.timedOut` = `Điện thoại của bạn đã không phản hồi kịp thời.` dùng lại nguyên khuôn
  `servers.refusal.timedOut` (`{host} đã không phản hồi kịp thời.`).
- **`adb.connect.transport` → `Cmdr đã mất kết nối tới điện thoại của bạn.`** · luật đã ghi trong `style.md`: một cú rớt
  kết nối là `mất kết nối`, còn `ngắt` để dành cho hành động chủ ý (`Ngắt kết nối`). Không nhắc tới giao thức hay bất kỳ
  thông tin chẩn đoán nào, đúng yêu cầu của `en`.
- **`adb.connect.serverUnreachable` → `Công cụ Android trên máy Mac này đã không phản hồi.`** · `công cụ Android` là chữ
  catalog đã ship (`settings.fileOperations.adbEnabled.description`: `chưa cài công cụ Android nào`). ❌ Không nhắc tới
  máy chủ adb, transport, hay daemon.
- **`adb.readiness.noPermissions` → `Máy Mac này không truy cập được điện thoại của bạn qua USB.`** · một THIẾT BỊ USB
  đi với `truy cập` (`mtp.permissionDialog.title` = `Không thể truy cập thiết bị USB`), khác với luật
  `không kết nối được tới` dành cho MÁY CHỦ. Câu sau (`Hãy thử cáp hoặc cổng khác.`) giữ thứ tự cáp-rồi-cổng của bản
  tiếng Anh; catalog cũng đã có `Thử một cổng USB hoặc cáp khác` ở `errors.listing.deviceProblem.suggestion`.
- **`adb.hint.text` → `Muốn truy cập toàn bộ hệ thống tệp? Hãy bật Gỡ lỗi qua USB.`** · `truy cập toàn bộ hệ thống tệp`
  là nguyên văn cụm đã ship ở `settings.fileOperations.adbEnabled.description`. Viết hoa `Gỡ lỗi qua USB` ở ĐÂY vì đây
  là chuỗi duy nhất bảo người đọc đi tìm và bật đúng công tắc đó; mọi chỗ khác trong catalog viết thường
  (`đang bật gỡ lỗi qua USB`, `cho phép gỡ lỗi qua USB`), đúng như AOSP tự viết thường trong câu.
- **`adb.hint.dismiss` → `Bỏ qua`** · bắt buộc: 11 khóa khác có cùng bản tiếng Anh `Dismiss` đều là `Bỏ qua`, nên
  `i18n-terms` sẽ báo nếu lệch.
- **`adb.disconnectDeviceAriaLabel` → `Ngắt kết nối {name}`** · trùng khít
  `fileExplorer.navigation.disconnectPlaceAriaLabel`, cũng cùng bản tiếng Anh; hai nút này phải đọc y hệt nhau.
- **`adb.disconnectBusyTooltip` → `Không thể ngắt kết nối khi còn thao tác đang chạy trên thiết bị này`** · lấy khuôn
  của chuỗi chị em cùng động từ `fileExplorer.navigation.disconnectBusyTooltip` (`…trên máy chủ này`) và chỉ đổi danh
  từ. `fileExplorer.navigation.ejectBusyTooltip` dùng cùng khuôn (`Không thể tháo khi còn thao tác đang chạy trên thiết
  bị này`); ❌ đừng quay lại `trong khi có thao tác đang diễn ra`.
- **`adb.connect.openSettings` → `Mở cài đặt`** · dùng lại nguyên văn ba khóa đã ship (`commands.appSettings.label`,
  `commands.handler.openTerminalHere.openSettings`, `askCmdr.wake.needsApiKey`). Viết thường theo luật sentence case của
  tiếng Việt, dù bản tiếng Anh viết hoa `Settings`.
- **`adb.connect.waitingHint` → `Cmdr sẽ mở điện thoại ngay khi bạn nhấn.`** · bỏ `của bạn` vì dòng ngay trên đã nói
  `điện thoại của bạn`; đây là câu trấn an, không phải chỉ dẫn, nên không có `Hãy`.
- **`settings.behavior.adbHintDismissed.*` đi theo khuôn của cặp chị em `settings.behavior.serversPinHintSeen.*`**: nhãn
  `Đã bỏ qua gợi ý về gỡ lỗi qua USB`, mô tả `Dòng gợi ý một lần về việc bật gỡ lỗi qua USB đã bị bỏ qua hay chưa.` Khác
  chị em ở chỗ tiếng Anh là "dismissed" (người dùng tự tắt) chứ không phải "shown", nên động từ là `bỏ qua`, khớp với
  `adb.hint.dismiss`. Hai khóa này không bao giờ hiện trên giao diện.
- Không giá trị nào trong đợt này chứa dấu nháy đơn, nên không có dấu nháy nào phải nhân đôi. Chỉ
  `adb.volumeLabelWithSuffix` (đã dịch từ đợt trước) mang `sameAsSourceJustification`.
- **`You stopped opening your phone.` → `Bạn đã dừng việc mở điện thoại.`** (`adb.connect.cancelled`) · `dừng`, giống
  khóa song song `search.coverage.walk.cancelled` (`Bạn đã dừng lần tìm kiếm này`) và `errors.volume.cancelled`
  (`Cmdr đã dừng việc này theo yêu cầu của bạn.`) · `high`. ❌ Không dùng `hủy`: `Hủy` là NHÃN của nút
  (`fileOperations.button.cancel`), câu sẽ đọc như đang nhắc tới nút đó. Khuôn `dừng việc` + động từ đã có sẵn trong
  catalog (`dừng việc lập chỉ mục mới`). Bỏ `của bạn` vì chủ ngữ đã là `Bạn`; `mở` là động từ đã chốt cho điện thoại
  (`adb.connect.waitingHint`).

## Danh tính bị khoá của máy chủ (`servers.sheet.identityLocked`)

Hai dòng dưới hai ô đã bị làm mờ `Địa chỉ` và `Tên người dùng`, khi người dùng SỬA một máy chủ đã lưu.

- **`the account` (ô dùng để đăng nhập vào máy chủ) → `tài khoản`** · catalog đã dùng đúng nghĩa này (sáu lần trong
  `errors.json`, một lần trong `onboarding.json`) · `high`.
- **Dòng gợi ý gọi tên hành động y hệt các nút mà nó trỏ tới**: `quên` lấy từ `menu.network.forgetServer` ("Quên máy
  chủ") và `thêm` lấy từ `servers.sheet.addTitle` ("Thêm máy chủ"). Dùng từ đồng nghĩa ("xoá", "tạo") sẽ khiến người đọc
  đi tìm một mục menu không tồn tại.
- **`are what name this server` → `là những gì xác định máy chủ này`** · biểu mẫu có ô `Tên` riêng
  (`servers.sheet.name`), nên câu này không được dựa vào chữ "đặt tên": người đọc sẽ tưởng đang nói về ô nhãn đó.
  `xác định` nói đúng ý (hai giá trị đó CHÍNH LÀ máy chủ) · `high`.

## Toast khi vốn không có mật khẩu nào được lưu (`fileExplorer.navigation.forgetSecretNoneToast`)

- **`There was no saved password for {name}.` → `Không có mật khẩu đã lưu cho {name}.`** · lấy nguyên cụm
  `mật khẩu đã lưu` và `cho {name}` từ ba khóa anh em đã xuất bản (`menu.network.forgetSavedPassword` và
  `fileExplorer.navigation.forgetSecretConfirmTitle` = `Quên mật khẩu đã lưu`, `.forgetSecretConfirm`,
  `.forgetSecretRefusedToast`) · `high`.
- **`đã` trong `đã lưu` đã mang nghĩa quá khứ**, nên câu không cần thêm dấu thì nào nữa; `Không có …` là câu phủ định
  tồn tại quen thuộc của catalog (`fileOperations.trash.undoUnavailable`: `Không có gì để đưa trở lại.`).
- **`{name}` đứng sau `cho`, không cần loại từ**, nên tên máy chủ có hình dạng nào cũng đọc trôi. Không dùng chữ `lỗi`
  hay `không thể`: chẳng có gì hỏng cả.
- Kho tham chiếu không có trên máy này (`_ignored/i18n/` cũng không có trong bản clone chính), nên quyết định dựa vào
  catalog đã xuất bản và termbase này.

## Tổng thời gian thử lại, tiêu đề khóa máy chủ và nút Cho phép của Android (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `servers.paneState.hostKeyChanged`, `adb.connect.unauthorized`)

- **`{seconds}`/`{minutes}` giờ là khối số nhiều ICU với HAI chỗ giữ** (`servers.paneState.retryTotalSeconds`,
  `.retryTotalMinutes`): `{seconds}` chỉ để chọn nhánh, thứ người dùng đọc là `{secondsText}`, con số đã được định dạng
  theo ngôn ngữ. Tiếng Việt chỉ có `other` (CLDR, § style.md) nên mỗi khối chỉ một nhánh, nhưng vẫn phải giữ lớp bọc
  `{…, plural, other {…}}` để khớp chỗ giữ với bản tiếng Anh · `high`.
- **Cả hai giá trị là mảnh ghép của `servers.paneState.retryKeepsTrying`**
  (`Sẽ tiếp tục thử trong tổng cộng {duration}.`) nên để trần, không thêm giới từ hay dấu chấm. `giây` và `phút` là các
  danh từ đơn vị đã chốt trong bảng này.
- **`Cmdr won't connect to {name}` → `Cmdr sẽ không kết nối tới {name}`** · lấy đúng cụm của khóa anh em
  `servers.refusal.hostKeyRevoked` (`Cmdr sẽ không kết nối tới máy chủ đó.`). Tiếng Anh đã đổi từ “stopped connecting”
  sang lời từ chối thường trực, nên bỏ `đã dừng`, vốn nghe như một lần thử bị ngắt giữa chừng · `high`.
- **`Allow` là nút của chính Android → `Cho phép`**, chép nguyên từ `adb.connect.unauthorized`
  (`Hãy xem điện thoại của bạn rồi nhấn vào Cho phép.`), không đặt trong ngoặc kép, cùng động từ `nhấn vào`, để chữ
  trong câu trùng với chữ trên màn hình · `high`.
- Kho tham chiếu không có trên máy này (`_ignored/i18n/` cũng không có trong bản clone chính), nên quyết định dựa vào
  catalog đã xuất bản và termbase này.

## Menu chuột phải trên hàng máy chủ: `Mở` và `Sửa máy chủ…` (`menu.network.open`, `menu.network.edit`)

- **`Open` (trên một hàng máy chủ) → `Mở`** (`menu.network.open`) · giống hệt `menu.file.open`, vì cùng một nghĩa: đi
  vào bên trong một thứ, không phải giao tệp cho một ứng dụng. Tiếng Việt không tách hai nghĩa đó, macOS cũng vậy:
  Finder dùng chung một động từ cho `Mở` (`LocalizableMerged` `N151`), `Mở bằng` (`N152`) và `Mở trong cửa sổ mới`
  (`FV7`, nghĩa đi vào) (Finder 26.6.2, build 25G83, đọc ngày 2026-09-07) · `high`.
- **`Edit server…` → `Sửa máy chủ…`** (`menu.network.edit`), chép nguyên từng byte từ `commands.serversEdit.label` ·
  `high`. Hai chỗ này mở cùng một biểu mẫu; hai nhãn khác nhau sẽ đọc thành hai tính năng. Dấu ba chấm là MỘT ký tự `…`
  (U+2026) và phải giữ.
- **Cả hai sự trùng khớp đều có check canh**, không chỉ là gọn gàng: `i18n-terms` báo khi hai khóa cùng giá trị tiếng
  Anh lại lệch nhau trong tiếng Việt. Viết lại một cái thì phải kéo cái kia theo.
- **`menu.*` là họ RAW**: Rust vẽ menu qua `menu_t`, không bao giờ qua `t()`. Dấu nháy đơn giữ nguyên MỘT dấu, `''` nhân
  đôi sẽ làm `i18n-icu` hỏng. Hai giá trị này không có dấu nháy nào.
- Kho tham chiếu không có trên máy này, nhưng `Finder.app` cho đúng bằng chứng Tier 1 lấy thẳng từ hệ thống
  (`docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?").

## Function key bar context menu (`menu.context.hideFunctionKeyBar`, `fileExplorer.functionKeyBar.hiddenToast`)

- function key bar (hàng nút lệnh phím chức năng ở cuối cửa sổ) → thanh phím chức năng · đã được chốt trong danh mục
  (`settings.appearance.showFunctionKeyBar.label`); dùng lại cho mục menu ngữ cảnh và thông báo đi kèm · high

## Lời mời ghim Cmdr vào Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

Bề mặt: một thông báo một lần, hiện ra sau vài ngày dùng Cmdr, hỏi xem ứng dụng có được tự thêm biểu tượng của mình vào
Dock macOS hay không, cộng bốn câu ngắn báo kết quả sau khi người dùng đồng ý. Hai khóa `settings.*` là cờ nội bộ, người
dùng không bao giờ nhìn thấy.

Nguồn: kho tham chiếu `_ignored/i18n/vi/` (đọc được, macOS/Finder + AppKit + SystemSettings, MS terminology) cộng bằng
chứng Tier 1 lấy thẳng từ `Dock.app` đang cài (`vi.lproj/DockMenus.strings` so với `en.lproj`, qua
`plutil -convert json`), macOS 26.6.2, đọc ngày 2026-09-09.

### Thuật ngữ chốt trong đợt này

- **Dock → `Dock`, giữ nguyên tiếng Anh** · macOS vi không bao giờ dịch tên này: Finder `MenuBar` khóa `300772.title`
  (`Add to Dock` → `Thêm vào Dock`), `Dock.app/vi.lproj/DockMenus.strings` (`Dock Settings…` → `Cài đặt Dock…`,
  `Remove from Dock` → `Xóa khỏi Dock`), AppKit `Common` (`… from Dock` → `… từ Dock`). Catalog cũng đã ship
  `errors.listing.diskFullErrno.suggestion` = `… biểu tượng Thùng rác trên Dock`. ❌ Đừng lấy `thanh dock` hay `đế cắm`
  của Microsoft · `high`.
- **Finder → `Finder`, giữ nguyên** · `Dock.app/vi.lproj/Localizable.strings` khóa `FinderName` (`Finder` → `Finder`),
  AppKit `Menus` (`Show in Finder` → `Hiển thị “%@” trong Finder`), Finder `Localizable` (`Search in Finder` →
  `Tìm kiếm trong Finder`) · `high`.
- **thư mục Applications → `thư mục Ứng dụng`** · Apple DỊCH tên thư mục này: Finder `Localizable` (`Applications` →
  `Ứng dụng`), `LocalizableMerged` (`Go to Applications Folder` → `Đi tới thư mục Ứng dụng`), và AppKit `AppKitErrors`
  cho gần đúng câu của ta: `Thử kéo “%@” từ Thùng rác vào thư mục Ứng dụng của bạn.` · `high`.
- **Keep in Dock → `Giữ lại trong Dock`** · `Dock.app/vi.lproj/DockMenus.strings` khóa `KEEP_IN_DOCK` — đúng lệnh mà
  người dùng thấy khi bấm chuột phải vào một biểu tượng trong Dock, nên tiêu đề thông báo mượn nguyên chữ này · `high`.
- **configuration profile → `hồ sơ cấu hình`** (chữ thường trong câu) · SystemSettings `InfoPlist`
  (`Configuration Profile` → `Hồ sơ Cấu hình`) cộng hàng chục câu trong `ErrorStrings.loctable` /
  `authorization.prompts.loctable` viết thường trong văn xuôi (`Không thể cài đặt hồ sơ cấu hình.`,
  `… từ hồ sơ cấu hình người dùng.`) · `high`.
- **X is managed by Y → `X được Y quản lý`** · khuôn bị động chuẩn của Apple:
  `Không thể đồng bộ hóa iPad “^FILENAME” vì thiết bị này được quản trị viên quản lý.` (Localizable.loctable) · `high`.
  Tác nhân đứng GIỮA `được` và `quản lý`, đừng viết `được quản lý bởi …` cho câu ngắn.
- **log in → `đăng nhập`** · `Dock.app/DockMenus` khóa `OPEN_AT_LOGIN` (`Open at Login` → `Mở khi đăng nhập`), AppKit
  `Menus` (`Log Out` → `Đăng xuất`), MS terminology (`log in` / `log on` / `sign in` → `đăng nhập`) · `high`.
- **reload → `tải lại`** · MS terminology (`reload` → `tải lại`; biến thể `nạp lại` thua vì catalog và macOS đều dùng
  `tải`) · `high`. Khác `khởi động lại` (restart), là việc ta KHÔNG nói ở đây.
- **offer (lời mời một lần của ứng dụng) → `đề nghị`** · MS terminology (`offer` → `đề nghị`) · `high`. Chỉ dùng cho hai
  khóa nội bộ; câu người dùng đọc thì viết thẳng hành động (`Ghim nó cạnh Finder…`) chứ không gọi tên "lời mời".

### Ghi chú theo chuỗi

- **`title` → `Giữ lại Cmdr trong Dock?`** · mượn nguyên `Giữ lại trong Dock` của Dock.app rồi chèn tên sản phẩm. Bỏ
  `của bạn` theo luật trong `style.md` (đừng rải sở hữu khi quyền sở hữu đã hiển nhiên); tiêu đề vẫn là một câu hỏi
  thật, giữ dấu `?`.
- **`body` → `Bạn đã dùng Cmdr được vài ngày rồi, có vẻ nó hợp với bạn. Ghim nó cạnh Finder ở dưới đó nhé?`** ·
  `vài ngày` là cách nói mơ hồ đúng ý bản gốc; ❌ tuyệt đối không thay bằng một con số, vì ngưỡng có thể đổi. Tiếng Anh
  nối hai vế bằng "so"; tiếng Việt để dấu phẩy gánh, câu ngắn hơn và đọc tự nhiên hơn. Tiểu từ `nhé` giữ đúng giọng mời
  nhẹ nhàng, không ép.
- **`unpinNote` → `Bạn có thể bỏ ghim bất cứ lúc nào nếu đổi ý.`** · `ghim` / `bỏ ghim` là cặp đã chốt ở § "Trung tâm
  máy chủ, đợt 5", nên thân thông báo và dòng trấn an dùng chung một gốc từ, đúng như tiếng Anh ("pinned" / "unpin").
  `bất cứ lúc nào` khớp chín chuỗi đã ship trong catalog (macOS hay viết `bất kỳ lúc nào`, nhưng nhất quán catalog thắng
  ở đây). `đổi ý` đã có trong `onboarding.stepFda.postAction.body`.
- **`decline` → `Không, cảm ơn`; `accept` → `Có, thêm vào Dock`** · lời từ chối lịch sự thường ngày, và vế đồng ý mượn
  nguyên mục menu `Thêm vào Dock` của Finder. Catalog đã có khuôn `Có, …` cho nút đồng ý (`Có, tôi muốn AI`). ❌ Đừng
  dùng `Để sau` (macOS `Not Now`): Cmdr KHÔNG hỏi lại lần nữa, nên "để sau" sẽ là nói dối.
- **`addedButDockDidNotRestart` →
  `Biểu tượng Cmdr đã nằm đúng chỗ, nhưng Dock chưa tải lại. Nó sẽ xuất hiện trong lần đăng nhập tới.`** · đây là chuỗi
  dễ dịch ngược nghĩa nhất trong đợt: việc ghim ĐÃ xong, chỉ thiếu lần vẽ lại. Dùng `chưa` (chưa xảy ra, sẽ xảy ra) chứ
  không phải `không` (đã hỏng), và mở câu bằng `đã nằm đúng chỗ` để sự thật tích cực đứng trước. Không có chữ `lỗi` hay
  `thất bại` nào.
- **`managedDock` →
  `Dock của bạn được một hồ sơ cấu hình quản lý, nên Cmdr không thể tự thêm vào. Người quản lý máy Mac này có thể thay đổi điều đó.`**
  · giữ `của bạn` ở đây vì nó mang thông tin (Dock của người này, không phải Dock nói chung).
  `Người quản lý máy Mac này` dịch "whoever manages this Mac": nêu ai gỡ được hạn chế, không đổ lỗi và không bày cách
  lách.
- **`notAdded` → `Lần này Cmdr chưa vào được Dock. Bạn có thể kéo nó từ thư mục Ứng dụng vào Dock bất cứ lúc nào.`** ·
  câu thứ hai dựng theo khuôn AppKit `Thử kéo “%@” … vào thư mục Ứng dụng của bạn.`, đảo chiều nguồn/đích. Viết rõ
  `vào Dock` thay vì `vào đó` vì đích đã cách xa chủ ngữ. Lại dùng `chưa` để câu không đọc thành một lời tuyên bố hỏng.
- **`settings.behavior.dockPinNudgeOfferedAt.*` → `Đã đề nghị thêm vào Dock` /
  `Đề nghị một lần về việc thêm Cmdr vào Dock đã được đưa ra hay chưa.`** · chép đúng khuôn của cặp khóa nội bộ hàng xóm
  `settings.behavior.adbHintDismissed.*` (`Đã bỏ qua gợi ý về gỡ lỗi qua USB` /
  `Dòng gợi ý một lần … đã bị bỏ qua hay chưa.`), để hai cờ đọc như một họ.
- **Không giá trị nào có dấu nháy đơn**, nên phần nhân đôi `''` của ICU không đụng tới đợt này. Không có placeholder,
  không có `<tag>`, không có plural/select.

## Menu chuột phải trên biểu tượng Dock (`menu.dock.*`)

Bề mặt: menu bật ra khi bấm chuột phải vào biểu tượng Cmdr trong Dock macOS. Đây là menu gốc do Rust vẽ, thuộc họ RAW
(`menu.*`), nên dấu nháy đơn giữ nguyên MỘT dấu và `{name}` / `{parent}` là đích thay thế theo nghĩa đen, không phải
tham số ICU.

Nguồn Tier 1, đọc thẳng từ macOS 26.6.2 đang cài, ngày 2026-09-09:

- `Dock.app/Contents/Resources/vi.lproj/DockMenus.strings` (qua `plutil -convert json -o -`) — chính là menu này. macOS
  CÓ ship `vi.lproj` cho `Dock.app`.
- `Finder.app/Contents/Resources/{en_GB,vi}.lproj/LocalizableMerged.strings` — hai mục Go của Finder.
- `_ignored/i18n/vi/macOS/AppKit/{Common,InfoPanel}.json` — khuôn ngoặc đơn phân biệt.

### Thuật ngữ chốt trong đợt này

- **Mở + tên ỨNG DỤNG → `Mở <Tên>`, không ngoặc kép** · `DockMenus.strings` tách rõ hai dạng: tên ứng dụng đi trần
  (`HIDE_NAME` = `Ẩn %@`, `SHOW_NAME` = `Hiển thị %@`), còn tên TỆP mới được đóng ngoặc kép (`OPEN_FILENAME` =
  `Mở “%@”`). Động từ trần là `OPEN` = `Mở`. Catalog cũng đã ship đúng khuôn này (`menu.app.hide` = `Ẩn Cmdr`,
  `menu.app.quit` = `Thoát Cmdr`) · `high`. ❌ Đừng viết `Mở “Cmdr”`: đó là dạng dành cho tên tệp.
- **Go to Folder… → `Đi tới thư mục…`** · Finder `LocalizableMerged` khóa `N83` (`Go to Folder…`), khóa `GT8` cho dạng
  không có dấu ba chấm · `high`. Khác `menu.go.goToPath` (`Đi tới đường dẫn…`), vì bản tiếng Anh ở đó nói "path", không
  phải "folder"; hai khóa cố ý khác chữ.
- **Connect to Server… → `Kết nối với máy chủ…`** · Finder `LocalizableMerged` khóa `N84`, và catalog đã ship y hệt ở
  `commands.serversConnect.label` cùng `settings.network.permissionIntroConnectLink` · `high`. Giới từ là `với`, ❌
  không phải `tới` (câu văn xuôi trong `errors.*` dùng `kết nối tới`, nhưng NHÃN lấy đúng chữ của Finder).
- **Khuôn ngoặc đơn phân biệt hai hàng trùng tên → giữ nguyên `{name} ({parent})`** · AppKit vi dịch `"%@ (%@)"` thành
  `"%1$@ (%2$@)"` trong cả `Common.json` lẫn `InfoPanel.json`: cùng thứ tự, cùng dấu ngoặc, cùng khoảng trắng. Tiếng
  Việt đặt phần bổ nghĩa SAU danh từ chính y như tiếng Anh, và không nguồn nào trong kho thêm giới từ vào trong ngoặc ·
  `high`. Khóa mang `sameAsSourceJustification`; ❌ đừng "dịch" thành `{name} (trong {parent})`.

### Ghi chú theo chuỗi

- **`menu.dock.searchFiles` phải khớp TỪNG CHỮ với `menu.edit.searchFiles`** (`Tìm kiếm tệp…`). Cùng một lệnh ở hai
  menu; lệch một chữ là người dùng đọc thành hai chức năng khác nhau. `commands.searchOpen.label` là bản không dấu ba
  chấm của cùng lệnh đó.
- **Dấu ba chấm là một ký tự `…` (U+2026)**, giữ nguyên ở ba khóa có nó; ❌ không viết ba dấu chấm rời.
- **Không viết dấu gạch dưới `_` hay `&` làm phím tắt.** Trên Linux, gạch chân được cấp phát từ nhãn ĐÃ DỊCH theo từng
  menu con.
- **Không có `bạn` trong bất kỳ nhãn nào**: nhãn hành động là động từ trần, đúng luật trong `style.md` § Formality.

## Lời mời về “Hiển thị trong Finder” và thông báo lần đầu (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

Hai thời điểm của cùng một tính năng: lời mời một lần để “Hiển thị trong Finder” từ ứng dụng khác mở trong Cmdr, và
thông báo một lần vào lần đầu một yêu cầu như vậy rơi vào đây. Cả hai bề mặt đều trỏ tới lệnh của chính macOS, nên cách
dùng từ của macOS thắng (style.md § bề mặt hệ thống).

- **“Show in Finder” → `“Hiển thị trong Finder”`, dùng dấu ngoặc kép cong** · Đã chốt ở
  `settings.navigationAndFileOps.card.showInFinder` và `settings.revealHandler.description` · `high`. Các thông báo lấy
  đúng dạng đó, để thẻ cài đặt và thông báo gọi cùng một hành động bằng cùng một tên.
- **pane → `khung`** · Dạng trong catalog: `fileExplorer.doubleClickHint.body` (“nền khung”) · `high`.
- **Settings (cửa sổ riêng của Cmdr) → `Cài đặt`** · `settings.window.title` · `high`.
- **“for a while now” → `một thời gian rồi`** · Cố ý mơ hồ: ngưỡng có thể đổi, nên ❌ không bao giờ ghi con số. Cùng quy
  tắc với `main.dockPinNudge.body` (“được vài ngày rồi”) · `high`.
- **Thông báo lần đầu ❌ không phải lời xin lỗi** · Nó nói vừa xảy ra chuyện gì, vì sao, và công tắc nằm ở đâu. Vì thế
  `Cmdr được đặt để nhận những lệnh này`, ❌ không dùng “xin lỗi” · `high`.

## Phần thiết lập ban đầu: bảng kiểm, chú giải bước, tóm tắt tùy chọn (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

Bề mặt: bốn bước của trình thiết lập ban đầu. Mới hoàn toàn là chú giải hàng chấm tiến độ, nhãn trợ năng của biểu tượng
chữ i, chú giải mô hình cục bộ, cảnh báo thiếu khóa API, cả bảng kiểm bốn dòng ở bước 3 (sao GitHub, Like trên
AlternativeTo, ô email), hai câu hỏng của việc đăng ký, và bốn dòng tóm tắt một dòng ở bước 4. Viết lại:
`stepFda.ifAllow`, `stepAi.cloud.help`, `stepAi.local.label`, `stepBeta.openBeta`.

Nguồn: kho tham chiếu `_ignored/i18n/vi/` (macOS Finder/AppKit/SystemSettings, MS terminology `VIETNAMESE.tbx`, GNOME
Nautilus, Xfce Thunar, KDE Dolphin, Total Commander) cộng bằng chứng Tier 1 đọc thẳng từ macOS 26.6.2 (build 25G83) đang
cài, quét toàn bộ `*.loctable` theo công thức trong `docs/i18n/reference-pile/how-to-mine.md`, ngày 2026-09-09.

### Thuật ngữ chốt trong đợt này

- **Why? → `Tại sao?`** · Tier 1, `PhotosUICore.framework/Resources/*.loctable`, khóa tiếng Anh `Why?` → `Tại sao?`,
  macOS 26.6.2 build 25G83, quét live-bundle 2026-09-09 · `high`. Kho tham chiếu không có chuỗi nào; chỉ bản quét trực
  tiếp mới tìm ra.
- **Learn more / More about → `Tìm hiểu thêm`** · Finder `LocalizableMerged` khóa `NE115` (`Learn More`) và ba
  `.loctable` khác của hệ thống đều nhất trí · `high`. `onboarding.moreAbout` viết `Tìm hiểu thêm về {topic}`; `{topic}`
  là một nhãn ĐÃ DỊCH, và tiếng Việt không biến hình nên nhãn giữ nguyên từng chữ sau `về` (đúng luật WCAG 2.5.3 dù khóa
  này không thuộc cặp `*Aria`). ❌ Không lấy `Thông tin khác` (macOS dùng cho "More Info", một nút mở panel chi tiết).
- **Save (nút) → `Lưu`** · macOS AppKit + `ActionKit.framework` + `PhotosUICore` (`SAVE` → `LƯU`), và Total Commander vi
  cũng `Lưu` · `high`. Ba khóa phải khớp: nút `checklist.emailSave` và hai câu `signup.rejected` / `signup.unreachable`
  nhắc lại tên nút đó.
- **Email address → `địa chỉ email`** · Tier 1, `AddressBookCore.framework` (`Email address` → `Địa chỉ email`) và
  AppKit `TextFinder` (`Email Address` → `Địa chỉ Email`) · `high`. Cmdr viết thường theo luật sentence case; catalog đã
  dùng `email` làm từ mượn ở `settings.updates.attachEmailToReports.*` và `onboarding.stepBeta.emailNote`.
- **star (động từ, của GitHub) → `gắn sao`; a star (danh từ) → `sao`** · GitHub KHÔNG có giao diện tiếng Việt (danh sách
  ngôn ngữ của GitHub chỉ có en, zh-CN, zh-TW, fr, de, ja, ko, pt-BR, ru, es), nên không có "chữ của chính GitHub" để
  lấy. Chốt theo hai đường: (1) catalog đã ship `onboarding.stepBeta.checklist.star` = `Gắn sao cho repo trên GitHub`;
  (2) GNOME Nautilus vi dịch chính khái niệm này là `Sao` / `đánh sao` (`Star` → `Sao`, `starred` → `đã đánh sao`), tức
  cùng một gốc từ `sao` · `high` (thống nhất catalog + gốc từ chung). Ghi lại biến thể `đánh sao` của Nautilus; ❌ đừng
  đổi sang nó, hai khóa mới phải đọc y như câu đã ship.
- **repo → `repo`, giữ nguyên** · catalog đã giữ từ mượn ở khắp `errors.git.*` (`Không có git repo ở đây`,
  `Repo này trông như đã hỏng`) và ở `onboarding.stepBeta.checklist.star` · `high`. MS terminology dịch `repository` là
  `kho lưu trữ`, nhưng tiếng Anh của Cmdr viết tắt "repo" và người dùng Việt trong giới lập trình cũng nói "repo"; chỉ
  dùng `kho` khi tiếng Anh viết đủ chữ "repository" (`settings.fileExplorer.git.showRepoChip.*` = `Huy hiệu kho`).
- **Like (nút của một trang web) → `Thích`** · không nguồn Tier 1 nào có nhãn này (AlternativeTo cũng chỉ có tiếng Anh),
  nhưng catalog đã ship gốc từ ở `fileExplorer.doubleClickHint.iLikeIt` (`Tôi thích`) và
  `fileExplorer.doubleClickHint.dontLikeIt` (`Không thích à?`), và `Thích` là chữ mọi mạng xã hội tiếng Việt dùng cho
  nút này · `high`.
- **checklist → `danh sách kiểm tra`** · MS terminology, hai mục Office/Kaizala đều `Danh sách kiểm tra` · `high`. Bản
  quét macOS live có một bundle để nguyên `Checklist`, nhưng đó là tên mẫu trong Reminders, không phải thuật ngữ chung.
- **mailing list → `danh sách gửi thư`** · MS terminology (`mailing list` → `danh sách gửi thư`, định nghĩa "A list of
  names and email addresses…") · `high`.
- **Local Network (tên quyền của macOS) → `Mạng cục bộ`; câu văn xuôi → `Truy cập mạng cục bộ`** · Tier 1, ba bundle
  nhất trí (`AppSystemSettingsUI.framework` và hai `.loctable` khác, khóa `Local Network` / `LOCAL_NETWORK`), cùng
  `TCC.framework` (`…devices on your local network` → `…thiết bị trên mạng cục bộ của bạn`), macOS 26.6.2 build 25G83,
  2026-09-09 · `high`. Cả `networking.summary` lẫn `networking.desc` đều trích nguyên nhãn `Mạng cục bộ` của Apple; ❌
  đừng diễn giải thành `Truy cập mạng cục bộ`, vì đó không phải cái tên người dùng tìm thấy trong Cài đặt hệ thống.
- **native (của hệ điều hành) → `gốc`** · `style.md` § Notes and decisions: đã chốt `menu gốc` cho native menu;
  `mtp.summary` viết `trình xử lý gốc của macOS` · `high`.
- **warning → `cảnh báo`** · Xfce Thunar và KDE Dolphin đều `Cảnh báo` · `high`.

### Ghi chú theo chuỗi

- **Chú giải hàng chấm lấy đúng khuôn của `wizard.stepProgress`.** `Bước {step} trên {mandatory}+1`; `+1` là chữ đen
  đúng nghĩa, ❌ đừng gộp thành `4`. Ba nhánh `select` giữ nguyên tên nhánh (`remaining` / `last` / `other`), nhánh
  `other` để rỗng. `vi` chỉ có phạm trù số `other`, nhưng đây là `select`, không phải `plural`, nên tên nhánh không đổi.
- **`stepAi.local.tooltip` phải nhắc lại nhãn của lựa chọn đám mây từng chữ.** `<strong>Có, tôi muốn AI</strong>` chính
  là `stepAi.cloud.label`; sửa một cái là phải sửa cái kia. Ba dòng `\n` là ba câu, giữ đúng một câu một dòng.
- **"dumber" dịch nhẹ đi một nấc: `kém thông minh hơn hẳn`.** Tiếng Anh cố ý nói thẳng ("dumber"); `dốt hơn` trong tiếng
  Việt là chữ dành cho NGƯỜI và đọc như xúc phạm, nên chọn cụm mô tả năng lực. Giọng thành thật vẫn còn, chỉ bớt phần
  thô.
- **`<field></field>` là một cái ô nằm GIỮA câu.** Tiếng Việt đặt tân ngữ sau động từ y như tiếng Anh, nên ô ở đúng chỗ
  cũ: `Nhập địa chỉ email của bạn <field></field> để nhận…`. Lời hứa "rất hiếm khi" dời ra cuối câu
  (`, rất thi thoảng thôi`) cho tự nhiên; `thi thoảng` là chữ khóa anh em `stepBeta.emailNote` đã dùng.
- **`checklist.emailMark` là tên trợ năng của một dấu tích trạng thái, nên viết ở dạng `Đã + động từ`**:
  `Đã lưu địa chỉ email`, cùng khuôn với `errorReporter.autoSentToast.title` (`Đã gửi báo cáo trục trặc`).
- **Hai câu hỏng của việc đăng ký theo đúng hai luật đã có.** "Reach" một MÁY CHỦ là `không kết nối được tới`
  (`style.md` § "Reach" có hai lối), và "at any time" là `bất cứ lúc nào`. Đường dẫn cài đặt lấy nguyên chữ của
  `settings.section.updatesAndPrivacy`: `Cài đặt › Cập nhật & quyền riêng tư`, giữ ký tự `›`.
- **Bốn dòng tóm tắt ở bước 4 phải NGẮN và không được xuống dòng**, đồng thời dùng lại thuật ngữ của khóa `…desc` anh
  em: `kích cỡ thư mục`, `khởi động ứng dụng`, `Truy cập mạng cục bộ`, `điện thoại Android`. Nhớ luật **show →
  `hiển thị`, không phải `hiện`** ở `indexing.summary`.
- **`stepFda.ifAllow` đổi hẳn nghĩa**: tiếng Anh từ "If you decide to allow:" thành "Three easy steps:", nên bản cũ
  `Nếu bạn quyết định cho phép:` sai hẳn. Nay là `Ba bước đơn giản:`, đứng ngay trên ba khóa `stepFda.step1..3`.
- **`stepBeta.openBeta` và `stepBeta.feedbackIntro` cố ý khác một động từ.** Tiếng Anh: "helps me fix bugs" so với
  "helps me spot bugs". Tiếng Việt giữ đúng khoảng cách đó: `giúp mình sửa lỗi` so với `giúp mình phát hiện lỗi`. ❌
  Đừng gộp hai câu làm một.
- **Giọng ngôi thứ nhất của David là `mình`** ở cả bước 3 (`stepBeta.greeting`, `.feedback.call`, `.star`), nên câu mới
  trong `openBeta` cũng viết `mình`, không phải `tôi`. `tôi` chỉ dùng khi NGƯỜI DÙNG là người nói (`stepAi.*.label`).
- **Dấu nháy kép: theo khóa anh em trong cùng tệp, tức nháy thẳng `"`.** `style.md` khuyên nháy cong `“…”` cho văn xuôi
  nói chung, nhưng cả `onboarding.json` (`stepFda.step2.tip`, `networking.desc`, `stepAi.table.*`) đã dùng nháy thẳng
  theo đúng bản tiếng Anh; hai khóa mới (`alternativeToNote`, `networking.summary`) theo tệp, không theo luật chung.
- **released copy / Dev and test builds: `bản phát hành chính thức` / `các bản dựng để phát triển và thử nghiệm`**
  (`settings.revealHandler.notProductionBuild`) · Microsoft terminology (`VIETNAMESE.tbx`: `release` → `bản phát hành`,
  `build` → `bản dựng`). "come and go" viết thành `chỉ tồn tại tạm thời`, vì `đến rồi đi` đọc như dịch từng chữ. Câu thứ
  hai theo khuôn `settings.revealHandler.notInApplications` (`… mọi lần bấm “Hiển thị trong Finder” trỏ vào chỗ trống`),
  nhưng viết `xóa` theo `style.md` thay vì `xoá` của khóa anh em · `high` (thuật ngữ), `tentative`
  (`chỉ tồn tại tạm thời`).

## Trình xem tải tệp từ điện thoại, máy chủ hoặc tệp nén (`viewer.pull.*`, `viewer.error.stoppedResponding`)

Bề mặt: khung giữa cửa sổ trình xem khi Cmdr chép một tệp ở xa vào tệp tạm trước khi hiển thị (tiêu đề, thanh tiến
trình, dòng "x trên y", nút Hủy), và thông báo khi không có dữ liệu nào tới trong khoảng 45 giây. Bằng chứng Tier 1 đọc
thẳng từ macOS 26.6.2 (build 25G83), 2026-09-10.

- **"Fetching" (chép tệp về để xem trước) → `Đang tải`** · chính động từ của trình xem (`viewer.loading` =
  `Đang tải...`) · `high`. ❌ Không lấy `Đang tìm nạp…` của Finder (`IN_MD1`): đó là nghĩa lấy siêu dữ liệu trong cửa sổ
  Lấy thông tin, đọc rất kỹ thuật. ❌ Không lấy `Đang tải về` (Foundation `Progress.loctable` "Downloading"): tệp trong
  một tệp nén thì không "tải về" từ đâu cả. `{fileName}` đi trần giữa câu như `downloads.notification.title`
  (`Đã tải về {fileName}`) và Quick Look (`Preview of %@` → `Bản xem trước của %@`).
- **"{done} of {total}" (dòng dung lượng dưới thanh tiến trình) → `{doneText} trên {totalText}`** · Foundation
  `Progress.loctable`, khóa `%@ of %@` → `%1$@ trên %2$@`, đúng dòng "12,4 MB of 250 MB" của `NSProgress` · `high`. Khớp
  `settings.mediaIndex.progress.ofTotal` và `onboarding.wizard.stepProgress`. Finder (`PW3` `^0 / ^1 – ^2`) và
  `ai.toast.progress` dùng `/`, nhưng đó là những dòng dày đặc có thêm tốc độ/thời gian; dòng này đứng riêng.
- **"{done} so far" (dung lượng khi chưa biết tổng) → `Đến giờ đã tải {doneText}`** · `so far` → `đến giờ` đã chốt, và
  khuôn đặt `Đến giờ` lên đầu là của bộ đếm trực tiếp `queryUi.results.live.matchesSoFar`
  (`Đến giờ có {countText} kết quả`) · `high`. Cần thêm động từ `đã tải` (như `có` ở câu anh em), vì `12,4 MB đến giờ`
  trơ trọi không thành câu.
- **"stopped arriving" → `đã ngừng tải`** · cùng gốc `tải` với tiêu đề; `ngừng` là chữ catalog dùng cho một thứ tự dừng
  ngoài ý muốn (`search.coverage.walk.abandoned` = `Vài thư mục ngừng phản hồi`) · `high`. Vế sau lấy nguyên khuôn
  `errors.listing.couldntReadUnknown.suggestion` (`Kiểm tra xem … có còn kết nối không`) và `rồi` của
  `viewer.error.tooLargeToPreview`.

## Chỉ mục lỗi thời của điện thoại qua ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`, `.bodyPhone`)

Bề mặt: chú giải khi rê chuột lên chấm vàng của chỉ mục và hộp thoại giải thích một lần, cho điện thoại Android qua gỡ
lỗi qua USB. Điện thoại vẫn đang cắm; chỉ mục đọc là lỗi thời vì điện thoại không bao giờ báo thay đổi của nó. Đây là
bản "điện thoại" của ba khóa chị em `tooltipStale` / `staleDialog.title` / `staleDialog.body`, nên ❌ không được nhắc
tới `ngắt kết nối`.

- **phone: `điện thoại`** · catalog đã ship ở mọi khóa ADB (`adb.connect.*`, `adb.readiness.*`, `settings.adb.status.*`)
  và `errors.listing.notConnected.*` · `high`. "the yellow status next to the phone" đổi đúng một danh từ so với chị em:
  `Trạng thái màu vàng bên cạnh điện thoại`.
- **keep the index current (with the changes Cmdr makes):
  `luôn cập nhật chỉ mục … theo những thay đổi do Cmdr thực hiện`** · `cập nhật chỉ mục` là của macOS Finder
  (`LocalizableMerged` `PW39` = `Đang cập nhật chỉ mục thẻ.`) và catalog (`fileExplorer.dirSize.updatingIndexTooltip`);
  `thay đổi … được thực hiện` đã ship ở `errors.json` và `operationLog.json`
  (`những thay đổi thực hiện bên trong một tệp nén`) · `high`. Trong thân hộp thoại viết `chỉ mục của điện thoại`, ❌
  không phải `chỉ mục của nó`: câu có hai chủ ngữ ({name} và Cmdr), nên `nó` đọc mơ hồ.
- **show up (after a rescan): `xuất hiện`** · khuôn gần nhất trong catalog là `whatsNew.dialog.empty`
  (`Các thay đổi mới sẽ xuất hiện ở đây sau một bản cập nhật.`) · `high`. "after a rescan" → `sau một lần quét lại`,
  cùng cụm `một lần quét lại` của `indexing.staleDialog.body`; dùng giống nhau ở cả chú giải lẫn hộp thoại.
- **"stays as a reminder" → `vẫn hiện ra để nhắc bạn điều này`** · `hiện ra` lấy từ câu chị em
  (`… luôn hiện ra khi điều này xảy ra`), `vẫn` giữ nghĩa "stays" · `tentative` (không nguồn nào có câu này; lựa chọn
  theo chị em).
- Tên điện thoại `{name}` đứng đầu câu (`{name} không báo cho Cmdr biết…`): tiếng Việt không biến hình nên tên nào cũng
  vừa, kể cả `Pixel 9 Pro XL`. Không giá trị nào chứa dấu nháy đơn.

## Thư mục gốc và thư mục bắt đầu của máy chủ đã lưu (`servers.sheet.rootFolder*`, `servers.sheet.startFolder*`, `servers.sheet.nameHelp`, `servers.refusal.startFolderOutsideRoot`, `.rootNotFound`, `.startFolderNotFound`, `.saveUnconfirmed`)

Bề mặt: biểu mẫu thêm hoặc sửa một máy chủ SFTP / WebDAV đã lưu. Thư mục gốc là TRẦN: Cmdr không bao giờ đi lên cao hơn
nó. Thư mục bắt đầu là nơi khung mở ra, và phải là thư mục gốc hoặc nằm bên trong nó; để trống nghĩa là thư mục gốc.

- **root folder (trần của một máy chủ đã lưu) → `thư mục gốc`** · MS terminology `VIETNAMESE.tbx` (`root folder` /
  `root directory` → `thư mục gốc`), Xfce Thunar (`The root folder has no parent` → `Thư mục gốc không có thư mục cha`),
  và catalog đã ship ở `errors.mutation.cantRenameVolumeRoot` · `high`. Cùng một chữ ở nhãn, dòng gợi ý và các câu từ
  chối (`en` `@key.description` yêu cầu vậy).
- **start folder → `thư mục bắt đầu`** · kho tham chiếu không có "start folder" hay "initial folder" nào (Nautilus,
  Thunar, Dolphin, Total Commander, MS terminology đều trống). macOS vi dịch bổ ngữ "start" đúng khuôn này:
  `WorkflowKit.framework` (`Start Location` → `Vị trí bắt đầu`) và `ActionKit.framework` (`Start Location Not Found` →
  `Không tìm thấy vị trí bắt đầu`), macOS 26.6.2 build 25G83, quét `.loctable` trực tiếp 2026-09-11 · `tentative` (gốc
  từ chắc chắn, nhưng nguồn là nghĩa lộ trình của Shortcuts, không phải thư mục). ❌ Không dùng `thư mục khởi động`:
  macOS dành `khởi động` cho startup (`Startup Disk` → `Ổ đĩa khởi động`). ❌ Không dùng `thư mục mặc định` (MS
  `default folder`), nghĩa khác.
- **host trong `nameHelp` → `địa chỉ`** · `host` và `server` đều là `máy chủ` (`host` và `server` trong `terms.json`), nên dịch sát "by its
  account and host" thành `máy chủ này theo tài khoản và máy chủ` đọc lẫn lộn. `địa chỉ` là nhãn của chính ô đó
  (`servers.sheet.address`) và là chữ `servers.sheet.identityLocked` đã dùng cho cặp `Địa chỉ và tài khoản` · `high`.
- **"goes above this folder" → `đi lên cao hơn thư mục này`** · cùng gốc `lên` với `lên thư mục cha` đã ship
  (`Bấm đúp vào nền khung để lên thư mục cha`). Đặt `Trên máy chủ,` lên đầu câu để không lặp `trên` hai lần.
- **"Where the server opens." → `Thư mục hiện ra đầu tiên khi bạn mở máy chủ.`** · dịch sát `Nơi máy chủ mở ra` đọc như
  máy chủ đang khởi động; câu mô tả thẳng thứ người dùng thấy. `mở` là động từ đã chốt cho một hàng máy chủ
  (`menu.network.open`) · `tentative` (không nguồn nào có câu này).
- **Hai câu `…NotFound` dùng chung đuôi từng chữ**:
  `Hãy kiểm tra xem thư mục có tồn tại không và tài khoản của bạn có đọc được không.` Chỉ vế đầu khác (`thư mục này` /
  `thư mục bắt đầu này`), theo luật chuỗi chị em trong `style.md`. `không mở được` theo khuôn `không kết nối được tới`
  của `servers.refusal.unreachable`.
- **`saveUnconfirmed` → `{host} đã không phản hồi kịp thời, nên Cmdr chưa lưu gì. Hãy thử lại sau giây lát.`** · vế đầu
  chép nguyên `servers.refusal.timedOut`; `Hãy thử lại sau giây lát.` là câu catalog đã ship nhiều lần. `chưa` thay vì
  `không`, vì thử lại là an toàn và sẽ lưu (cùng luật `chưa` / `không` của § Lời mời ghim Cmdr vào Dock). Câu chủ động
  với `Cmdr` làm chủ ngữ thay cho bị động của tiếng Anh.
- **"Leave it empty" → `Để trống để …`** · `để trống` đã chốt trong `style.md`; khuôn câu y như
  `settings.askCmdr.interactiveModel.description` (`Để trống để dùng chung mô hình…`).
- Không giá trị tiếng Việt nào có dấu nháy đơn, nên không có `''` nào phải nhân đôi; `{host}` giữ nguyên.

## Vì sao mục chia sẻ không gắn kết được hoặc danh sách không tải được (`errors.mount.*`, `errors.shareList.*`)

Các câu dưới tiêu đề `Không thể gắn kết mục chia sẻ` (`fileExplorer.networkMount.mountFailedTitle`) và
`Không thể kết nối tới {hostName}` (`fileExplorer.network.share.connectFailedTitle`), cùng các toast
`fileExplorer.pane.directConnectionShareGoneToast`, `fileExplorer.pane.directConnectionMountNotRespondingToast`,
`fileExplorer.pane.directConnectionNotNetworkShareToast` và `servers.refusal.accountNotPermitted`. Tier 1 lấy từ bundle
ĐANG CÀI `NetAuthAgent.app/Contents/Resources/Localizable.loctable` (macOS 26.6.2, 25G83, 2026-09-11): nó viết đúng các
trường hợp này cho "Kết nối với máy chủ", và kho tham chiếu không có nó.

- **share → `mục chia sẻ`** · các chuỗi chị em (`fileExplorer.networkMount.mountFailedTitle`,
  `fileExplorer.pane.directConnectionUnreachableToast`) · `high`
- **guest → `khách`** · NetAuthAgent `EINFO_NO_ACCESS_GUEST` (`Máy chủ này không cho phép Khách truy cập.`) · `high`
- **reach a server → `không kết nối được tới`** · `style.md` § "Reach" có hai lối, `servers.refusal.unreachable` ·
  `high`
- **didn't answer in time → `đã không phản hồi kịp thời`**, **isn't responding → `không phản hồi`** ·
  `servers.refusal.timedOut`, `adb.readiness.offline` · `high`
- **SMB 2 or later → `SMB 2 trở lên`** · khuôn của `settings.ai.tooltipLocalDisabled` (`M1 trở lên`) · `high`
- **Go back (về màn hình trước) → `quay lại`** · `style.md` § "Back" · `high`
- **package → `gói`** · GNOME Nautilus (`bộ cài đặt gói`), `licensing.acknowledgements.npmHeading` (`Gói npm`) · `high`.
  **distribution (Linux) → `bản phân phối`** · không nguồn nào có nghĩa Linux · `tentative`
- **Something went wrong → `Có gì đó không ổn`** · `errors.mutation.unexpected` · `high`
- **Try again in a moment → `Hãy thử lại sau giây lát.`** · `errors.volume.deletePending` · `high`
- Cùng tiếng Anh thì cùng tiếng Việt: `errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `errors.mount.authFailed` / `errors.shareList.authFailed`. Dấu ngoặc kép thẳng, như `errors.volume.permissionDenied`.

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- **text editor → `trình soạn thảo văn bản`** · thuật ngữ Microsoft (`text editor` → `trình soạn thảo văn bản`) ·
  `high`. Là loại ứng dụng, ❌ không phải ứng dụng TextEdit của Apple (tên đó nằm trong `{app}`).
- **default text editor → `trình soạn thảo văn bản mặc định`** · danh mục (`commands.fileEdit.label` "Chỉnh sửa bằng
  trình soạn thảo mặc định") · `high`.
- **Edit files in [app] → `Chỉnh sửa tệp bằng`** · câu nối tiếp vào menu thả xuống, `bằng` như `commands.fileEdit.label`
  · `high`.
- Dismiss và Open settings giống `commands.handler.openTerminalHere.dismiss` /
  `commands.handler.openTerminalHere.openSettings`.
- **system default → `Mặc định hệ thống`** · danh mục (`settings.appearance.language.opt.system`,
  `settings.appearance.dateTimeFormat.opt.system`) · `high`. `settings.behavior.textEditorApp.systemDefault` đặt tên ứng
  dụng trong ngoặc phía sau, như `settings.appearance.language.opt.systemWithLanguage`.
- "Choose an app…" và "Checking your apps…" giống `settings.behavior.openTerminalHereApp.chooseApp` /
  `settings.behavior.openTerminalHereApp.checking`. Gợi ý (`fileExplorer.edit.hint`) theo mẫu
  `commands.handler.openTerminalHere.hint` nhưng không nêu vị trí trong Cài đặt, vì nút của nó dẫn thẳng tới đó.

## A drive leaving mid-request (`fileExplorer.navigation.driveIndex.driveLeaving`)

- **is being disconnected (đang tháo hoặc tháo gắn kết) → `đang được ngắt kết nối`** · dùng `ngắt kết nối` đã chốt, thêm
  `đang` như Thunar `vi` (“Đang tháo gắn kết thiết bị” / “Đang ngắt thiết bị”) · `high`. Chọn `được` thay vì `bị`
  (`fileExplorer.navigation.driveIndex.tooltipStale` dùng `bị` cho ổ đĩa rơi mất) vì ở đây việc tháo là chủ ý của người
  dùng. “Left its index as it was” → “để nguyên”, như `operationLog.rollback.partiallyRolledBackNotice`; “try again in a
  moment” → “sau giây lát”, như `errors.eject.notResponding`.

## A drive unplugged mid-index (`indexing.needsFreshScan.afterDisconnect`)

- **was disconnected (đã xảy ra, ổ đĩa không còn đó) → `bị ngắt kết nối`** · dùng `ngắt kết nối` đã chốt với `bị`, như
  `indexing.staleDialog.body` (“Trong khi {name} bị ngắt kết nối”) · `high`. Chọn `bị` chứ không phải `được` của
  `fileExplorer.navigation.driveIndex.driveLeaving`: ở đây ổ đĩa rơi mất giữa chừng, chứ không phải người dùng chủ ý
  tháo. “Starts from scratch” → “bắt đầu lại từ đầu”, như `indexing.rescan.incompletePreviousScan`; `lần quét` và
  `kích cỡ thư mục` lấy từ cùng họ khóa.

## A drive pulled mid-transfer (`errors.write.deviceDisconnected.sided.destination.copy`)

Bốn câu cho cùng một hộp thoại dưới tiêu đề đã có `errors.write.deviceDisconnected.title` (`Thiết bị đã ngắt kết nối`).
Người đọc vừa bị rút ổ đĩa giữa chừng, nên cả bốn câu tồn tại để nói tệp của họ ĐANG ở đâu và không mất gì. Dùng lại từ
đã chốt: disconnect → `ngắt kết nối`, drive/volume → `ổ đĩa`, copy/move → `sao chép`/`di chuyển`, destination → `đích`,
tệp → `tệp`.

- **was disconnected (ổ đĩa bị rút, ngoài ý muốn) → `bị ngắt kết nối`** · giống hệt
  `indexing.needsFreshScan.afterDisconnect` ở mục trên · `high`. Chọn `bị` chứ không phải `được` của
  `fileExplorer.navigation.driveIndex.driveLeaving`: tám khóa này đều là ổ đĩa rơi mất giữa chừng, không phải người dùng
  chủ ý tháo.
- **"{done} of {total} files" → `{done} trên {total} tệp`** · khuôn của danh mục
  (`fileExplorer.imageIndex.folder.someIndexed` "Đã lập chỉ mục {doneText} trên {totalText} hình ảnh",
  `indexing.enrich.progress`, `askCmdr.renameReview.coverage`) · `high`. macOS Finder `vi` Tier 1 rút gọn thành dấu gạch
  chéo (`PW8` "Copied: ^0 of ^1" → `Đã sao chép: ^0 / ^1`, `PW35`, `SB18`, kiểm chứng trên macOS 26.6.2, build 25G83,
  2026-09-16), nhưng đó là bộ đếm trong thanh tiến trình; trong câu văn xuôi thì `trên` mới đọc được. ❌ Đừng bọc
  `{done}`/`{total}` vào cú pháp số ICU: chúng đã được định dạng sẵn thành chuỗi, và `errors.*` là họ khóa RAW.
- **originals → `các bản gốc`** · macOS Finder Tier 1 (`PE113` "Keep Original" → `Giữ lại bản gốc`, `N163` "Show
  Original" → `Hiển thị bản gốc`), và danh mục đã dùng (`fileOperations.transferProgress.titleRemovingOriginals` "Đang
  xóa các bản gốc...") · `high`.
- **untouched / are still on the drive → `vẫn nguyên vẹn`** · `errors.write.destinationNotFound.message.copy` viết đúng
  lời trấn an này trong cùng hộp thoại ("Các bản gốc vẫn nguyên vẹn."), và
  `errors.write.notConnected.message.destination` cũng vậy · `high`. `nguyên vẹn` nói cả "còn đó" lẫn "không bị đụng
  vào", nên nó gánh được lời trấn an mà tiếng Anh chia làm hai vế.
- **where they were → `ở chỗ cũ`** · `errors.write.readOnlyDevice.source.suggestion` ("Các tệp gốc vẫn ở nguyên chỗ
  cũ."), `fileOperations.cancelRollback.moveAlreadyLanded` ("{countText} bản gốc vẫn nằm ở chỗ cũ.") · `high`.
- **the rest → `các tệp còn lại`** · `fileOperations.cancelRollback.stoppedDeleting` ("Các mục còn lại vẫn nằm ở đó."),
  `settings.askCmdr.memory.notAllForgotten` ("hãy xóa phần còn lại") · `high`. Ở đây là tệp chứ không phải mục, nên dùng
  `tệp` cho khớp với `{total} tệp` ngay câu trước.
- **so nothing is lost → `nên không mất gì cả`** · `fileOperations.cancelRollback.stagedLeftover.named` ("Bạn có thể xóa
  nó mà không mất gì") · `high`. Thêm `cả` để vế trấn an đứng cuối câu không bị trôi đi.
- **all your files → `mọi tệp của bạn`** · `fileOperations.transferProgress.rollbackAlreadyLandedTooltip` ("Mọi tệp đều
  đã ở đích") · `high`. ❌ Đừng dùng `toàn bộ tệp`: `ai.cloudConsent.askCmdr.contentsRule` đã dành cụm đó cho nghĩa
  "nguyên cả tệp" (whole file), nên ở đây nó sẽ đọc nhầm nghĩa.
- **on {counterpart} → `trên {counterpart}`; to it → `tới đó`** · `nằm trên ổ đĩa` theo
  `errors.listing.crossDeviceOperation.explanation`; `tới đó` theo `errors.write.destinationNotFound.message.copy` ("Thư
  mục bạn đang sao chép tới") và `fileOperations.cancelRollback.stagedLeftover.named` ("một lần truyền sau tới đó") ·
  `high`. Cả `{volumeName}` lẫn `{counterpart}` đứng trần, không loại từ đứng trước, như
  `errors.write.readOnlyDevice.source.message` ("{deviceName} chỉ đọc") và `indexing.staleDialog.body`: tên ổ đĩa là
  chuỗi tùy ý, gắn `ổ đĩa` phía trước sẽ sai khi tên đã tự chứa từ đó.
- Hai khóa `sided.source.*` chỉ khác nhau đúng một động từ (`sao chép` / `di chuyển`); phần đuôi trấn an giữ nguyên từng
  chữ, theo quy tắc "các biến thể chị em dùng chung mọi câu có thể dùng chung" trong `style.md`.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`)

Bốn khóa cho hộp thoại sau một lần di chuyển mà Cmdr không chứng minh được là các bản sao đã ghi xuống đĩa. ❌ Đừng viết
thành một lần hỏng việc: không có gì mất cả, Cmdr giữ lại các bản gốc CHÍNH VÌ nó chưa chắc chắn. Giữ nghĩa đen của
"couldn't confirm", và theo `style.md` thì không có `lỗi` hay `thất bại` nào trong câu.

- **couldn't confirm → `chưa xác nhận được`** · `fileExplorer.rename.unconfirmed` ("Chưa xác nhận được việc đổi tên
  \"{name}\"."), `fileOperations.mkdir.timeoutMessage` ("Chưa xác nhận được thư mục đã được tạo.") · `high`. Thuật ngữ
  Microsoft chốt confirm → `xác nhận` (VNM, Verb). Chọn `chưa … được` chứ không phải `không thể xác nhận` của
  `fileExplorer.pane.trashUnconfirmedToast`: `chưa` nói "chưa chứng minh xong", còn `không thể` nghe như một lời từ chối
  dứt khoát, đúng cái sắc thái hỏng việc mà khóa này cố tình tránh.
- **the move (danh từ hóa thao tác) → `việc di chuyển`** · khuôn `việc + động từ` của `fileExplorer.rename.unconfirmed`
  ("việc đổi tên") · `high`. Tiêu đề viết hoa kiểu câu, không dấu chấm cuối, như các tiêu đề `errors.write.*` khác.
- **were saved → `đã được lưu`** · `fileExplorer.pane.trashUnconfirmedToast` dùng cùng dạng bị động `đã được` cho một
  thao tác chưa xác nhận · `high`. `các tệp vừa di chuyển` (vừa = just now) tránh chồng hai chữ `đã` liền nhau mà "các
  tệp đã di chuyển đã được lưu" sẽ tạo ra.
- **it kept your originals → `nó giữ nguyên các bản gốc của bạn`** · `nó` thay cho Cmdr theo
  `fileOperations.transferProgress.rollbackAlreadyLandedTooltip` ("trước khi nó xóa những bản gốc còn lại");
  `giữ nguyên` theo `askCmdr.renameReview.nameKeptTooltip` · `high`.
- **Have a look at the destination → `Hãy kiểm tra đích`** · `kiểm tra` theo `fileExplorer.pane.trashUnconfirmedToast`
  ("Hãy kiểm tra Thùng rác để chắc chắn."); `đích` trần làm tân ngữ theo `errors.write.destinationNotFound.suggestion`
  ("Hãy chọn đích khác") · `high`.
- **try the move again → `thử lại việc di chuyển`** · `rồi thử lại` là khuôn của cả họ `errors.write.*`
  (`errors.write.deviceDisconnected.suggestion`, `errors.write.notConnected.suggestion`) · `high`. Giữ `việc di chuyển`
  để câu gợi ý nhắc lại đúng thao tác, thay vì một lời "thử lại" chung chung.
- **haven't moved → `vẫn ở nguyên chỗ cũ`** · `errors.write.readOnlyDevice.source.suggestion` · `high`. ❌ Đừng dịch sát
  thành `chưa di chuyển`: `di chuyển` là từ đã chốt cho thao tác Move, nên "các bản gốc chưa di chuyển" sẽ đọc như thể
  thao tác vẫn đang chờ chạy, chứ không phải các tệp vẫn nằm yên chỗ cũ.
- Không giá trị nào trong tám khóa có dấu nháy đơn, nên không có `''` nào phải nhân đôi; `errors.*` là họ RAW.

## Thư mục làm việc còn lại sau một lần di chuyển dang dở (`fileOperations.leftovers.stagingFolderKept`)

Toast báo tin, hiện khi cắm lại ổ đĩa (hoặc khi Cmdr khởi động) và Cmdr thấy thư mục làm việc của một lần di chuyển
không chạy xong, bên trong vẫn còn tệp. Cmdr cố ý không đụng vào chúng, vì đó có thể là bản duy nhất người dùng còn.
Giọng trấn an, kể việc, không đòi hỏi gì. ❌ Đây KHÔNG phải họ `fileOperations.cancelRollback.stagedLeftover.*`: ở đó là
phần thừa của chính Cmdr và người dùng được mời xóa, còn ở đây là tệp của người dùng và câu này **không bao giờ** được
gợi ý bỏ đi thứ gì, nên `dọn dẹp` và `xóa` đều bị cấm trong chuỗi này.

- **found → `tìm thấy`, không phải `phát hiện`** · catalog dùng `tìm thấy` cho việc Cmdr tìm rồi thấy
  (`errors.listing.symlinkLoopErrno.explanation`: "Cmdr tìm thấy một chuỗi liên kết tượng trưng vòng tròn"), và để dành
  `phát hiện` cho việc dò tự động (`settings.adb.status.watching`, `viewer.toolbar.encoding.detectedSuffix`) · `high`.
  macOS Finder có `Đã phát hiện thấy bản sao một phần của thư mục “^0”` cho đúng tình huống này, nhưng `phát hiện` nghe
  như máy dò ra một dấu hiệu, còn ở đây Cmdr chỉ đơn giản nhìn thấy tệp.
- **an unfinished move → `một lần di chuyển chưa hoàn tất`** · macOS Finder `vi` chốt đúng cụm cho một thao tác dở dang:
  `Một số thông tin đã được ghi vào đĩa này, nhưng thao tác chưa hoàn tất.`; `hoàn tất` cũng đã ở trong catalog
  (`errors.write.deviceDisconnected.sided.destination.move`: "trước khi Cmdr kịp hoàn tất việc di chuyển") · `high`. ❌
  Không mượn `chưa hoàn chỉnh` của `fileOperations.cancelRollback.stagedLeftover.named`: `hoàn chỉnh` tả một VẬT đủ đầy
  (bản sao), `hoàn tất` tả một VIỆC chạy xong, và giữ hai từ tách nhau cũng giữ hai họ chuỗi tách nhau. Khuôn
  `từ một lần <động từ> <tính từ>` lấy của `settings.advanced.showStagingTempFiles.description` ("Các tệp còn sót lại từ
  lần sao chép bị gián đoạn"); ở đây bỏ `còn sót lại` vì cụm đó đã thuộc về phần thừa của chính Cmdr.
- **left them in place → `giữ nguyên chúng ở đó`** · chọn giữa hai lựa chọn đều đúng: `giữ nguyên` là động từ catalog
  dùng khi Cmdr chủ động bảo vệ tệp của người dùng (`errors.write.moveNotConfirmed.message.named`: "nó giữ nguyên các
  bản gốc của bạn ở chỗ cũ"), còn `để nguyên` là khi Cmdr chỉ không đụng tới một thứ của chính nó
  (`fileExplorer.navigation.driveIndex.driveLeaving`: "để nguyên chỉ mục của ổ đĩa",
  `operationLog.rollback.partiallyRolledBackNotice`) · `high`. Chuỗi này thuộc vế bảo vệ, nên lấy `giữ nguyên`. ⚠️
  `ở đó`, ❌ không phải `ở chỗ cũ`: các tệp đang nằm trong thư mục làm việc chứ không phải chỗ xuất phát của chúng, nên
  `chỗ cũ` sẽ nói sai. Vế `ở nguyên chỗ` của `fileOperations.cancelRollback.leftBehind` là dạng nội động, không ghép
  được vào câu có tân ngữ này.
- **a hidden folder named X → `một thư mục ẩn có tên {folderName}`** · `ẩn` là từ của macOS Finder `vi`
  (`Hệ thống coi các mục có tên như vậy là tệp ẩn.`, `Finder ẩn các tệp bắt đầu bằng dấu chấm.`) và của catalog
  (`menu.view.showHiddenFiles` = `Hiển thị tệp ẩn`); KDE Dolphin có sẵn `thư mục ẩn`. Khuôn `có tên` lấy nguyên của
  Finder (`… đã được di chuyển vào một thư mục mới có tên “^0”`) · `high`. Giữ tên thư mục đứng trần, không ngoặc kép,
  theo tiếng Anh và theo `fileOperations.cancelRollback.stagedLeftover.named`, dù Finder có đóng ngoặc ở câu của nó. Chữ
  `ẩn` là phần duy nhất người đọc có thể làm gì đó với nó (phải bật hiển thị tệp ẩn mới thấy), nên không được lược.
- `{volumeName}` và `{folderName}` giữ nguyên từng byte, đứng trần không có loại từ đi trước (tên ổ đĩa là chuỗi tùy ý,
  theo quy tắc ở `style.md`); giá trị không có dấu nháy đơn nào nên không phát sinh `''` của ICU.

## Menu mục ưa thích (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent` / `.favoritesAlreadyAdded` / `.favoritesCantAddHere` / `.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

⌃D mở danh sách thư mục đã đánh dấu thành một menu phủ lên khung đang chọn; chín hàng đầu mang phím số 1–9 để nhảy thẳng
tới nơi đó, hàng cuối mang phím `0` và thêm thư mục hiện tại của khung vào danh sách. Bộ chọn ổ đĩa không còn mục Mục ưa
thích bên trong nữa, chỉ còn một hàng trên cùng mở menu này.

- **favorites (danh sách của Cmdr) → `mục ưa thích`** · macOS AppKit `FontManager` dịch thẳng `Favorites` thành
  `Mục ưa thích`, và catalog đã chốt từ này ở `fileExplorer.navigation.groupFavorites`,
  `fileExplorer.navigation.favoritesEmpty`, `commands.favoritesAdd.label`, `menu.go.addToFavorites` · `high`. ❌ Không
  lấy `yêu thích` của Microsoft (thuật ngữ Windows, quy tắc "ưu tiên từ của Finder"). Viết hoa theo đúng tiếng Anh: chữ
  thường trong câu (`Hiển thị mục ưa thích`), viết hoa khi đứng đầu câu hoặc là tên riêng của danh sách
  (`Mục ưa thích của bộ chọn ổ đĩa` ở `commands.favoritesAdd.description`).
- **favorites menu → `menu mục ưa thích`; làm tiêu đề nhóm thì `Menu mục ưa thích`** · Microsoft terminology giữ `menu`
  nguyên dạng (`menu` → `menu`, `submenu` → `menu con`), và `style.md` đã có luật "nhắc tới một menu thì viết
  `menu <Tên>`" · `high`. Trật tự trung tâm-trước-bổ-nghĩa-sau nên `menu mục ưa thích` đọc đúng là "menu của các mục ưa
  thích". `shortcuts.scope.favoritesMenu` = `Menu mục ưa thích` để khớp độ dài và giọng của các tiêu đề nhóm hàng xóm
  (`shortcuts.scope.volumeChooser` = `Bộ chọn ổ đĩa`).
- **show → `hiển thị`; see → `xem`** · giữ hai động từ tách nhau đúng như tiếng Anh: `commands.favoritesOpen.label` và
  `menu.go.showFavorites` (hai bề mặt của một lệnh, phải khớp từng chữ) đều là `Hiển thị mục ưa thích` theo luật "show =
  `hiển thị`, không bao giờ `hiện`" ở `style.md`; `fileExplorer.navigation.seeFavorites` là `Xem …` theo macOS Finder,
  nơi `Xem` phủ nghĩa "see/view" trong các chú giải thanh công cụ (`TL_HELP_BACK` =
  `Xem các thư mục bạn đã xem trước đây`, `TL_HELP_INFO`, `TL_HELP_PATH`) · `high`.
- **Số nhiều của `fileExplorer.navigation.seeFavorites`: chỉ `=0` + `other`** · CLDR cho `vi` chỉ có đúng một hạng
  `other` (`new Intl.PluralRules('vi')`; GNOME `nplurals=1`), vì tiếng Việt không có hình thái số · `high`. Giá trị là
  `{count, plural, =0 {Xem mục ưa thích} other {Xem {count} mục ưa thích}}`. ❌ **Đừng "khôi phục" nhánh `one` của tiếng
  Anh**: nó không phải hạng CLDR của tiếng Việt, `desktop-i18n-plural` không đòi, và một nhánh thừa chỉ tạo ra hai câu
  phải sửa song song. Nhánh `=0` cố ý không có `{count}` (đó là câu "bạn chưa có cái nào", không được hiện số 0).
- **current folder → `thư mục hiện tại`** · macOS Finder `vi` (`Search the Current Folder` →
  `Tìm kiếm trong thư mục hiện tại`, `TL_HELP_PATH` = `Xem vị trí của thư mục hiện tại`), và catalog đã dùng ở
  `commands.favoritesAdd.description` · `high`. Là thư mục KHUNG đang mở, không phải mục dưới con trỏ, nên không thêm
  `đang chọn` vào cụm này (`đang chọn` thuộc về khung: `khung đang chọn`, theo `commands.navGoToPath.description`).
- **mounted share → `mục chia sẻ đã gắn kết`** · catalog đã chốt `mục chia sẻ` cho một share (33 lần, trong đó
  `fileExplorer.network.browser.noMountedShares`, `fileExplorer.networkMount.mountFailedTitle`) và `gắn kết` cho "mount"
  (macOS AppKit `Document`: `couldn't be mounted` → `không thể gắn ổ đĩa`; GNOME
  `Danh sách các lối tắt, điểm gắn, và đánh dấu`) · `high`. ❌ Đừng viết `bản chia sẻ` (16 lần trong catalog, thiểu số)
  cho chuỗi mới, và ❌ đừng gọi tên giao thức: tiếng Anh cố ý tránh SMB/MTP/ADB ở
  `fileExplorer.navigation.favoritesCantAddHere`, tiếng Việt cũng vậy. `disk` trong cùng câu là `ổ đĩa`, khớp Microsoft
  (`drive` → `ổ đĩa`) và toàn bộ catalog.
- **press (một phím) → `nhấn`, không phải `bấm`** · catalog tách đôi: `nhấn` cho phím (`shortcuts.section.pressKeys`,
  `settings.behavior.fileSystemWatching.globalGoToLatestShortcut.enabled.description`), `bấm` cho chuột · `high`. Các
  phím `0`–`9` và ký hiệu ⌘ ⌥ ⌃ ⇧ giữ nguyên, không dịch.
- **"press a number to jump to that favorite" → `nhấn một con số để đi tới thư mục đó`** · `để đi` trơ trọi cụt nghĩa
  hẳn trong tiếng Việt, nên câu phải nói rõ đi đâu · `high`. Trước đây tiếng Anh dừng ở "to go" và bản dịch tự bù đích
  đến; nay tiếng Anh đã tự nói ra đích đó. Vẫn giữ `thư mục đó` chứ không đổi thành `mục ưa thích đó`: mỗi mục ưa thích
  chính là một thư mục, nên nghĩa y hệt, mà câu tránh được việc lặp cụm ba âm tiết `mục ưa thích` hai lần trong một câu
  vốn đã mở đầu bằng `menu mục ưa thích`. Động từ `đi tới` là của họ menu Go (`menu.bar.go` = `Đi`, `menu.go.goToPath` =
  `Đi tới đường dẫn…`, macOS Finder `Go To Location` → `Đi tới vị trí`).
- **Hàng `0` chỉ còn MỘT khóa: `fileExplorer.navigation.favoritesAddCurrent`** =
  `Thêm thư mục hiện tại vào mục ưa thích` · `high`. Danh sách phím tắt TRÍCH lại đúng hàng đó để giải thích phím `0`
  chứ không mô tả lại, nên đọc cùng một giá trị. Trước kia có hai khóa và tiếng Việt vẫn ra trùng nhau từng chữ, vì
  tiếng Anh chỉ phân biệt chúng bằng mạo từ "the", thứ tiếng Việt không có. ❌ Đừng nghĩ ra cách diễn đạt thứ hai cho
  danh sách phím tắt. Điều PHẢI giữ tách là so với lệnh thật `commands.favoritesAdd.label` = `Thêm vào mục ưa thích`
  (không nhắc thư mục nào).
- **`fileExplorer.navigation.favoritesCantAddHere` nói về CHÍNH thư mục này, lý do đặt sau dấu hai chấm** ·
  `Thư mục này không thể là mục ưa thích: mục ưa thích chỉ hoạt động trên ổ đĩa và mục chia sẻ đã gắn kết` · `high`. Mở
  đầu giống hệt hàng chị em `fileExplorer.navigation.favoritesAlreadyAdded` (`Thư mục này đã là mục ưa thích`), nên hai
  dòng xám đọc thành một cặp. ❌ Bỏ `chỉ có thể trỏ tới`: động từ trỏ buộc các ngôn ngữ biến cách phải chọn cách cho
  đích đến, còn `hoạt động trên` chỉ là một trạng ngữ nơi chốn phẳng. Không có dấu chấm cuối (đây là chú giải).
- **`commands.favoritesAdd.description` không còn nhắc "Mục ưa thích của bộ chọn ổ đĩa"** (M3 đã bỏ mục đó khỏi bộ
  chọn), mà nói thư mục thật sự đi đâu:
  `Thêm thư mục hiện tại của khung đang chọn vào mục ưa thích, để sau này quay lại đó từ menu mục ưa thích.` · `high`.
- **already → `đã`** · macOS AppKit `SavePanel` / `Document` (`already exists` → `đã tồn tại`) · `high`.
  `fileExplorer.navigation.favoritesAlreadyAdded` = `Thư mục này đã là mục ưa thích`: câu kể bình thản, không dấu chấm
  (theo tiếng Anh), không dùng `lỗi` hay `không thể`.
- Không giá trị nào trong đợt này chứa dấu nháy đơn, nên không phát sinh `''` của ICU; `menu.go.showFavorites` thuộc họ
  RAW (Rust vẽ menu gốc) và cũng không có gì phải nhân đôi.

## macOS từ chối tháo ổ đĩa, và Cmdr nói rõ ai đang giữ (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

Sáu khóa nối tiếp `errors.eject.unmountRefused` (khóa chung, khi Cmdr không đoán được ai giữ). Cả sáu đều rơi vào sau
dấu hai chấm của `Không thể tháo {volumeName}: …`, nên chỉ nêu lý do và bước tiếp theo. Cả họ dùng chung khuôn
`… vẫn đang sử dụng ổ đĩa này. Hãy … , rồi tháo lại.` để ba khóa đọc như một nhà.

- **"is still using this drive" → `vẫn đang sử dụng ổ đĩa này`, thể CHỦ ĐỘNG** · macOS AppKit `AppKitErrors` có sẵn câu
  này ở thể bị động (`The disk could not be ejected because it is in use by “%@”.` →
  `Không thể tháo ổ đĩa vì ổ đĩa này đang được “%@” sử dụng.`, kiểm chứng trên macOS 26.6.2, quét `.loctable` toàn hệ
  thống, 2026-09-16) · `high`. Chọn thể chủ động vì `{app}` đứng đầu câu và vì khóa anh em `errors.eject.unmountRefused`
  (`Vẫn còn thứ gì đó đang sử dụng ổ đĩa này`) và `errors.eject.busy` (`Cmdr vẫn đang chuyển tệp trên ổ đĩa này`) đã
  viết chủ động. Khuôn `được “%@” sử dụng` của Apple cần tên trong ngoặc kép, còn `{app}` ở đây đi trần (tên tiến trình
  là chuỗi tùy ý, như `{volumeName}`).
- **`{app}` một mình và `{apps}` nhiều tên chỉ khác nhau đúng một đại từ** · tiếng Việt không đánh dấu số ở động từ, nên
  hai câu không thể khác nhau ở `is`/`are` như tiếng Anh. Khác biệt duy nhất là `nó` (một ứng dụng) so với `chúng`
  (nhiều ứng dụng), cả hai đều là đại từ catalog đã dùng cho vật
  (`fileOperations.transferProgress.rollbackAlreadyLandedTooltip` "trước khi nó xóa những bản gốc còn lại",
  `askCmdr.renameUndo.skipReason.*` "chúng đã thay đổi") · `high`. ❌ Đừng bịa thêm dấu hiệu số (`các ứng dụng đó`,
  `những ứng dụng này`) để "cho giống tiếng Anh": danh sách tên đứng ngay trước đã nói số rồi.
- **"Close anything it has open there" → `Hãy đóng những gì nó đang mở ở đó`** · `Hãy đóng …` theo Tier 1 macOS
  DiscRecordingUI (`Other applications may be using this media. Close those applications and try again.` →
  `Các ứng dụng khác có thể đang sử dụng phương tiện này. Đóng các ứng dụng này và thử lại.`); `những gì` và `ở đó` đều
  là cách viết catalog đã ship (đoạn tính năng mới đã gỡ askCmdr.consent.whatsNew.body "những gì bạn đang duyệt",
  `fileOperations.leftovers.stagingFolderKept` "giữ nguyên chúng ở đó") · `high`. Giữ `những gì` chứ không thu hẹp thành
  `các tệp`: bản tiếng Anh cố ý nói "anything" (cửa sổ, tệp, phiên terminal).
- **"other apps" (mục cuối trong danh sách) → `các ứng dụng khác`, CÓ loại từ `các`** · macOS `SESUIServiceCore`
  (`Other Apps` → `Các ứng dụng khác`) và hàng chục chuỗi văn xuôi (`TCC.framework`: "data from other apps" →
  `dữ liệu từ các ứng dụng khác`), kiểm chứng 2026-09-16 · `high`. `Intl.ListFormat('vi')` ghép thành
  `Preview, Warp, Photos và các ứng dụng khác`; bỏ `các` thì `… và ứng dụng khác` đọc thành "và một ứng dụng khác nữa",
  tức là một cái, chứ không phải phần còn lại. Viết thường vì nó nằm giữa câu, và không thêm dấu chấm.
- **disk image → `ảnh đĩa`, và luôn nhắc lại đủ hai chữ** · Tier 1 macOS áp đảo: Disk Utility, `DiskImages.framework`,
  DiskImageMounter đều viết `ảnh đĩa` (`Disk Image` → `Ảnh đĩa`, `The disk image “%@” contains an APFS volume.` →
  `Ảnh đĩa “%@” chứa ổ đĩa APFS.`), kiểm chứng trên macOS 26.6.2, quét `.loctable`, 2026-09-16 · `high`. Kho tham chiếu
  KHÔNG có chuỗi "disk image" nào (cả macOS lẫn thuật ngữ Microsoft), phải quét hệ điều hành đã cài mới ra. ❌ Câu thứ
  hai không được rút gọn thành `tháo ảnh đó`: `ảnh` một mình là bức ảnh chụp (`askCmdr` dùng `ảnh` đúng nghĩa đó), nên
  viết `tháo ảnh đĩa đó trước`.
- **"stored on this drive" → `nằm trên ổ đĩa này`** · `errors.listing.crossDeviceOperation.explanation` ("nguồn và đích
  nằm trên các ổ đĩa khác nhau") · `high`. ❌ Không dùng `được lưu trữ trên`: `lưu trữ` đọc ra nghĩa sao lưu, trong khi
  ở đây chỉ là tệp `.dmg` tình cờ nằm ở đó.
- **"macOS is still working with this drive" → `macOS vẫn đang làm việc với ổ đĩa này`** · Tier 1 macOS có đúng cụm này
  (`Working with %@` → `Đang làm việc với %@`, `Working with Safari` → `Đang làm việc với Safari`), kiểm chứng
  2026-09-16 · `high`. Cố ý đổi động từ so với `sử dụng` của hai khóa ứng dụng: bản tiếng Anh cũng đổi, vì ở đây không
  có gì để người dùng đóng, chỉ có việc chờ.
- **"Wait a minute" → `Hãy đợi một phút`; "Wait a moment" → `Hãy đợi một chút`** · macOS dịch "a minute" là `một phút`
  (`About a minute` → `Khoảng một phút`) và "Wait a moment and try again" là `Chờ một lát rồi thử lại`; catalog đã chốt
  `Đợi một chút rồi thử lại` (`errors.write.deletePending.suggestion`) · `high`. Giữ hai độ dài tách nhau: khóa macOS
  nói chờ lâu hơn thật (Spotlight đang lập chỉ mục), khóa Cmdr chỉ là một nhịp.
- **"Cmdr itself" → `Chính Cmdr`** · `chính` là cách tiếng Việt nhấn "đúng nó chứ không phải ai khác", và câu này cố ý
  nhận lỗi về phía Cmdr · `high`.
- **"send a report" → `gửi báo cáo`, trần** · đúng nút trong ứng dụng (`errorReporter.dialog.send` và
  `crashReporter.dialog.send` đều là `Gửi báo cáo`) · `high`. ❌ Không viết `báo cáo sự cố` (đó là crash report) hay
  `báo cáo trục trặc` (tên đầy đủ của luồng error report): bản tiếng Anh cố ý nói "a report", theo đúng luật hai tên báo
  cáo ở `style.md`.
- **"if it keeps happening" → `nếu vẫn tiếp diễn`** · `errors.serverRequest.unexpected` ("Hãy thử lại, và nếu vẫn tiếp
  diễn, hãy khởi động lại Cmdr.") có y hệt khuôn "làm X, và nếu vẫn tiếp diễn thì làm Y" · `high`. Biến thể
  `Nếu vẫn cứ vậy` của `errors.listing.resourceBusy.suggestion` cũng đúng nhưng suồng sã hơn.
- `errors.*` là họ RAW: `{app}` / `{apps}` là chỗ thay chuỗi thuần, không phải cú pháp ICU, và không giá trị nào trong
  sáu khóa có dấu nháy đơn nên không có `''` nào cả.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- **kind (a row's file kind; the umbrella the three live labels generalize) → `Loại`** · macOS Finder `vi`
  `ArrangeByMenu` `119.title`/`338.title`, the Kind sort criterion · `high`.
- **"Select all with extension `*.{extension}`" → `Chọn tất cả tệp có đuôi *.{extension}`** · Total Commander
  (`WCMD.INC` `527` → `Chọn tất cả các tập tin với cùng phần mở rộng`) name this exact command, and the mask replaces
  their "same extension" because Cmdr shows the concrete one · `high`. Mặt nạ đứng sau `có đuôi`, tiếng Việt không biến
  hình nên không có gì phải hợp với `{extension}`. TC `vi` dùng `phần mở rộng`, nhưng thuật ngữ đã chốt của catalog là
  `đuôi tệp`.
- **`menu.context.selection` (a NOUN: the right-click submenu's title) → `Lựa chọn`** · the catalog's settled noun for
  the SET of selected files, from `commands.selectionSelectFiles.description` („… vào lựa chọn”) · `high`. Its siblings
  in that menu are verbs; this one names what the submenu holds. ❌ Not the verb `Chọn`, which is `menu.bar.select`.
- The four label twins (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension` against
  `commands.selectionSelectSameKind.label`/`.allFolders`/`.sameExtension`/`.noExtension`) each share ONE English string,
  so `i18n-terms` holds each pair identical. Reword neither alone. The only legitimate difference is the apostrophe:
  `menu.*` is a RAW family (single `'`), `commands.*` is ICU (doubled `''`), and the check normalizes that away.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`)

Huy hiệu cảnh báo trên thanh tiêu đề cùng tooltip của nó, hiện ra khi Cmdr chưa có quyền truy cập đầy đủ vào ổ đĩa; bấm
vào thì phần thiết lập ban đầu mở lại ở bước 1.

- **`onboarding.fdaBadge.label` → `Không có quyền truy cập đầy đủ vào ổ đĩa`** · Apple's own Cài đặt Hệ thống row (see
  the full-disk-access entry above for the live-bundle evidence) · high. Long for a pill, but the badge's whole job is
  to be findable in System Settings, and that is the row's actual wording. ❌ Not `Chưa có …`: this is a flat present
  state, not an unfulfilled expectation (same ruling as "no drive is indexed").
- **`onboarding.stepAi.bannerTitle.denied` carries the SAME English string** ("No full disk access"), so `i18n-terms`
  holds the two identical. It was moved from `Không có quyền truy cập toàn bộ đĩa` onto the pane name in the same pass.
  ❌ Reword neither alone.
- **`onboarding.fdaBadge.ariaLabel` opens with the label verbatim**
  (`Không có quyền truy cập đầy đủ vào ổ đĩa. Mở bước quyền truy cập đầy đủ vào ổ đĩa trong phần thiết lập ban đầu.`),
  which is what satisfies `i18n-aria` (WCAG 2.5.3). ❌ Re-wording the label alone breaks it.
- **Tooltip terms**: drive → `ổ đĩa` (settled), cloud folders → `thư mục đám mây`, file → `tệp` (settled), "files macOS
  keeps to itself" → `những tệp mà macOS giữ riêng cho mình` (plain, ❌ never a macOS feature name), "Click to …" →
  `Bấm để …` (`bấm`, ❌ never `nhấp`; as in `fileExplorer.breadcrumb.navigateTooltip`), onboarding → `thiết lập ban đầu`
  (settled).
- **Name vs prose, the boundary**: `quyền truy cập đầy đủ vào ổ đĩa` where a string NAMES the setting. The FDA step's
  running prose (`onboarding.stepFda.revoked.noAccess` and its siblings) still says `truy cập toàn bộ đĩa`. Open
  decision: whether a later pass sweeps the prose onto the pane name too.

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

macOS turned down a move to the trash, and Cmdr now words the refusal three ways (permission-shaped, no trash at that
location, unclassified) instead of one sentence. RAW family, so single apostrophes and `{count}` is a literal
replacement target. Four rules bind this whole group:

- **The title is NOT a free choice.** `errors.write.fallback.title.trash`, `errors.write.ioError.title.trash`,
  `errors.write.readError.title.trash`, and `errors.write.writeError.title.trash` carry the same English, so
  `i18n-terms` holds all five identical. ❌ Reword one and you have to reword all five.
- **No plural machinery**, so every `message.*` has to read correctly at `{count}` = 1 as well as 7. Vietnamese has no
  number agreement at all, so `{count} trong số các mục bạn đã chọn` just works.
- **❌ Never "try again" in a suggestion.** Retrying a permission refusal reproduces it exactly; that advice is what the
  original bug report came back calling useless. Say what the user CAN do instead.
- **`suggestion.other` must reuse the disclosure label** `fileOperations.errorDialog.technicalDetails`
  (`Chi tiết kỹ thuật`), because it points at that very control.

- **locked → `bị khóa`** · macOS Finder (`AXNODE1` `Đã khóa`) and the settled catalog term · high.
- **"delete them permanently" → `xóa vĩnh viễn`** · matches `commands.fileDeletePermanently.label`, so the suggestion
  names the command the user will run · high.
- **badge (the title-bar pill) → `huy hiệu`** · tentative; descriptive, no pile term for this shape. title bar →
  `thanh tiêu đề` · high.
- The quoted badge text is `onboarding.fdaBadge.label` verbatim, in the catalog's `“…”` quotes.
- **trash stays lowercase `thùng rác` here**, matching the four sibling titles this key is locked to; the older
  `errors.mutation.trashRefused` capitalizes it because it names the Trash location ("to the Trash").
- "somewhere macOS keeps to itself" → `nơi mà macOS giữ riêng cho mình`, reusing the wording settled for
  `onboarding.fdaBadge.tooltip`. Plain, ❌ never a macOS feature name.

## Cảnh báo nội dung chỉ có trực tuyến (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

Nếu một mục được chọn trong thư mục đám mây chỉ có trực tuyến, thùng rác sẽ phải tải nó về trước. Vì vậy Cmdr mở hộp
thoại xóa vĩnh viễn và giải thích điều đó trong dải cảnh báo. Hai biến thể của dải cảnh báo: một cho lựa chọn hỗn hợp,
một cho lựa chọn mà mọi thứ đều chỉ có trực tuyến. Chúng chỉ khác nhau ở câu đầu và ở các lối thoát mà chúng có thể đề
nghị. Khóa thứ ba là dòng hiện ra khi Cmdr trả lại một lần nhấn.

- **`.cloudOnlineOnlyMixedWarning`** · `chỉ có trực tuyến` là cách Finder gọi một tệp đã được dọn khỏi máy; `thùng rác`
  và `dịch vụ đám mây` lấy từ `terms.json` · medium.
- **`.cloudOnlineOnlyAllWarning`** · cùng một đoạn, chỉ đổi “Mọi thứ bạn đã chọn” thay cho “Một phần lựa chọn của bạn”,
  và bỏ lối thoát bỏ chọn: nếu mọi thứ đều chỉ có trực tuyến thì sẽ không còn gì được chọn · medium.
- **`.cloudOnlineOnlyHandedBack`** · dòng phía trên nút, sau một lần nhấn mà Cmdr cố ý không thực hiện. Giọng điềm đạm,
  không xin lỗi · medium.
- **Cả bốn sự thật đều phải giữ**: (1) thùng rác sẽ tải các tệp về, (2) vì vậy Cmdr chỉ đề nghị xóa TOÀN BỘ lựa chọn,
  (3) sau đó KHÔNG có bản sao nào trong thùng rác, nhưng dịch vụ vẫn giữ bản của họ (❌ đừng làm nhẹ đi), (4) các lối
  thoát mà dải cảnh báo nêu ra.
- **Hai đoạn `<strong>` phải giữ nguyên**, ở “tải chúng về trước” và ở động từ “xóa”. Và “Xóa” trong ngoặc kép là nhãn
  của nút: luôn trùng với `fileOperations.delete.confirmDelete`.
- **Lối thoát "make available offline" gọi đúng tên lệnh**: `tải về … để dùng ngoại tuyến`, theo
  `commands.cloudMakeOffline.label` / `menu.context.makeAvailableOffline` (`Tải về để dùng ngoại tuyến`), để người đọc
  tìm được lệnh đó.

## Khi máy chủ nói là không có mục chia sẻ đó (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

Trường hợp duy nhất trong nhóm này mà thử lại cũng vô ích: máy chủ trả lời rõ ràng rằng nó không có mục chia sẻ nào tên
như vậy. Vì thế thông báo này không có nút, và giọng văn không được gợi ý điều gì tạm thời (không `hiện`, không
`thử lại`), khác hẳn các thông báo anh em.

- **"the server says it has no share by that name" → `vì máy chủ cho biết nó không có mục chia sẻ nào tên như vậy`** ·
  catalog (`errors.mount.shareNotFound`) và NetAuthAgent `EINFO_NO_SHARE` (bundle SỐNG trong máy: "Chia sẻ “%@” không
  tồn tại trên máy chủ.", macOS 26.6.2, 25G83, 2026-09-17) · `high`. Apple viết `chia sẻ` trần; catalog giữ
  `mục chia sẻ` như `style.md` đã chốt.
- **`cho biết`, không phải `nói`** · máy chủ đưa ra câu trả lời dứt khoát, và `cho biết` là cách tiếng Việt diễn đạt một
  hệ thống báo lại trạng thái mà không nhân cách hóa · `high`.
- **"This one won't sort itself out" → `Tình huống này sẽ không tự thay đổi`** · không có nguồn trong pile; đây là cách
  nói thông dụng và mang đúng điều tách thông báo này khỏi các thông báo khác: chờ cũng không hết · `high`.
- **"may have been renamed or removed" → `có thể … đã được đổi tên hoặc xóa`** · nguyên văn từ
  `errors.write.destinationNotFound.suggestion` · `high`. Dùng `xóa` (từ đã chốt cho _delete_), không phải `gỡ bỏ`.
- **"so it's worth checking there" → `nên hãy kiểm tra lại ở đó`** · `ở đó` trỏ về máy chủ nên không phải lặp từ ·
  `high`.
- **"a lot slower" → `chậm hơn nhiều`** · ở đây tiếng Anh không nêu bội số, khác với thông báo anh em (`4 lần`) ·
  `high`.
- **Thông báo ngắn gọi lại mục chia sẻ là `mục này`** để khỏi lặp `mục chia sẻ` hai lần trong một câu · `high`. Phần
  đuôi `vẫn dùng kết nối hệ thống` là của ba thông báo anh em (`fileExplorer.pane.directConnectionUnreachableToast`…).

## Tên trông giống hệt nhau trên máy chủ (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

Hai mục có tên hiện ra y hệt nhau nhưng máy chủ lưu bằng chuỗi ký tự khác (é một ký tự so với e + dấu rời, hoặc chữ hoa
so với chữ thường). Bốn khóa phải kể cùng một câu chuyện bằng cùng một cụm từ.

- **"look the same, but the server spells them differently" →
  `trông giống hệt nhau, nhưng máy chủ lưu chúng theo cách viết khác nhau`** · cùng gốc với
  `errors.listing.ambiguousName.explanation` (`lưu chúng với cách viết khác nhau`) · `high`. ❌ Không
  `máy chủ viết chúng khác nhau`: máy chủ không "viết", nó lưu. Không nhắc Unicode hay chuẩn hóa.
- **"the one that's there" → `mục hiện có`** · nối thẳng với nhãn `Hiện có (thư mục):` / `Hiện có (tệp):` ngay trên
  trong cùng hộp thoại (`fileOperations.transferProgress.existingFolderLabel`) · `high`. `hiện có` = "existing", không
  phải động từ "show".
- **Nhắc tới nút Ghi đè trong câu: `Nút Ghi đè sẽ thay thế …`** · tên nút lấy nguyên từ
  `fileOperations.transferProgress.conflictOverwrite`; động từ mô tả là `thay thế`, đúng như nút Replace trong hộp thoại
  trùng tên của Finder (`PE108` → `Thay thế`) · `high`. Thêm `Nút` để câu không mở đầu bằng một nhãn trần.
- **"so Cmdr didn't pick one" → `nên Cmdr không tự chọn mục nào`** ở cả hai khóa lỗi · `tự` mang đúng ý "đoán hộ bạn" ·
  `high`.
- **"Choose it from its folder instead" → `Hãy mở thư mục chứa nó rồi chọn ở đó.`** · ở đây `thư mục chứa nó` là cụm mô
  tả ("thư mục đang chứa nó"), không phải thuật ngữ parent folder; bản gợi ý dài vẫn dùng `thư mục cha` như catalog ·
  `high`.
- **Lời dẫn "Here's what to try" là `Bạn có thể thử:`**, đúng như § Giọng lỗi, ở cả 60 khóa `errors.json` có lời
  dẫn này (kể cả khi nó đứng sau một câu mở, như `errors.provider.*`). ❌ Không `Đây là những cách để thử:`: dịch sát
  từng chữ và dài hơn. Trong `errors.listing.notFound.suggestion` / `…pathNotFoundErrno.suggestion`, "the share" là
  `mục chia sẻ` như catalog đã chốt, không phải `thư mục chia sẻ` (shared folder).

## Công tắc "Cho phép AI đám mây" và các trạng thái AI đám mây đang tắt (`ai.cloudConsent.*`, `askCmdr.gate.*`)

Một công tắc đồng ý về quyền riêng tư: mặc định tắt, và không gì rời khỏi máy Mac cho đến khi bật. Câu chữ phải bình
tĩnh và không bao giờ hứa nhiều hơn những gì Cmdr làm. Nguồn lấy từ macOS đang cài (máy M1 không có kho tham chiếu),
theo `docs/i18n/reference-pile/how-to-mine.md` § "No pile on this machine?".

- Allow cloud AI (nhãn công tắc, `ai.cloudConsent.label`) · **Cho phép AI đám mây** · `Allow` → `Cho phép` là nút trong
  các hộp hỏi quyền của macOS vi (`TCC.framework` `Localizable.loctable`, `REQUEST_ACCESS_ALLOW`, macOS 26.6.2 build
  25G83, đọc 2026-09-23); `AI đám mây` là giá trị đã ship của `settings.ai.provider.opt.cloud`, lựa chọn ngay phía trên
  · `high`
- cloud AI đang tắt · **AI đám mây đang tắt**, cùng khuôn `AI đang tắt` của `ai.translateError.off.title` · `high`
- **Nhắc lại nhãn công tắc từng chữ.** `Cho phép AI đám mây` vừa là nhãn vừa là câu mệnh lệnh tự nhiên, nên
  `askCmdr.gate.cloudOff.body` và `settings.askCmdr.cloudOffHint` viết thẳng `cho phép AI đám mây`; riêng
  `settings.ai.cloudConsent.lockedHint` trích nhãn trong ngoặc thẳng `"…"`, theo các chuỗi bên cạnh trong
  `settings.json`.
- Settings > AI · **Cài đặt > AI**, giữ `>` như các chuỗi cùng họ `ai.translateError.*` và đúng như `@key` yêu cầu
  ("keep it as shown"). Quy tắc `›` trong `style.md` là cho đường dẫn tới các mục cài đặt khác; ở họ chuỗi AI này,
  catalog đã ship `>` bốn lần.
- AI service / cloud AI service · **dịch vụ AI** / **dịch vụ AI đám mây**; tiếng Anh tách `service` khỏi `provider`
  (`nhà cung cấp`) của `ai.cloudConsent.askCmdr.*`, tiếng Việt cũng vậy.
- custom endpoints · **các điểm cuối tùy chỉnh** · `high`
- side panel · **khung bên** (thuật ngữ sidebar đã chốt) · `high`
- Tên tính năng trong phần mở rộng (trong `<b>`): `Gợi ý tên thư mục mới`, `Tìm kiếm bằng ngôn ngữ tự nhiên` (thuật ngữ
  của `queryUi.bar.aria.ai`), `Chọn theo mô tả`; "what you type" là `nội dung bạn nhập` (`nhập` cho ô nhập liệu) ·
  `high`
- `settings.askCmdr.enabled.label` = `Ask Cmdr`, giống hệt tiếng Anh, có `sameAsSourceJustification` (tên sản phẩm, như
  `settings.section.askCmdr`).

## Thoát toàn màn hình bằng phím Escape (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- **full screen (chế độ cửa sổ của macOS): `toàn màn hình`, thường kèm `chế độ`** · macOS AppKit vi `MenuCommands`
  (`Exit Full Screen` → `Thoát Toàn màn hình`, `Make Window Full Screen` → `Chuyển cửa sổ sang chế độ toàn màn hình`) và
  `AccessibilityImageDescriptions` (`thoát toàn màn hình`, chữ thường giữa câu); Microsoft terminology cũng là
  `toàn màn hình` / `chế độ toàn màn hình`. Viết thường theo sentence case, ❌ không viết hoa `Toàn` như tiêu đề menu
  của Apple. `high`.
- **Escape (phím): `Escape`, trong nhãn là `phím Escape`** · macOS AppKit vi `FunctionKeyNames` giữ `Escape` → `Escape`;
  khuôn `bằng phím <Phím>` theo `settings.fileExplorer.suppressQuickLookHint.label` (`… bằng phím Space`). `high`.
- **Exit full screen on Escape (nhãn công tắc): `Thoát toàn màn hình bằng phím Escape`** · nhãn trong toast
  (`main.escapeFullScreenHint.switchLabel`) và trong Cài đặt khớp từng chữ. `high`.
- **Settings > Advanced (liên kết trong toast): `Cài đặt > Nâng cao`** · giữ `>` như tiếng Anh và như
  `fileExplorer.quickLookHint.configurable` (`Cài đặt > Phím tắt`); `Nâng cao` = `settings.section.advanced`. `high`.
- **"You'll only see this once": `Bạn sẽ chỉ thấy thông báo này một lần.`** · `thông báo` cho toast, như thuật ngữ toast
  → `thông báo nhỏ` đã chốt. `high`.

## Bản gốc đã thay đổi trong khi di chuyển (`transfer.changedDuringMove`, 2026-09-25)

- **"changed during the move" → `đã thay đổi trong khi di chuyển`** · Finder `PE56` "một hoặc nhiều mục đã thay đổi
  trong khi đang ghi" (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25) · `high`.
  `trong khi di chuyển`, `vẫn ở lại` và `thư mục nguồn` lấy nguyên từ chuỗi chị em `transfer.appearedDuringMove`, hiện
  cùng một toast.

## Dòng chờ trong "Mở bằng" và "Chia sẻ" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Dòng chờ trong "Mở bằng" và "Chia sẻ" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- **Finding apps…: `Đang tìm ứng dụng…`** · mẫu `Đang …` của macOS ("Searching…" = `Đang tìm kiếm…`); `ứng dụng` như
  `settings.behavior.textEditorApp.checking`. `high`.
- **share options: `tùy chọn chia sẻ`** · `chia sẻ` = tên submenu. `high`.
- **No share options: `Không có tùy chọn chia sẻ`** · mẫu menu trống của macOS ("No Services Apply" =
  `Không có Dịch vụ Áp dụng`), viết thường theo sentence case. `high`.
