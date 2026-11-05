# vi decisions

Distilled rulings behind `terms.json`: "X over Y because Z", each defending a form against a well-meant "fix". The brief
pulls a section when its heading cites a batch key. Evidence tiers: macOS (pile or live bundles) over Microsoft over
GNOME / Thunar / Dolphin / Total Commander; catalog consistency over a pile-ideal form that would fork a term. What
`style.md` or `terms.json` already says isn't repeated here.

## Tên mục Cài đặt (`settings.section.*`, `shortcuts.scope.*`, `fileExplorer.navigation.group*`)

- Naming a section, group, or scope copies these labels verbatim; two-part names join with `&`
  (`Điều hướng & thao tác tệp`). The AI family (`ai.translateError.*`, `ai.cloudConsent.*`, `askCmdr.gate.*`) and
  `fileExplorer.quickLookHint.configurable` keep EN's `>` (their `@key`).

## Giọng lỗi: các cụm đã chốt (`errors.*`, `updates.failure.*`, `fileExplorer.network.browser.status.error`, `licensing.error.*Hint`)

- The style digest's error phrases beat the word-for-word `Đây là những cách để thử:`, the MS-register
  `Đã xảy ra sự cố không mong muốn`, and the bureaucratic `Vui lòng`. "Try again?" stays a bare `Thử lại?`. "fixes" →
  `việc khắc phục`, except David's `sửa lỗi` (`onboarding.stepBeta.openBeta`).

## Trạng thái hàng đợi (`queue.row.status`, `operationLog.status.*`, `operationLog.outcome.*`, `operationLog.initiator.*`, `indexing.eta.*`)

- Queue and log share the queue's status set; "Couldn't finish" → `Chưa hoàn tất được` (macOS `thao tác chưa hoàn tất`).
  AI client → `Máy khách AI` (pairs with `máy chủ`); "Ns left" → `còn Ns`.

## Nhãn macOS khi thiết lập (`onboarding.stepFda.*`, `onboarding.stepOptional.*`, `onboarding.stepAi.*`)

- Copy Finder / System Settings exactly (`Thoát & Mở lại`). `Chấp nhận kết nối đến` has no pile source: best effort.

## Bấm đúp vào nền khung (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`)

- "I like it" → `Tôi thích` (the user speaking). Pane background → `nền khung`; empty space → `khoảng trống`; go up a
  folder → `lên thư mục cha`.

## Tệp quá lớn cho hệ thống tệp (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- A drive holding files → `chứa` over `lưu trữ` (reads as archiving); too large for X → `quá lớn đối với X` (GNOME).

## Hộp thoại sao chép (`fileOperations.transferDialog.*`, `fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`, `queue.row.label`)

- `transferDialog.policy*` keep an `=1` arm because the WORDING changes ("Skip" vs "Skip all"); without it one clash
  read `Bỏ qua tất cả`.
- `delete.trashSwitch` reads exactly like `titleVerbOnly`'s trash arm. From / To → `Từ` / `Đến` (Total Commander
  662/663); doesn't exist yet → `chưa tồn tại`.

## Duyệt và chỉnh sửa tệp nén (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `settings.archives.*`, `fileOperations.archivePassword.*`, `errors.listing.archiveUnreadable.*`)

- zip / tar / 7z → `tệp nén` over GNOME `kho lưu trữ` and macOS `Bộ lưu trữ` (both read as backup storage).
  `settings.archives.*.description` share the frame `Nhấn Enter sẽ làm gì với tệp …`.

## Nén (`commands.fileCompress.*`, `fileOperations.transferDialog.toggleCompress`, `fileOperations.transferDialog.confirmCompress`, `fileOperations.transferDialog.pathErrorNotZip`, `settings.archives.compressionLevel.*`)

- The NEW archive Compress writes is `tệp lưu trữ` (Finder `Tệp lưu trữ Zip`); a browsed archive stays `tệp nén`. Two
  objects, not drift.

## Dán thành tệp (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- "as a file" → `thành tệp` / `thành {filename}` over `dưới dạng`: tighter, and it says "turn into".

## Nhật ký thao tác (`operationLog.*`, `commands.logOperationLog.*`)

- Every rollback state uses `hoàn tác` (`Không thể` / `Có thể` / `Đang` / `Đã` / `Đã … một phần`), so the column reads
  as one scale.

## Ask Cmdr (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- Archive a chat → `Lưu trữ` / `Bỏ lưu trữ` (Finder `AR40`), never `tệp nén`. free → `miễn phí` over MS `tự do`;
  estimate → `ước tính` over `báo giá` (a quote); cost → `chi phí` over `giá vốn`.
- The "file history" tool reads the operation log → `lịch sử thao tác`; don't fork silently if English splits them.
  Consent items take no final period.

## Lập chỉ mục hình ảnh (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.*`)

- A photo NAS → `kho ảnh`, never `tệp nén`. `drive.indexing` fronts `Trên ổ đĩa này, …` to avoid a double `trên`.

## Xem lại việc đổi tên hàng loạt (`askCmdr.renameReview.*`)

- review → `xem lại` (AppKit "Review Changes…"); needs attention → `cần được xem lại` (macOS vi never says `chú ý`). Ask
  Cmdr to → `nhờ Cmdr` over `yêu cầu` (formal, makes the brand an object).
- cycle → `chu trình` (graph sense) over `chu kỳ` / `vòng tròn`; one row → `lần đổi tên này`.

## Lập chỉ mục đang tắt (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- `không có ổ đĩa nào được lập chỉ mục` (a flat state). "Off with X" → `Tắt theo X`.

## Tên tiện ích macOS, mục chia sẻ, liên kết mềm, beta công khai (`errors.listing.*`, `settings.network.*`, `settings.behavior.*Seen.*`, `queryUi.results.live.*`, `settings.mediaIndex.*`)

- Utility names verified on macOS 27.0 (each app's `InfoPlist.loctable` / `MainMenu.loctable`, 2026-09-24). A "has been
  shown" flag → `hiển thị`; a search-walk scan → `quét`, walking → `duyệt`.

## Lẻ (`commands.handler.zoomResetHintMenu`, `askCmdr.error.notConfigured`, `askCmdr.wake.needsApiKey`)

- macOS sidebar → `thanh bên`; Cmdr's side panel → `khung bên`. placeholder (`[N]` too) → `phần giữ chỗ` over MS
  `chỗ dành sẵn` and `ký hiệu`; preset → `giá trị đặt trước` (bare `đặt trước` reads "reserved").
- Azure deployment / resource → `bản triển khai` / `tài nguyên`: MS's own product, so MS outranks macOS.
  `View > Zoom > 100%` stays English (a literal menu path, per its `@key`).

## Kiểm tra thay đổi (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- Run-kind headers are verb phrases (`Kiểm tra thay đổi`, like `Cập nhật nhanh`); "the check" → `lần kiểm tra`.

## Lần truyền bị đứng yên: thông báo trên hộp thoại + hàng đợi (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- no progress → `không có tiến triển` over macOS `tiến trình` (an OS process here) and MS `tiến độ`; the bar stays
  `tiến trình`. stopped moving → `đang đứng yên` over `đã dừng` (it hasn't) and `treo` (a crash). still open →
  `vẫn đang mở` over `đang được sử dụng` (something ELSE holds it).

## Đường dẫn đã sao chép (`fileExplorer.clipboard.copiedPath`)

- The path sits on its own line, so the sentence ends in a colon; "it's now on your clipboard" folds into
  `vào bảng nhớ tạm`, no `nó`.

## Hàng đợi thao tác: "operation queue" (`queue.windowTitle`, `queue.heading`, `queue.list.aria`, `queue.row.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- It lists every job kind, so `Hàng đợi thao tác` (pairs with `Nhật ký thao tác`); the copy / move dialog keeps
  `lần truyền`.
- Bare plural heading → `Các thao tác`: a bare `Thao tác` collides with `transferDialog.operationAria`. The queued toast
  repeats `thao tác` because a `nó` would bind to the jobs ahead.

## Huy hiệu tiến trình ở góc + thông báo chưa hoàn tất (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- dismiss → `Bỏ qua` over `Xóa` (on a delete row reads "delete again") and `Gỡ`; it also renders Skip, never on the same
  surface.
- "Couldn't finish <verb>" → `Chưa hoàn tất được thao tác <verb>`: without the noun, `được` + verb is passive ("hasn't
  finished being deleted"). to {destination} → `vào`, never `sang`. chip → `huy hiệu`.
- `chip.tooltip` pairs `=0 {}` with `other`; each optional clause carries its leading space, so nothing doubles or
  dangles a `·`.

## Lời nhắc xung đột (`fileOperations.operationConflict.context`/`.pausedNote`)

- Copy and move both take `vào` (one destination preposition, over Finder's `đến`); the generic arm says `trong`;
  `archive_edit` names the archive. "Everything else is paused" → `Các thao tác khác đã tạm dừng` over `Mọi thứ khác`
  ("Cmdr froze").

## Nút "Chạy nền": trạng thái hàng đợi trống của nút Hàng đợi (`fileOperations.transferProgress.background`/`.backgroundAria`)

- A bare `Nền` (Total Commander) names a backdrop here (`nền khung`). `backgroundAria` = `Giữ chạy nền`, containing the
  label; keep that on any rewording.

## Hộp thoại thoát khi thao tác đang chạy (`main.quit.*`)

- Quit now → `Thoát ngay` over `Vẫn Thoát` (overrides an objection; this skips a timer). Keep working →
  `Tiếp tục làm việc` over `Hủy` (cancels the running ops) and `sau` / `nhắc lại` (nothing is deferred).
- clears away → `dọn dẹp` over `xóa` ("deletes your file") and `Dọn sạch` (Empty Trash); the leftover is
  `tệp ghi dở còn sót lại` (`showStagingTempFiles`).
- ❌ The body never opens on `Chỉ mục …` ("the index"); whatever's finished → `Những gì đã xong`, not `Mọi thứ`. restart
  or logout → `khởi động lại máy` (bare = the app). `countdownAria` has no visible label.

## Usage stats: bỏ "ẩn danh" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, `onboarding.stepBeta.emailNote`)

- A stable per-install id, so ❌ `ẩn danh`, `bút danh`, `giả danh`. random id → `một mã định danh ngẫu nhiên` over
  `mã ngẫu nhiên` (a coupon or PIN); tied to → `gắn với`.

## Câu hỏi làm dừng một hàng trong hàng đợi + hộp thoại hoàn tác (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- Needs your answer → `Cần bạn trả lời`, ❌ never opening `Đang chờ` (the queued status). The prompt is a `câu hỏi`,
  over `lời nhắc` (a reminder).
- Stop in the rollback tooltip → `Dừng lại`, ❌ never `Hủy` (Cancel keeps the files). bring this one up → `hiện … lên`;
  the Show button stays `Hiển thị`.

## Đổi tên liên tiếp (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- and so did → `“{name}” cũng vậy: {reason}`: `{reason}` describes ONE file, a merged subject would spread it. N other
  files → `{othersText} tệp khác`.

## Tên không dùng được (`fileOperations.validation.nameNotUsable`)

- `Không thể dùng tên … đó`, no final period (joined into a toast), no guessed `không hợp lệ`.

## Thao tác được đề xuất (`suggestedOps.*`, `commands.suggestedOpsShow.*`, `askCmdr.decision.*`)

- approve → `Phê duyệt` over macOS `Chấp nhận` (receiving AirDrop); reject → `Từ chối`.
- Name `Gợi ý AI`, as `fileOperations.mkdir.aiSuggestions*`. `Why:` → `Lý do:` (labels an answer; `Tại sao?` asks).

## Nhân bản (`commands.fileDuplicate.*`)

- duplicate → `Nhân bản` (Finder File menu, macOS 26.6.1, 2026-08-19), distinct from F5 `Sao chép`.

## Menu gốc (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Source: Finder `vi.lproj` (`MenuBar.strings`, `LocalizableMerged.strings`), Safari for tabs (macOS 26.5.2,
  2026-08-19); the English side is `en_GB.lproj` (`Base.lproj` holds only nibs).
- Zoom → `Thu phóng`, Minimize → `Thu nhỏ`: Tier 1 despite clashing with zoom values elsewhere. changelog →
  `Nhật ký thay đổi` (a document) beside Help's `Có gì mới` (news).
- Tag row `Gỡ bỏ “{color}”` over Finder `Xóa “^0”` (`TG6`): nothing is deleted. `menu.zoom.percent*` and
  `menu.view.askCmdr` are same-as-source.

## Dự phòng SMB của macOS (`fileExplorer.network.osMountFallback.*`)

- native → `tích hợp sẵn` over MS `riêng` (`riêng của macOS` reads "macOS-only"); siblings say `kết nối hệ thống`. The
  `chậm hơn N lần` multiplier is `tentative` (no pile source).

## Lỗi khi đổi tên / tạo mới (`errors.mutation.*`, `errors.volume.*`)

- SIP → `tính năng Bảo vệ Toàn vẹn Hệ thống` (Finder `ET6`, 2026-08-23): Apple localizes it. Get Info →
  `cửa sổ Lấy thông tin`. can't hold that name → `Đích không dùng được tên đó` over `không hợp lệ` (claims a rule).
- `mutation.timedOut` isn't a failure: `Ổ đĩa vẫn chưa phản hồi, nên …`. `volume.deviceSessionReset` isn't an unplug: ❌
  `rút` / `tháo`. `mutation.notFound` and `volume.notFound` share one value.

## Lỗi khi chuyển vào Thùng rác (`errors.mutation.trash*`)

- has no Trash → `không có Thùng rác, nên cách duy nhất là xóa vĩnh viễn` over `không hỗ trợ` (`volume.notSupported`'s
  claim). `macOS đã từ chối chuyển mục này`, short: the reason sits in the details.

## Ba biến thể phần thân của hộp thoại báo cáo sự cố (`crashReporter.dialog.body.*`)

- `.keptRunning` says `nhưng vẫn tiếp tục chạy` (AppKit `NSExceptionAlert`). The shared second sentence is
  `Đây là báo cáo kèm chi tiết …`.

## `thoát bất ngờ`, `ở chế độ nền`, và hai hướng đã loại (`crashReporter.dialog.body.ended`, `crashReporter.dialog.privacyNote`, `crashReporter.dialog.title*`, `settings.updates.crashReports.description`)

- `thoát bất ngờ` (AppKit `AppKitErrors`) over the unsourced `thoát đột ngột`. `title.report` and the crash-reports
  description (both outcomes) say a bare `báo cáo`; the setting LABEL keeps `Gửi báo cáo sự cố` (a name).
- ❌ `ngầm` (every pile hit means underground) and ❌ `hậu trường` (no source); the pile's `nền` hits are visual
  (`hình nền`), no evidence either way.

## Lỗi khi tháo ổ đĩa / ngắt kết nối (`errors.eject.*`)

- in use → `đang dùng`, the everyday form of Finder's `đang được sử dụng` (`NE66`). not removable → the TYPE frame
  `Ổ đĩa này không phải loại có thể tháo` (Finder `Có thể tháo`) over `không tháo được` (a one-time refusal).
- once it's idle → `khi không còn bận` (`rảnh` is for the user). `eject.timedOut` isn't a failure (like
  `mutation.timedOut`); `eject.unexpected` shares `mutation.unexpected`'s value.

## Apple CÓ dịch "Get Info" và "Locked" sang tiếng Việt (`errors.write.fileLocked.suggestion.mac`, `errors.write.permissionDenied.suggestion.deleteMac`, `errors.listing.noPermissionErrno.suggestion`, `errors.listing.permissionDenied.suggestion`)

- Locked → `“Đã khóa”` (Finder `AXNODE1`, `NE18`); Sharing & Permissions → `phần Chia sẻ & quyền`, `phần` as Apple
  writes it (macOS 26.5.2, `InfoWindowPermissionsView.strings`, 2026-08-24).

## Toast thùng rác (`fileOperations.trash.*`, `commands.fileGoToTrash.*`, `commands.editUndoTrash.*`, `menu.edit.undoTrash`)

- put back → `đưa trở lại` (Finder `N153.1`) over `đặt lại` (names) and Nautilus `khôi phục` (broader). Go to trash →
  `Đi tới thùng rác`.
- Undo → `Hoàn tác thao tác chuyển vào thùng rác` (Finder `MV2_V1`, `ME14`), so it never reads as text undo.

## Thêm ghi chú vào báo cáo đã gửi (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- get your notes to the team → `Để nhóm nhận được …` over `chuyển` (Move) and a doubled `gửi … gửi`. "What was sent"
  twins `errorReporter.dialog.detailsToggle` (`sắp` → `đã`). `ID tham chiếu của bạn:` matches
  `errorReporter.sentToast.message`.

## Chọn và bỏ chọn: hộp thoại Select / Deselect (`selection.*`)

- focused pane → `khung đang hoạt động` over `khung đang chọn` (three `chọn`, ambiguous); if ever unified,
  `khung có tiêu điểm` has the Tier 1 source.
- Tooltips open with their label anyway (like `search.action.goToFile.tooltip`). `selection.recent.*` mirror
  `queryUi.recent.*` with `lựa chọn`; `{query}` sits last, after a colon.

## Rà soát trôi thuật ngữ toàn catalog (`menu.go.back`, `commands.navBack.label`, `onboarding.wizard.back`, `shortcuts.section.filterModified`, `licensing.about.srTitle`)

Tier 1 counts from the live bundles, macOS 26.6.2 (build 25G83), 2026-08-30, behind the `terms.json` rulings that beat
the catalog's drift:

- `kích cỡ` 33 vs `kích thước` 0 (the file list and search results must match); `hiển thị` 147; `tải về` 35 vs
  `tải xuống` 0 (MS's form is a Windows convention).
- `tab` / `thẻ` (Finder, Safari). Tag and accent colors are one Apple set; the everyday `Xanh dương` / `Xanh lá` / `Tím`
  appear nowhere in the pile. Cmdr-only colors (`Hổ phách`, `Xanh chanh`, `Xanh mòng két`, `Chàm`) have no source.
- `gỡ bỏ` even with an object, so `settings.mediaIndex.chosenFolders.removeAria` contains its label; Remove download →
  `Xóa bản tải về` (Finder).
- Unreachable → `không tới được` (`tentative`) over `không kết nối được` (connect) and `không truy cập được`
  (permission). Low disk space → `Sắp hết dung lượng đĩa` (Photos) over `Thiếu …` ("not enough").
- Modified as a date → `Đã sửa đổi`; a user-changed shortcut (`filterModified`) → `Đã thay đổi`.

## Menu wording, System Settings panes (`menu.app.hideOthers`, `commands.appHideOthers.label`, `errors.git.*`, `errors.provider.*`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`, `askCmdr.renameUndo.undone`/`.partial`)

- Hide others → `Ẩn các mục khác` (Finder, TextEdit, Preview, macOS 26.6.2), matching the row macOS shows above it.
  Localized panes stay vi (`Cài đặt chung`, `Mục đăng nhập & Phần mở rộng`, `Tài khoản Apple`, `Mạng`, `Dung lượng`);
  prefer a token where one exists.
- Sample email → `ban@` (like MS `ai_do@`). Old names back → `Đã đặt lại tên cũ cho …`: only the name changes.

## Thao tác mới hoàn tác được một nửa: làm tiếp cho hết (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- `Tiếp tục hoàn tác` (Finder `Tiếp tục sao chép`) over `Hoàn tất việc hoàn tác` (one mark apart, beside
  `Chưa hoàn tất được`). `vẫn` binds to `không chắc chắn`, not `bỏ qua`.

## Toast sau khi hoàn tác thao tác đang chạy (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

- Where English matches `askCmdr.renameUndo.skipReason.*` (`folderNotEmpty.*`), values match word for word. Count "item"
  as `mục`, "file" as `tệp`.
- Left X where it is → `Giữ nguyên`. after Cmdr put it there → `đưa nó tới đó` over `ghi nó vào đó` (copy only).
  `failed.*` leaves the frame (the drive refused): `Không hoàn tác được …`.
- `leftBehind` and the four `rollbackConfirm.body*` share `Cmdr bỏ qua những gì nó không chắc chắn`. `stagedLeftover.*`
  retries `vào lần kết nối tới đó tiếp theo`, never a time.

## WebKit quá cũ (`main.oldWebkit.*`)

- `Safari`, `Mac`, and `15.4` stay verbatim; Software Update → `Cập nhật phần mềm`.

## Thông báo về macOS cũ (`main.oldMacos.*`)

- best effort → `chỉ cố hết sức thôi`, deliberately not literal; the closing line is David's `mình`.

## Ask Cmdr xem bên trong tệp: hai nhãn công cụ + hai khóa đồng ý (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

- `hình thu nhỏ`: AppKit, MS, Dolphin, Thunar. camera details → `thông tin máy ảnh` over `chi tiết` (macOS's expand
  button). `một bức ảnh` is `tentative`.
- whole files → `toàn bộ tệp`: the consent no longer promises "no contents", so ❌ don't restore `chỉ đọc` /
  `không bao giờ đọc nội dung tệp`.

## Hai chú giải của nút Rollback (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

- Rolling back a move deletes nothing: `đưa trở lại`, ❌ never `xóa`.

## Mở terminal tại đây (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- macOS vi keeps `Terminal` (Finder `N67`) and the loanword `terminal`, so the card title is same-as-source. The command
  is `Mở terminal tại đây` everywhere.

## `Sort by relevance` (`fileExplorer.columns.sortByRelevance`)

- `mức độ liên quan` (WorkflowKit, Automator) over AppStoreKit's `Độ phù hợp` (macOS 26.6.2, `plutil`, 2026-09-06).

## `Documents and packages` (`settings.archives.ooxml.*`)

- packages → a bare `gói`, wider than the `Gói ứng dụng` row below, as English splits them.

## Trung tâm máy chủ (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`, `fileExplorer.navigation.server*Toast`, `commands.servers*`, `shortcuts.scope.servers`, `shortcuts.scope.places`, `menu.network.pinToSwitcher`, `.unpin`, `settings.servers.*`, `settings.adb.*`, `goToPath.dialog.opensServer`, `goToPath.dialog.addsServer`)

Evidence: macOS 26.6.2 (25G83) live bundles, `plistlib` on each `.loctable`, 2026-09-06/07; richest is the Connect to
Server dialog (`NetAuthAgent.app`).

- Servers take `tới` (`Đang kết nối tới {name}…`, `reconnecting`) over macOS `với`, so the pane's states pair. Cmdr
  giving up → `đã dừng kết nối tới`, ❌ never `ngắt`.
- sign-in method → `cách đăng nhập` over `Phương thức xác thực` (English chose the everyday word). certificate →
  `chứng nhận` (macOS) over MS `chứng chỉ`; key `khóa` and fingerprint `dấu vân tay` (Keychain Access) stay apart.
  compromised → `bị xâm phạm`, not `bị thu hồi`.
- Last used → `Dùng cuối` (System Settings privacy table). The identity hint names actions as their buttons (`quên`,
  `thêm`). Found nearby → `Tìm thấy ở gần`, without `Đã`. pin / unpin → `Ghim` / `Bỏ ghim` (AppKit `MenuCommands`) over
  `Gỡ ghim` / `Hủy ghim`; `pinToSwitcher` / `unpin` name `bộ chọn ổ đĩa`.
- as {username} / as guest / Sign in as… / Use guest share `với tư cách` (`Kết nối với tư cách khách`), over
  `bằng tài khoản` (can't carry `khách`). Optional placeholder → `Không bắt buộc` (Apple's `Bắt buộc`); a bare
  `Tùy chọn` reads "Options". Passphrase → `Cụm từ mật khẩu của khóa`, apart from the account password.
- check X against Y → `đối chiếu`; bare check → `kiểm tra`, so `Tôi đã kiểm tra rồi` answers
  `Đang chờ bạn kiểm tra khóa`. "something sitting between you and it" stays descriptive (`tentative`), no MITM jargon.
- `servers.hostKey.changedBody` and `servers.paneState.hostKeyChangedHint` share one middle sentence; where `Hãy mở lại`
  could mean the app, name `máy chủ`. Got it → `Đã hiểu` (`Đã rõ` only inside `whatsNew.optOutToast`).
  `settings.adb.status.*` say `theo dõi để phát hiện điện thoại` (bare `theo dõi điện thoại` = surveillance).

## Android qua ADB (`adb.*`, `settings.behavior.adbHintDismissed.*`, `settings.fileOperations.adbEnabled.*`, `settings.summary.adb`)

- Sources: AOSP `enable_adb` (`main`, 2026-09-07); platform tools → `bộ công cụ nền tảng Android` (Google's vi docs).
  `adb.hint.text` capitalizes `Gỡ lỗi qua USB` (the exact switch); prose keeps it lowercase, as AOSP does.
- You stopped opening your phone → `dừng`, ❌ not `hủy` (the Cancel label). The busy tooltips share
  `Cmdr vẫn đang làm việc với tệp trên … này. Hãy … khi việc đó xong.`

## Không có mật khẩu đã lưu (`fileExplorer.navigation.forgetSecretNoneToast`)

- Nothing went wrong, so no `không thể` / `lỗi`: `Không có mật khẩu đã lưu cho {name}.`, reusing its siblings'
  `mật khẩu đã lưu`.

## Thử lại, khóa máy chủ (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `servers.paneState.hostKeyChanged`, `adb.connect.unauthorized`)

- The retry totals are bare fragments of `retryKeepsTrying` (no preposition, no period) and keep the one-arm plural so
  the placeholder set matches. Won't connect → `Cmdr sẽ không kết nối tới {name}` (a standing refusal) over `đã dừng`
  (an interruption).

## Menu hàng máy chủ (`menu.network.open`, `menu.network.edit`)

- `menu.network.edit` copies `commands.serversEdit.label`; open on a server row is `menu.file.open`'s `Mở` (Finder uses
  one verb).

## Lời mời ghim Cmdr vào Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- ❌ Not MS's `thanh dock` / `đế cắm`. The title borrows Dock's `Giữ lại trong Dock` (`KEEP_IN_DOCK`).
- decline → `Không, cảm ơn`, ❌ never `Để sau` (Cmdr doesn't ask again). Timing stays vague (`vài ngày`,
  `một thời gian`), ❌ never a number.
- `addedButDockDidNotRestart`: the pin HAPPENED, so `chưa tải lại`, positive fact first; `notAdded` also takes `chưa`. X
  is managed by Y → `X được Y quản lý`. `managedDock` keeps `của bạn` (it informs).

## Menu chuột phải trên biểu tượng Dock (`menu.dock.*`)

- Source: `Dock.app/Contents/Resources/vi.lproj/DockMenus.strings` (macOS 26.6.2, 2026-09-09), not in the pile.
- Connect to Server… → `Kết nối với máy chủ…` (Finder `N84`): the LABEL takes `với`, prose `tới`. `{name} ({parent})`
  stays as English (AppKit `%1$@ (%2$@)`), ❌ no preposition inside. `menu.dock.searchFiles` = `menu.edit.searchFiles`.

## Hiển thị trong Finder (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- Quote `“Hiển thị trong Finder”` everywhere, like its card. The first-time notice explains, ❌ never apologizes.

## Thiết lập ban đầu (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

- Why? → `Tại sao?` (PhotosUICore, 2026-09-09). Learn more → `Tìm hiểu thêm` over `Thông tin khác` (macOS's More Info
  button).
- star → `gắn sao` (GitHub has no vi; shipped, over Nautilus `đánh sao`). `repo` stays a loanword; "repository" → `kho`.
- Quote Apple's `Mạng cục bộ` as is, ❌ not `Truy cập mạng cục bộ`. dumber → `kém thông minh hơn hẳn` over `dốt hơn` (an
  insult).
- `stepAi.local.tooltip` quotes `stepAi.cloud.label`; `onboarding.wizard.stepTooltip` keeps `+1` and the `select` arm
  names. come and go → `chỉ tồn tại tạm thời` over `đến rồi đi` (`tentative`).

## Trình xem tải tệp từ điện thoại, máy chủ hoặc tệp nén (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- Fetching → `Đang tải` over Finder's `Đang tìm nạp…` (metadata) and `Đang tải về` (an archive member isn't downloaded).
- {done} of {total} → `{doneText} trên {totalText}` (Foundation `Progress.loctable`); `/` is for dense counters. so far
  → `Đến giờ đã tải …`.

## Chỉ mục lỗi thời của điện thoại (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`, `.bodyPhone`)

- The phone is still plugged in, so ❌ `ngắt kết nối`; each value swaps one noun of its `tooltipStale` / `staleDialog.*`
  sibling.

## Thư mục gốc và thư mục bắt đầu của máy chủ đã lưu (`servers.sheet.rootFolder*`, `servers.sheet.startFolder*`, `servers.refusal.startFolderOutsideRoot`, `.rootNotFound`, `.startFolderNotFound`, `.saveUnconfirmed`)

- start folder → `thư mục bắt đầu` (Shortcuts `Vị trí bắt đầu`, `tentative`) over `thư mục khởi động` (startup) and
  `thư mục mặc định`. host in `nameHelp` → `địa chỉ` (the field label): host and server are both `máy chủ`.
- Where the server opens → describe what the user sees, over `Nơi máy chủ mở ra` (booting). The two `…NotFound` share
  their tail; `saveUnconfirmed` says `chưa lưu gì` (a retry is safe).

## Gắn kết mục chia sẻ (`errors.mount.*`, `errors.shareList.*`)

- Source: `NetAuthAgent.app` `Localizable.loctable` (macOS 26.6.2, 2026-09-11), not in the pile. Identical English
  shares one value (`…hostUnreachable`, `…authFailed`). Linux distribution → `bản phân phối` (`tentative`).

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- text editor → `trình soạn thảo văn bản` (MS), never TextEdit (which arrives in `{app}`). Chooser rows reuse
  `openTerminalHereApp.*`.

## Ổ đĩa rời đi giữa chừng (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`)

- A user's disconnect takes `được` (`đang được ngắt kết nối`), a drop takes `bị`. `{done} trên {total} tệp` arrive
  preformatted in a RAW family: ❌ no ICU number syntax.
- all your files → `mọi tệp của bạn` (`toàn bộ tệp` is "whole files"). Tails reuse `vẫn nguyên vẹn` / `ở chỗ cũ` /
  `không mất gì cả`; the `sided.source.*` pair differs by one verb.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`, `errors.write.moveNotConfirmed.*`)

- Cmdr kept the originals BECAUSE it's unsure, so `chưa xác nhận được` over `không thể xác nhận`. haven't moved →
  `vẫn ở nguyên chỗ cũ` over `chưa di chuyển` (still waiting); just moved → `các tệp vừa di chuyển`.

## Thư mục làm việc còn lại (`fileOperations.leftovers.stagingFolderKept`)

- The USER's files, maybe the only copy: ❌ never suggest `dọn dẹp` / `xóa` (unlike `stagedLeftover.*`). found →
  `tìm thấy` over `phát hiện`; unfinished move → `chưa hoàn tất` (a task) over `chưa hoàn chỉnh` (an object); kept →
  `giữ nguyên chúng ở đó` (`ở chỗ cũ` would be false). `ẩn` stays: the reader must act on it.

## Menu mục ưa thích (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent` / `.favoritesAlreadyAdded` / `.favoritesCantAddHere`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- `mục ưa thích` (AppKit) over MS `yêu thích`; `Hiển thị mục ưa thích` matches `menu.go.showFavorites`. jump to that
  favorite → `… để đi tới thư mục đó`.
- Row `0` quotes `favoritesAddCurrent`, apart from `commands.favoritesAdd.label`. `favoritesCantAddHere` opens like
  `favoritesAlreadyAdded`, says `trỏ tới`, names no protocol.

## macOS từ chối tháo ổ đĩa (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`, `errors.eject.otherProcesses`)

- Shared ACTIVE frame `… vẫn đang dùng thứ gì đó ở đó. Hãy …, rồi thử lại.` over AppKit's passive
  `đang được “%@” sử dụng` (needs quotes); `{app}` bare. A share or phone may be meant, so `ở đó`, never `ổ đĩa này` /
  `tháo lại`.
- `…ByApp` / `…ByApps`: `Hãy đóng ứng dụng này` / `các ứng dụng này`; Eject all → `Tháo tất cả` (Finder `N199.1`); a
  volume beside its drive → `phần của ổ đĩa`, since both are `ổ đĩa`; `otherProcesses` → `các tiến trình khác`. called →
  `có tên` (Finder); the "(We don’t have …)" aside matches in `…ByProcess(es)`.
- `ảnh đĩa`: Disk Utility, `DiskImages.framework` (sweep 2026-09-16). stored there → `nằm ở đó` over `lưu trữ`. Busy
  tooltips: `vẫn đang làm việc với` (macOS `Đang làm việc với %@`), `một phút` / `một chút` kept apart. send a report →
  `gửi báo cáo` (the button).

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `menu.context.selection` is a NOUN → `Lựa chọn`, ❌ not `Chọn`. Each `menu.select.*` / command twin stays identical
  bar apostrophe escaping. extension → `đuôi` over Total Commander `phần mở rộng`.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`, `settings.onboarding.fullDiskAccessChoice.label`)

- A NAME (badge, aria, settings labels) is Apple's pane name, however long: it exists to be found. Prose (the FDA step)
  may say `quyền truy cập toàn bộ đĩa` (`proseAccept`); its two status titles are `exceptions`.
- `Không có …` (flat state); `bannerTitle.denied` matches the badge. `fdaBadge.ariaLabel` opens with the label: ❌
  reword neither alone. files macOS keeps to itself → `những tệp mà macOS giữ riêng cho mình`, ❌ no feature name.

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

- Shares English with four `errors.write.*.title.trash`, so all five match, lowercase `thùng rác`;
  `errors.mutation.trashRefused` capitalizes it (the location).

## Nội dung chỉ có trực tuyến (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- Keep all four facts, ❌ none softened: trashing would download; so only deleting the whole selection is offered; no
  copy lands in the trash; the ways out. `“Xóa”` quotes `confirmDelete`; the offline way names
  `Tải về để dùng ngoại tuyến`.

## Không có mục chia sẻ đó (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- ❌ Nothing temporary (`hiện`, `thử lại`): `Tình huống này sẽ không tự thay đổi`. The server `cho biết` over `nói`.
  Apple's bare `chia sẻ` stays `mục chia sẻ` here.

## Tên trông giống hệt nhau trên máy chủ (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

- spells them differently → `máy chủ lưu chúng theo cách viết khác nhau` (a server stores), no Unicode talk. the one
  that's there → `mục hiện có` (the dialog's `Hiện có`). Cite `Nút Ghi đè`, not a bare label opener.

## Công tắc "Cho phép AI đám mây" (`ai.cloudConsent.*`, `askCmdr.gate.*`, `settings.ai.cloudConsent.lockedHint`)

- `Cho phép AI đám mây` (TCC `Cho phép` + `settings.ai.provider.opt.cloud`) reads as an imperative, so off-state bodies
  repeat it; `lockedHint` quotes it. AI service → `dịch vụ AI`, apart from `nhà cung cấp`.
  `settings.askCmdr.enabled.label` = `Ask Cmdr`.

## Thoát toàn màn hình (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- `toàn màn hình` (AppKit), sentence-cased; the toast's switch label and the setting match.

## Dòng chờ trong menu (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- Loading rows follow macOS's `Đang …`; the empty row is sentence-cased (macOS: `Không có Dịch vụ Áp dụng`).

## Di chuyển xong nhưng bản gốc vẫn còn (`errors.write.sourceNotRemoved.*`, `errors.write.permissionDenied.*.source*`, `errors.write.deletePending.message`)

- Both failed steps are `xóa bản gốc` (it really leaves the disk). `Không mất gì cả` repeats
  `deviceDisconnected.sided.destination.copy`.

## Ký hiệu đổi tên hàng loạt (`multiRename.placeholderHelp.*`, `multiRename.searchOptionHelp.*`)

- Start → `Bắt đầu từ` (Finder `Bắt đầu số từ:`, macOS 27.0.1); [P]/[G] → `thư mục chứa`; step → `bước nhảy`. caret →
  `con trỏ văn bản` (`settings.viewer.showTextCursor`); On/Off → `Bật` / `Tắt`.

## Sắp xếp hàng đợi thao tác (`commands.queueMove*`, `queue.reorder.*`)

- `Đưa … lên trên / xuống dưới trong hàng đợi` over `di chuyển` (Move); in line → `hàng đợi`, never `dòng`.

## Máy chủ NFS (`servers.sheet.nfs*`, `servers.refusal.export*`, `servers.refusal.reservedPortRequired`, `servers.refusal.noNfsV3`, `servers.sheet.identityLockedNfs`, `servers.hub.nfsHint*`)

- export → `thư mục chia sẻ` over a bare `export` (only `/etc/exports` admins know it) and `mục chia sẻ` (the field
  takes a folder path). No pile source; `tentative`, in the review queue.
- The NAS switch “Allow connections from non-privileged ports” stays English (no vi NAS UI to quote); `insecure` is the
  literal option name.

## Trường đa phương tiện (`multiRename.mediaField.*`, `multiRename.mediaPicker.*`, `multiRename.mediaEditor.*`)

- Field names copy macOS Photos/Music vi (`Ngày chụp`, `Kiểu máy ảnh`, `Số bản nhạc`); make → `Nhãn hiệu máy ảnh`
  (Photos `Nhãn hiệu thiết bị`); field → `trường` (MS); media → `đa phương tiện` over AppKit's bare `phương tiện`.
