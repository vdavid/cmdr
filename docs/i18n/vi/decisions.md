# vi decisions

Distilled rulings behind `terms.json`: "X over Y because Z", each defending a form against a well-meant "fix". The brief
pulls a section when its heading cites a batch key, so every heading cites its keys in backticks. Evidence tiers: macOS
(the pile or the live bundles) over Microsoft over GNOME / Thunar / Dolphin / Total Commander, and catalog consistency
over a pile-ideal form that would fork a term mid-catalog.

## Tên mục Cài đặt, nhóm và phạm vi phím tắt (`settings.section.*`, `shortcuts.scope.*`, `fileExplorer.navigation.group*`)

- A string that names a section, group, or scope copies the label from these keys verbatim; two-part section names join
  with `&` (`Điều hướng & thao tác tệp`, like `Cập nhật & quyền riêng tư`).
- A Settings path is `Cài đặt › <section>`; the AI family (`ai.translateError.*`, `ai.cloudConsent.*`, `askCmdr.gate.*`)
  and `fileExplorer.quickLookHint.configurable` keep EN's `>` because their `@key` asks for it.

## Giọng lỗi: các cụm đã chốt (`errors.*`, `updates.failure.*`, `fileExplorer.network.browser.status.error`, `licensing.error.*Hint`)

- "Here's what to try:" → `Bạn có thể thử:` over `Đây là những cách để thử:` (word-for-word and longer).
- couldn't → `không thể` / `không … được`, never a bare `lỗi` / `thất bại` label. "Something went wrong" →
  `Có gì đó không ổn` over `Đã xảy ra sự cố không mong muốn` (stiff, MS-register). "Please try again" → `Hãy thử lại`
  over `Vui lòng thử lại` (bureaucratic); "Try again?" stays a bare `Thử lại?` so the short strings match.
- A bare "Error" status cell → `Sự cố` (its `@key` asks for the friendlier word). "fixes" → `việc khắc phục`, except
  David's own "helps me fix bugs" (`onboarding.stepBeta.openBeta`, `sửa lỗi`).

## Trạng thái hàng đợi và nhật ký thao tác (`queue.row.status`, `operationLog.status.*`, `operationLog.outcome.*`, `operationLog.initiator.*`, `indexing.eta.*`)

- Queue and log share one status set, the queue's. "Couldn't finish" → `Chưa hoàn tất được` (macOS
  `thao tác chưa hoàn tất`): a gentle status over a verdict of failure.
- AI client → `Máy khách AI` (the counterpart of `máy chủ`); "Ns left" → `còn Ns`, with `còn` leading.

## Nhãn macOS trong phần thiết lập ban đầu (`onboarding.stepFda.*`, `onboarding.stepOptional.*`, `onboarding.stepAi.*`)

- macOS labels copy Finder / System Settings exactly (`Thoát & Mở lại`, the Full Disk Access pane name per
  `full-disk-access`). "Accepting incoming connections" → `Chấp nhận kết nối đến` has no pile string: best effort.

## Gợi ý bấm đúp vào nền khung (`fileExplorer.doubleClickHint.*`, `settings.behavior.doubleClickPaneNavigatesToParent.*`)

- "I like it" → `Tôi thích`: the user speaking takes `tôi`, never David's `mình`. Pane background → `nền khung`; empty
  space around a list → `khoảng trống`; "go up a folder" → `lên thư mục cha`.

## Tệp quá lớn cho hệ thống tệp (`errors.write.filesTooLargeForFilesystem.*`, `fileOperations.errorDialog.tooLargeAndMore`)

- A drive holding files → `chứa` over `lưu trữ`, which reads as archiving. too large for X → `quá lớn đối với X`
  (GNOME).

## Hộp thoại sao chép, di chuyển và xóa (`fileOperations.transferDialog.*`, `fileOperations.delete.trashSwitch`, `fileOperations.delete.confirmDelete`, `queue.row.label`)

- The conflict-policy radios (`transferDialog.policy*`) keep an exact `=1 {…}` arm because the WORDING changes for one
  clash ("Skip" vs "Skip all"); it isn't the forbidden English-shaped `one` arm. Without it one clash read
  `Bỏ qua tất cả`.
- `delete.trashSwitch` reads exactly like the `transferDialog.titleVerbOnly` trash arm, so switch and button pair up.
  `Thùng rác` is capitalized only where a string names the Trash location, lowercase inside an action.
- "From" / "To" headings → `Từ` / `Đến` (Total Commander vi 662/663); "This folder doesn't exist yet" → `chưa tồn tại`,
  pairing with `đã tồn tại`.

## Duyệt và chỉnh sửa tệp nén (`fileExplorer.archiveEnterMenu.*`, `fileExplorer.readOnly.*`, `settings.archives.*`, `fileOperations.archivePassword.*`, `errors.listing.archiveUnreadable.*`)

- A browsable zip / tar / 7z is `tệp nén` over GNOME `kho lưu trữ` and macOS `Bộ lưu trữ`, which both read as backup
  storage.
- The `settings.archives.*.description` rows share one frame, `Nhấn Enter sẽ làm gì với tệp …`; keep new rows on it.

## Nén (`commands.fileCompress.*`, `fileOperations.transferDialog.toggleCompress`, `fileOperations.transferDialog.confirmCompress`, `fileOperations.transferDialog.pathErrorNotZip`, `settings.archives.compressionLevel.*`)

- The NEW archive the Compress dialog writes is `tệp lưu trữ` (Finder "Zip archive" → `Tệp lưu trữ Zip`, cited in
  `pathErrorNotZip`); every archive the user browses stays `tệp nén`. Not a drift: two different objects.

## Dán nội dung bảng nhớ tạm thành tệp (`settings.fileOperations.pasteClipboardAsFile.*`, `fileExplorer.clipboard.pastedAsFile*`)

- "as a file" → `thành tệp` / `thành {filename}` over `dưới dạng`: tighter, and it says "turn into".

## Nhật ký thao tác (`operationLog.*`, `commands.logOperationLog.*`)

- Every rollback state uses the one verb `hoàn tác` (`Không thể` / `Có thể` / `Đang` / `Đã` / `Đã … một phần`), so the
  column reads as one scale.

## Ask Cmdr: trò chuyện, chi phí và công cụ (`askCmdr.*`, `settings.askCmdr.*`, `settings.advanced.logLlmCalls.*`, `commands.askCmdrToggle.*`)

- Archive / unarchive a chat → `Lưu trữ` / `Bỏ lưu trữ` (Finder `AR40`), never the zip `tệp nén`.
- free (of charge) → `miễn phí` over MS `tự do` (free as in freedom); estimate → `ước tính` over MS `báo giá` (a sales
  quote); cost → `chi phí` over `giá vốn`.
- The "file history" tool reads the operation log, so it's `lịch sử thao tác`; if English ever splits the two, don't
  fork silently. Consent-screen items carry no trailing period (their `@key`).

## Lập chỉ mục hình ảnh (`settings.mediaIndex.*`, `fileExplorer.imageIndex.*`, `search.imageResults.*`)

- The feature is `hình ảnh`; a concrete photo count is `ảnh` (English splits the same way). A photo NAS ("photo
  archive") → `kho ảnh`, never `tệp nén`. `drive.indexing` fronts `Trên ổ đĩa này, …` to avoid a double `trên`.

## Xem lại việc đổi tên hàng loạt (`askCmdr.renameReview.*`)

- review → `xem lại` (AppKit "Review Changes…"); "needs attention" → `cần được xem lại`, since macOS vi never uses
  `chú ý`. "Ask Cmdr to prepare it again" → `nhờ Cmdr` over `yêu cầu`, which reads formal and makes the brand an object.
- rename cycle → `chu trình` (the graph sense) over MS `chu kỳ` / `vòng tròn`; one row → `lần đổi tên này`.

## Lập chỉ mục ổ đĩa đang tắt (`fileExplorer.navigation.driveIndex.refusedIndexingOff`, `.tooltipIndexingOff`, `.menuIndexingOffNote`, `settings.indexing.masterOffNote`, `.overriddenBadge`)

- A flat present state takes `không`, never `chưa`: `không có ổ đĩa nào được lập chỉ mục`; `chưa` only where the
  sentence means "not yet". "Off with X" → `Tắt theo X`.

## Tên tiện ích macOS, mục chia sẻ, liên kết mềm, beta công khai (`errors.listing.*`, `settings.network.*`, `settings.behavior.*Seen.*`, `queryUi.results.live.*`, `settings.mediaIndex.*`)

- Apple localizes its utilities, so error advice says `Tiện ích ổ đĩa`, `Sửa nhanh`, `Giám sát hoạt động`, `Xem trước`
  (verified on macOS 27.0, `InfoPlist.loctable` / `MainMenu.loctable` of each app, 2026-09-24); Spotlight, Mission
  Control, and TextEdit stay English because Apple keeps them.
- An internal "has been shown" flag says `hiển thị`; "show up here" says `xuất hiện`. A search-walk "scan" is `quét`,
  "walking" is `duyệt`.

## Các quyết định lẻ khác (`commands.handler.zoomResetHintMenu`, `askCmdr.error.notConfigured`, `askCmdr.wake.needsApiKey`)

- The user's machine in prose → `máy Mac`, never a bare `Mac` as a noun phrase. A macOS sidebar → `thanh bên`; Cmdr's
  own side panel → `khung bên` (`side-panel`).
- placeholder (template token) → `phần giữ chỗ` over MS `chỗ dành sẵn` (reads "reserved space"); preset →
  `tùy chọn đặt trước`, since a bare `đặt trước` reads "reserved".
- Azure deployment / resource → `bản triển khai` / `tài nguyên`: Microsoft's own product, so MS outranks macOS.
- `View > Zoom > 100%` in `commands.handler.zoomResetHintMenu` stays English: a literal menu path, per its `@key`.
- "at any time" → `bất cứ lúc nào` over macOS `bất kỳ lúc nào`: both correct, the catalog ships the first everywhere.
- "one" standing for a just-named thing → `một cái` (`askCmdr.error.notConfigured`, `askCmdr.wake.needsApiKey`), over
  repeating the heavy noun.

## Chỉ mục ổ đĩa: lượt kiểm tra thay đổi (`indexing.run.changeCheck`, `indexing.step.updateFileList`, `fileExplorer.navigation.driveIndex.tooltipCoalescedCheckRunning`)

- Run-kind headers are verb phrases (`Kiểm tra thay đổi`, like `Cập nhật nhanh`); "the check" is `lần kiểm tra`, the
  phrase `tooltipCoalesced` already uses.

## Lần truyền bị đứng yên: thông báo trên hộp thoại + hàng đợi (`fileOperations.transferProgress.stall*`, `fileOperations.transferProgress.close`)

- "no progress" → `không có tiến triển` over macOS `tiến trình` (this locale's word for an OS process, so it misreads)
  and MS `tiến độ` (unidiomatic negated). The progress bar itself stays `tiến trình`.
- "has stopped moving" → `đang đứng yên` over `đã dừng` (it hasn't stopped) and `treo` (reads as a crash).
- "still open" → `vẫn đang mở` over macOS `đang được sử dụng`, which says something ELSE holds the file.
- `stallNotice` feeds the dialog and the narrow queue row; if it clips the row, shorten both (`Đứng yên {duration}`).

## Đường dẫn đã sao chép: xác nhận bảng nhớ tạm (`fileExplorer.clipboard.copiedPath`)

- The path shows on its own line below, so the sentence ends in a colon and stands without it; "it's now on your
  clipboard" folds into `vào bảng nhớ tạm` over a clumsy `nó`.

## Hàng đợi thao tác: "operation queue" (`queue.windowTitle`, `queue.heading`, `queue.list.aria`, `queue.row.*`, `commands.queueShow.*`, `fileOperations.transferProgress.queue*`)

- The queue lists every kind of job, so it's `Hàng đợi thao tác` (pairs with `Nhật ký thao tác` in the View menu); the
  copy / move dialog keeps the narrow `lần truyền`. The test is the surface, as in English.
- The bare plural heading is `Các thao tác` over a bare `Thao tác`, which collides with `transferDialog.operationAria`
  ("Action") and mismatches `queue.empty.body`.
- The queued toast says `thao tác` three times on purpose: a `nó` in the middle would bind to the jobs ahead of it and
  invert the meaning.

## Huy hiệu tiến trình ở góc + thông báo chưa hoàn tất (`queue.chip.*`, `queue.failureToast.*`, `queue.row.dismiss*`, `queue.toolbar.dismissAll`)

- dismiss → `Bỏ qua`, the catalog's dismiss word, over `Xóa` (a delete row with a `Xóa` button reads "delete again") and
  `Gỡ`. It also renders Skip; the two never share a surface, so don't split them.
- "Couldn't finish <verb>" → `Chưa hoàn tất được thao tác <verb>`: the noun is load-bearing, because `được` + bare verb
  is a PASSIVE (`Chưa hoàn tất được xóa` = "hasn't finished being deleted"). The `other` arm keeps `thao tác` too.
- A counted noun takes no `các` (`{countText} thao tác chưa hoàn tất được`); `được` stays for the "couldn't".
- "to {destination}" → `vào`, never `sang` (the catalog's switch / convert word). `chip.tooltip` pairs `=0 {}` with
  `other`, and each optional clause carries its own leading space so no combination doubles a space or dangles a `·`.
- A string that has to say "chip" says `huy hiệu`, like the Settings labels for Cmdr's other chips.

## Lời nhắc xung đột đứng riêng: dòng ngữ cảnh + ghi chú tạm dừng (`fileOperations.operationConflict.context`/`.pausedNote`)

- Copy and move both take `vào` although Finder's move lines use `đến`: the catalog settled one destination preposition.
  The generic arm says `trong` (work inside the folder). `archive_edit` names the archive when there's a destination.
- "Everything else is paused" → `Các thao tác khác đã tạm dừng` over `Mọi thứ khác`, which reads as "Cmdr is frozen".
  Answer (the user) → `trả lời`; `phản hồi` is for a machine.

## Nút "Chạy nền": trạng thái hàng đợi trống của nút Hàng đợi (`fileOperations.transferProgress.background`/`.backgroundAria`)

- A control says `Chạy nền`; a sentence says `chạy ở chế độ nền`. A bare `Nền` (Total Commander's calque) names a
  backdrop in this app (`nền khung`), and the full form doesn't fit beside `Hàng đợi`.
- `backgroundAria` = `Giữ chạy nền` so it contains the label (WCAG 2.5.3); any rewording must keep that containment.

## Hộp thoại thoát khi thao tác đang chạy (`main.quit.*`)

- "Quit now" → `Thoát ngay` over macOS `Vẫn Thoát` ("Quit Anyway" overrides an objection; this button skips a timer).
- "Keep working" → `Tiếp tục làm việc` over `Hủy` (on a list of running operations it reads as cancelling THEM) and
  anything with `sau` / `nhắc lại` (the countdown is deleted, not deferred).
- "clears away" the leftover → `dọn dẹp` over `xóa` (a reassurance mustn't read "Cmdr deletes your file") and `Dọn sạch`
  (Finder's Empty Trash). The half-written file is `tệp ghi dở còn sót lại`, as in `showStagingTempFiles`.
- The body ❌ never opens on `Chỉ mục …`, which reads "the index". "Whatever's finished" → `Những gì đã xong`, never
  `Mọi thứ` (scopes to the whole app).
- "a restart or logout" takes `máy` (`khởi động lại máy`): a bare `khởi động lại` means restarting the app in this
  catalog. `countdownAria` has no visible label, so it may be reworded freely.

## Usage stats: bỏ "ẩn danh", nêu rõ "một mã định danh ngẫu nhiên" (`settings.analytics.enabled.label`/`.description`, `settings.updates.emailPrivacyNote`, `onboarding.stepBeta.analyticsLede`/`.analyticsTitle`, `onboarding.stepBeta.emailNote`)

- The stats carry a stable per-install id, so ❌ never `ẩn danh`, and ❌ never the jargon `bút danh` / `giả danh`. A
  random id → `một mã định danh ngẫu nhiên` over a bare `mã ngẫu nhiên` (reads as a coupon or PIN). tied to → `gắn với`.

## Câu hỏi làm dừng một hàng trong hàng đợi + hộp thoại hoàn tác (`queue.row.statusAwaitingAnswer`/`.awaitingAnswerTooltip`, `fileOperations.rollbackConfirm.*`, `transferProgress.foregroundBusyToast`/`.rollbackTooltip`)

- "Needs your answer" → `Cần bạn trả lời`, ❌ never opening on `Đang chờ` (the queued status in the same column). The
  prompt is a `câu hỏi`, over `lời nhắc` (a reminder).
- "Stop" in the rollback tooltip → `Dừng lại`, ❌ never `Hủy`: that IS Cancel, which keeps the finished files.
- "bring this one up" → `hiện … lên` (phrasal); the row's Show button stays `Hiển thị`.

## Đổi tên liên tiếp: thông báo gộp khi nhiều tệp giữ nguyên tên (`fileExplorer.rename.chainKeptOriginalNameAndOthers`)

- "and so did …" → sentence-final `… cũng vậy` over merging the subjects: `{reason}` describes ONE file, and a merge
  would spread it across all of them. "and N other files" → `và {othersText} tệp khác` (Finder), no `các`.

## Đổi tên không xác nhận được + tên không dùng được (`fileExplorer.rename.unconfirmed`/`.unconfirmedAndOthers`, `fileOperations.validation.nameNotUsable`)

- `unconfirmed*` says Cmdr doesn't know, so it takes the `Chưa xác nhận được … vẫn có thể đã hoàn tất` frame of
  `fileOperations.mkdir.timeoutMessage`; ❌ never `vẫn giữ nguyên tên` (that's `chainKept*`, a certainty).
- The renames (plural) → `các lần đổi tên`, over `các việc đổi tên` (`việc` doesn't count that way).
- `nameNotUsable` → `Không thể dùng tên … đó`, no final period (it's joined into a longer toast), and no guessed reason
  like `không hợp lệ`.

## Thao tác được đề xuất: hộp thoại cho những gì Ask Cmdr đề xuất (`suggestedOps.*`, `commands.suggestedOpsShow.*`, `askCmdr.decision.*`)

- approve → `Phê duyệt` over macOS `Chấp nhận`, which belongs to receiving AirDrop files; reject → `Từ chối`.

## Nhân bản: lệnh sao chép ngay trong cùng thư mục (`commands.fileDuplicate.*`)

- duplicate → `Nhân bản` (Finder File menu, verified on macOS 26.6.1, 2026-08-19), distinct from F5 `Sao chép`.

## Menu gốc: thanh menu, menu chuột phải, tiêu đề cửa sổ (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)

- Source: macOS Finder `vi.lproj` (`MenuBar.strings`, `LocalizableMerged.strings`) and Safari for tabs, verified on
  macOS 26.5.2, 2026-08-19; the English side is `en_GB.lproj`, since `Base.lproj` holds only compiled nibs.
- Window > Zoom → `Thu phóng` and Minimize → `Thu nhỏ` duplicate zoom values elsewhere, but they sit in other menus and
  all are Tier 1.
- changelog → `Nhật ký thay đổi` names a document; Help > `Có gì mới` names news. Keep both.
- Tag row: `Gỡ bỏ “{color}”` over Finder's `Xóa “^0”` (`TG6`), since removing a tag deletes nothing (`remove → gỡ bỏ`).
- A busy item keeps its label and adds ` (đang bận)` (`menu.volume.*Busy`); ❌ no second marker (`đang dùng`, `bận`).
- `menu.zoom.percent*` and `menu.view.askCmdr` are identical to English on purpose (`sameAsSourceJustification`).

## Thông báo dự phòng khi phải dùng kết nối SMB của macOS (`fileExplorer.network.osMountFallback.*`)

- native → `tích hợp sẵn` over MS `riêng` (`riêng của macOS` reads "macOS-only"); siblings say `kết nối hệ thống` when
  SMB needn't be named.
- "4x slower" → `chậm hơn 4 lần so với …` over `chậm gấp N lần` for a two-sided comparison; no pile source has
  multipliers, so it's `tentative`.

## Lỗi khi đổi tên / tạo mới (`errors.mutation.*`, `errors.volume.*`)

- System Integrity Protection → `tính năng Bảo vệ Toàn vẹn Hệ thống` (Finder `ET6`, 2026-08-23): Apple localizes it, so
  it isn't do-not-translate. Get Info → `cửa sổ Lấy thông tin`.
- "The destination can't hold that name" → `Đích không dùng được tên đó`, over `không hợp lệ`, which implies a specific
  rule this catch-all can't know.
- `errors.mutation.timedOut` isn't a failure (the change may have happened): `Ổ đĩa vẫn chưa phản hồi, nên …`.
  `errors.volume.deviceSessionReset` isn't an unplug: ❌ never `rút` / `tháo` there.
- `errors.mutation.notFound` and `errors.volume.notFound` share one English value, so they share one translation.

## Lỗi khi chuyển vào Thùng rác (`errors.mutation.trash*`)

- "has no Trash" → `không có Thùng rác, nên cách duy nhất là xóa vĩnh viễn`, over `không hỗ trợ` (that's
  `errors.volume.notSupported`, a different claim). "macOS wouldn't move this" → `macOS đã từ chối chuyển mục này`, kept
  short because the reason shows under the technical details.

## Ba biến thể phần thân của hộp thoại báo cáo sự cố (`crashReporter.dialog.body.*`)

- Only `.ended` may say Cmdr quit. `.keptRunning` must say it kept running (`nhưng vẫn tiếp tục chạy`, AppKit
  `NSExceptionAlert`); `.unknown` must claim neither.
- All three share the second sentence verbatim (`Đây là báo cáo kèm chi tiết …`, a bare `báo cáo`), so only the opener
  changes.

## `thoát bất ngờ`, `ở chế độ nền`, và hai hướng đã loại (`crashReporter.dialog.body.ended`, `crashReporter.dialog.privacyNote`, `crashReporter.dialog.title*`, `settings.updates.crashReports.description`)

- quit unexpectedly → `thoát bất ngờ` (AppKit `AppKitErrors`) over the unsourced `thoát đột ngột`.
- `sự cố` means "problem" (Finder, AppKit use it for problems the app survives), so `gặp sự cố` never claims Cmdr quit;
  the verb `thoát bất ngờ` carries the quitting. `báo cáo sự cố` is "crash report", so "a report" is a bare `báo cáo`
  (`title.report`, and the crash-reports description, which covers both outcomes). The setting's LABEL keeps
  `Gửi báo cáo sự cố`: it's a name.
- in the background → `ở chế độ nền` over ❌ `ngầm` (the pile's hits are all underground) and ❌ `hậu trường` (no
  source). Most pile hits for `nền` are visual (`hình nền`), so they're no evidence for "run in the background".

## Lỗi khi tháo ổ đĩa / ngắt kết nối (`errors.eject.*`)

- The values land after the colon of `Không thể tháo {volumeName}: …` / `Không thể ngắt kết nối: …`, so they state only
  the reason and the next step, never the refusal again.
- in use → `đang dùng` in these sentences: the everyday form of Finder's `đang được sử dụng` (`NE66`), which the voice
  rule prefers in prose.
- "not removable" → the TYPE frame `Ổ đĩa này không phải loại có thể tháo` (Finder "Removable" → `Có thể tháo`) over
  `không tháo được`, which sounds like a one-time refusal.
- loose "moving files" (copy, move, or delete) → `chuyển tệp`, over `di chuyển tệp`, which narrows to the Move
  operation.
- unplug → `rút … ra` rests on catalog consistency alone (no pile string); ❌ never in
  `errors.volume.deviceSessionReset`. "once it's idle" → `khi không còn bận` (`rảnh` is for the user being idle).
- `errors.eject.timedOut` isn't a failure (same frame as `errors.mutation.timedOut`); `errors.eject.unexpected` shares
  `errors.mutation.unexpected`'s English, so it shares its value.

## Apple CÓ dịch "Get Info" và "Locked" sang tiếng Việt (`errors.write.fileLocked.suggestion.mac`, `errors.write.permissionDenied.suggestion.deleteMac`, `errors.listing.noPermissionErrno.suggestion`, `errors.listing.permissionDenied.suggestion`)

- macOS vi localizes these, so a user can only find them by the Finder text: Get Info → `Lấy thông tin`, Locked →
  `“Đã khóa”` (Finder `AXNODE1`, `NE18`), Sharing & Permissions → `phần Chia sẻ & quyền` (verified on macOS 26.5.2,
  `InfoWindowPermissionsView.strings`, 2026-08-24). `phần` over `mục`, as Apple writes it in that sentence.

## Toast thùng rác: hoàn tác và đi tới thùng rác (`fileOperations.trash.*`, `commands.fileGoToTrash.*`)

- put back → `đưa trở lại` (Finder `N153.1` "Put Back"), over `đặt lại` (reserved for old names) and Nautilus
  `khôi phục` (Restore, broader, Tier 3).
- go to trash → `Đi tới thùng rác` (the catalog's `Đi tới` family); it's 16 characters against 11, so overflow-check the
  toast pair. "This drive doesn't keep a trash" states a fact about the drive, never a reproach.

## Thêm ghi chú vào báo cáo đã gửi (`errorReporter.amend.*`, `errorReporter.amendedToast.message`, `errorReporter.autoSentToast.viewOrAddNotes`)

- The whole family grows from `thêm` (`Thêm vào báo cáo sự cố của bạn`, `Thêm vào báo cáo`, `Đang thêm…`,
  `Đã thêm ghi chú vào báo cáo`, `Không thể thêm ghi chú của bạn.`); ❌ no second root.
- "To get your notes to the team" → `Để nhóm nhận được …`, over `chuyển` (reserved for Move) and a doubled `gửi … gửi`.
- "What was sent" twins `errorReporter.dialog.detailsToggle` (`sắp` → `đã`). A menu is named `menu <Tên>` with the
  `menu.bar.*` label (`từ menu Trợ giúp`).
- Drop one `của bạn` where two would sit in a short line; keep it where a sibling string has it
  (`ID tham chiếu của bạn là` matches `errorReporter.sentToast.message`).

## Chọn và bỏ chọn: hộp thoại Select / Deselect (`selection.*`)

- focused pane → `khung đang hoạt động` in these tooltips, over `khung đang chọn` (three `chọn` in one line, and "the
  selecting pane" is ambiguous). If "focused pane" is ever unified, `khung có tiêu điểm` is the one with a Tier 1
  source.
- The tooltips needn't contain the label (the button's aria comes from the label key), but they open with it anyway,
  like `search.action.goToFile.tooltip`.
- `selection.recent.*` mirrors `queryUi.recent.*` with `lựa chọn` for "search"; `{query}` sits last, after a colon.

## Rà soát trôi thuật ngữ toàn catalog (`menu.go.back`, `commands.navBack.label`, `onboarding.wizard.back`, `shortcuts.section.filterModified`, `licensing.about.srTitle`)

Tier 1 counts from the live bundles, macOS 26.6.2 (build 25G83), 2026-08-30. Each ruling is in `terms.json`; this is why
each one beats the form the catalog had drifted to:

- size → `kích cỡ` (macOS 33, `kích thước` 0; the file list and search results must match); only "Text size" keeps
  `Cỡ chữ`. show → `hiển thị` (147, and no `hiện` as "show": a label opening on `Hiện` reads "current…"). download →
  `tải về` (35, `tải xuống` 0; MS's form is a Windows convention).
- tab → `tab`, Finder tag → `thẻ` (Finder, Safari), so `thẻ` no longer means three things. Tag and accent colors are
  Apple's `Lam` / `Lục` / `Tía` (one set for both); the everyday `Xanh dương` / `Xanh lá` / `Tím` appear nowhere in the
  pile. Cmdr-only colors (`Hổ phách`, `Xanh chanh`, `Xanh mòng két`, `Chàm`) have no source.
- remove from a list → `gỡ bỏ`, delete → `xóa`: macOS says `Xóa` for both because Finder never shows them together; Cmdr
  does. "Remove download" really erases the local copy, so it's `Xóa bản tải về` (Finder). One form, `gỡ bỏ`, even with
  an object, so `settings.mediaIndex.chosenFolders.removeAria` contains its button label.
- "Unreachable" → `không tới được` (`tentative`, catalog-sourced), over `không kết nối được` (collides with connect) and
  `không truy cập được` (collides with access permission).
- "Low disk space" → `Sắp hết dung lượng đĩa` (macOS Photos) over `Thiếu …`, which says "not enough".
- Deliberate boundaries, ❌ don't "fix": Back in folder HISTORY → `Trở lại` (Finder Go > Back), back to a previous
  screen or step → `Quay lại` (Setup Assistant); Modified as a date → `Đã sửa đổi`, a user-changed shortcut
  (`shortcuts.section.filterModified`) → `Đã thay đổi`.

## Menu wording, System Settings panes, email placeholder (`menu.app.hideOthers`, `commands.appHideOthers.label`, `errors.git.*`, `errors.provider.*`, `settings.updates.emailPlaceholder`, `common.attachEmailPlaceholder`, `onboarding.stepBeta.emailPlaceholder`, `askCmdr.renameUndo.undone`/`.partial`)

- Hide others → `Ẩn các mục khác` (Finder, TextEdit, Preview agree, macOS 26.6.2), over `Ẩn các ứng dụng khác`: it must
  read exactly like the row macOS shows above it.
- System Settings panes macOS localizes are written in vi (`Cài đặt chung`, `Mục đăng nhập & Phần mở rộng`,
  `Tài khoản Apple`, `Mạng`, `Dung lượng`); where a `{system_settings}`-style token exists, use it, never hand-translate
  it.
- Sample email local part → `ban@`, ASCII-folded like Microsoft vi's `ai_do@`.
- "Put the old names back" → `Đã đặt lại tên cũ cho …` over the trash verb `đưa trở lại`: nothing moves, only the name.

## Thao tác mới hoàn tác được một nửa: làm tiếp cho hết (`operationLog.dialog.finishRollBack`, `operationLog.rollback.partiallyRolledBackNotice`, `fileOperations.rollbackConfirm.titleFinish`/`.finishRollBack`, `queue.row.reversalInFolder`)

- Finish rolling back → `Tiếp tục hoàn tác` (Finder's `Tiếp tục sao chép` for an unfinished file operation), over
  `Hoàn tất việc hoàn tác`: `hoàn tất` and `hoàn tác` differ by one mark, a tongue-twister beside `Chưa hoàn tất được`.
- `vẫn` binds to `không chắc chắn` ("still isn't sure"), not to `bỏ qua`.

## Toast sau khi hoàn tác thao tác đang chạy (`fileOperations.cancelRollback.*`, `rollbackConfirm.body`)

- `cancelRollback.reason.*` share the `Giữ nguyên {name}: …` frame with `askCmdr.renameUndo.skipReason.*` so the two
  read as one feature; where the English is identical (`folderNotEmpty.*`), the values match word for word. The only
  difference: this set counts `mục` (English "item"), the rename set `tệp`.
- "Left X where it is" folds into `Giữ nguyên` (it already means "left in place"). "after Cmdr put it there" →
  `đưa nó tới đó`, over `ghi nó vào đó`, which fits only the copy.
- `failed.*` leaves the `Giữ nguyên` frame on purpose (the drive refused; it wasn't a choice): `Không hoàn tác được …`.
- `leftBehind` repeats the confirm dialogs' promise `Cmdr bỏ qua những gì nó không chắc chắn`, and the four
  `rollbackConfirm.body*` variants share that tail verbatim.
- English's "the N items" (all of them) vs "N items" (some) → `cả` in the complete one, over `tất cả`, which reads like
  a select-all button.
- `stagedLeftover.*` promises cleanup `trong một lần truyền sau`, ❌ never "next time": the cleanup skips anything under
  an hour old, so an immediate retry cleans nothing.

## Màn hình chặn khi WebKit quá cũ (`main.oldWebkit.*`)

- This HTML shell is all that user ever sees of Cmdr; `Safari`, `Mac`, and `15.4` stay verbatim (`Safari` is in
  `BRAND_WORDS`). Software Update → `Cập nhật phần mềm`.

## Thông báo về macOS cũ (`main.oldMacos.*`)

- "best effort" → `chỉ cố hết sức thôi`, deliberately not literal; the closing line is David, so `mình`.

## Ask Cmdr xem bên trong tệp: hai nhãn công cụ + hai khóa đồng ý (`askCmdr.tool.inspectFile.*`, `ai.cloudConsent.askCmdr.item.contents`, `ai.cloudConsent.askCmdr.contentsRule`, `askCmdr.empty.hint`, `settings.askCmdr.intro`)

- thumbnail → `hình thu nhỏ` (AppKit, MS, Dolphin, Thunar) over GNOME `ảnh thu nhỏ`; the `hình` also separates it from
  the `ảnh` right before it.
- EXIF "camera details" → `thông tin máy ảnh`, over `chi tiết` (macOS's expand-a-dialog button). Where it was taken →
  `vị trí chụp`, never a bare `vị trí` (the catalog's word for a path).
- "a photo's" next to `máy ảnh` → `của một bức ảnh`, so `ảnh` doesn't double up; elsewhere a bare `ảnh` (`tentative`).
- The tool labels and the consent copy share one root, `xem bên trong tệp`. "whole files" → `toàn bộ tệp`: the consent
  no longer promises "no file contents", so ❌ don't restore `chỉ đọc` / `không bao giờ đọc nội dung tệp`.

## Hai chú giải của nút Rollback (`fileOperations.transferProgress.rollbackTooltipStopAndMoveBack`, `.rollbackAlreadyLandedTooltip`)

- Rolling back a move deletes nothing, so it's `đưa trở lại`, ❌ never `xóa`.

## “Mở terminal tại đây” và bộ chọn ứng dụng (`settings.behavior.openTerminalHereApp.*`, `settings.navigationAndFileOps.card.terminal`)

- macOS vi keeps `Terminal` (`Mở trong Terminal`, Finder `N67`) and the generic loanword `terminal`, so the card title
  is identical to English on purpose. The command is `Mở terminal tại đây` on every surface.

## `Sort by relevance`: chú giải của cột kết quả tìm kiếm (`fileExplorer.columns.sortByRelevance`)

- relevance → `mức độ liên quan` (WorkflowKit, Automator; Music and TV shorten it), over AppStoreKit's `Độ phù hợp`
  (verified on macOS 26.6.2, `plutil` dumps, 2026-09-06).

## `Documents and packages`: hàng OOXML mới (`settings.archives.ooxml.*`)

- packages → a bare `gói`, so the row stays wider than the `Gói ứng dụng` row below it, as English splits "packages"
  from "app bundles".

## Trung tâm máy chủ (`servers.*`, `fileExplorer.navigation.connectionTooltip*`, `fileExplorer.navigation.disconnect*`, `fileExplorer.navigation.forget*`, `fileExplorer.navigation.server*Toast`, `commands.servers*`, `shortcuts.scope.servers`, `shortcuts.scope.places`, `menu.network.pinToSwitcher`, `.unpin`, `settings.servers.*`, `settings.adb.*`, `goToPath.dialog.opensServer`, `goToPath.dialog.addsServer`)

Evidence: the live bundles of macOS 26.6.2 (build 25G83), `plistlib` on each `.loctable`, 2026-09-06/07; the richest
source is Apple's own Connect to Server dialog (`NetAuthAgent.app`).

- A dropped connection → `Đã mất kết nối` (CFNetwork "was lost"), over `Kết nối đã bị ngắt`: `ngắt` is the user's
  deliberate `Ngắt kết nối`, and the two states must never read alike. Cmdr giving up → `đã dừng kết nối tới`, ❌ never
  `ngắt`.
- reach a SERVER → `không kết nối được tới`; reach a path, drive, or USB device → `không thể truy cập`. Servers take
  `tới` (`Đang kết nối tới {name}…`, and its sibling `reconnecting`), over macOS's `với`, so the pane's states pair up.
- sign-in method → `cách đăng nhập` over macOS `Phương thức xác thực`: English chose the everyday word. `đăng nhập`
  always has a PERSON as subject, so a server "signs in with a key" becomes `Máy chủ này dùng khóa … để đăng nhập`.
- certificate → `chứng nhận` (macOS everywhere) over MS `chứng chỉ`. key → `khóa`, fingerprint → `dấu vân tay` (Keychain
  Access `Fingerprints`), kept apart as in English. "Marked as compromised" → `bị xâm phạm`, not `bị thu hồi`.
- Forget → `Quên` on every surface, including the failure toast: macOS does the same for Wi-Fi, and it's the deliberate
  exception to `xóa` / `gỡ bỏ`. The identity hint names the actions exactly as their buttons (`quên`, `thêm`).
- Last used → `Dùng cuối`, its empty cell → `Chưa từng` (the System Settings privacy table, same column), over
  `Không bao giờ`, which is a FUTURE choice ("never do"). "Found nearby" → `Tìm thấy ở gần`, deliberately without `Đã`.
- Places (spots inside a server) → `Vị trí` (Finder Locations), ❌ never `Địa điểm`, Apple's GEOGRAPHIC places.
- One surface, one name: the volume chooser and "switcher" are both `bộ chọn ổ đĩa`, and `pinToSwitcher` says so. pin /
  unpin → `Ghim` / `Bỏ ghim` (AppKit `MenuCommands`) over `Gỡ ghim` / `Hủy ghim`.
- Passphrase → `Cụm từ mật khẩu của khóa`: `của khóa` keeps it apart from the account password in the same form.
- "check X against Y" → `đối chiếu`; a bare "check" → `kiểm tra`, so `Tôi đã kiểm tra rồi` answers
  `Đang chờ bạn kiểm tra khóa`. "something sitting between you and it" stays descriptive (`tentative`), no MITM jargon.
- rather than → `thay vì` (shipped widely), over `chứ không phải`; type into a field → `nhập`, since macOS keeps
  `gõ ký tự` for the keyboard.
- `servers.hostKey.changedBody` and `servers.paneState.hostKeyChangedHint` share one middle sentence. Where a bare
  `Hãy mở lại` could mean reopening the app, name `máy chủ`.
- "Got it" button → `Đã hiểu` (three siblings); `Đã rõ` belongs only inside `whatsNew.optOutToast`'s sentence.
- `settings.adb.status.*` say `theo dõi để phát hiện điện thoại`, since a bare `theo dõi điện thoại` reads as
  surveillance.

## Điện thoại Android qua ADB: khung kết nối, chú giải bộ chọn ổ đĩa, dòng gợi ý (`adb.*`, `settings.behavior.adbHintDismissed.*`, `settings.fileOperations.adbEnabled.*`, `settings.summary.adb`)

- Android's own vi UI wins for phone labels: USB debugging → `gỡ lỗi qua USB` (AOSP `enable_adb`, `main`, 2026-09-07),
  ❌ never `gỡ lỗi USB`; the dialog button → `Cho phép`; tap a named button → `nhấn vào`. `adb`, `ADB`, `Android SDK`,
  and `Homebrew` stay verbatim; platform tools → `bộ công cụ nền tảng Android` (Google's vi docs).
- `adb.hint.text` capitalizes `Gỡ lỗi qua USB` because it tells the reader to find that exact switch; running prose
  keeps it lowercase, as AOSP does.
- "You stopped opening your phone" → `dừng`, ❌ not `hủy`: `Hủy` is the Cancel button's label.
- The busy tooltips share one frame (`Không thể … khi còn thao tác đang chạy trên …`); the two disconnect arias match.

## Toast khi vốn không có mật khẩu nào được lưu (`fileExplorer.navigation.forgetSecretNoneToast`)

- Nothing went wrong here, so no `không thể` and no `lỗi`: `Không có mật khẩu đã lưu cho {name}.`, reusing the
  `mật khẩu đã lưu` of its `forgetSecret*` siblings.

## Tổng thời gian thử lại, tiêu đề khóa máy chủ và nút Cho phép của Android (`servers.paneState.retryTotalSeconds`/`.retryTotalMinutes`, `servers.paneState.hostKeyChanged`, `adb.connect.unauthorized`)

- The retry totals are bare fragments of `retryKeepsTrying` (no preposition, no period), and keep the one-arm plural
  wrapper so the placeholder set matches English.
- "Cmdr won't connect to {name}" → `Cmdr sẽ không kết nối tới {name}` (a standing refusal), over `đã dừng`, which sounds
  like an interrupted attempt.

## Menu chuột phải trên hàng máy chủ: `Mở` và `Sửa máy chủ…` (`menu.network.open`, `menu.network.edit`)

- `menu.network.edit` copies `commands.serversEdit.label` byte for byte (same form); open on a server row is the same
  `Mở` as `menu.file.open`, since Finder uses one verb for both senses.

## Lời mời ghim Cmdr vào Dock (`main.dockPinNudge.*`, `settings.behavior.dockPinNudgeOfferedAt.*`)

- Apple keeps `Dock` and `Finder` English but localizes the Applications folder (`thư mục Ứng dụng`); decide per label,
  never by analogy. ❌ Not MS's `thanh dock` / `đế cắm`. Title borrows Dock's own `Giữ lại trong Dock` (`KEEP_IN_DOCK`).
- decline → `Không, cảm ơn`, ❌ never `Để sau`: Cmdr doesn't ask again, so "later" would be a lie. Timing stays vague
  (`vài ngày`, `một thời gian`), ❌ never a number: the threshold can change.
- `addedButDockDidNotRestart`: the pin HAPPENED, so `chưa tải lại` (will happen), never a verdict of failure; the
  positive fact leads. `notAdded` also takes `chưa`.
- X is managed by Y → `X được Y quản lý` (Apple's passive, agent between `được` and the verb). `managedDock` keeps
  `của bạn` because it carries information.

## Menu chuột phải trên biểu tượng Dock (`menu.dock.*`)

- Source: `Dock.app/Contents/Resources/vi.lproj/DockMenus.strings` (macOS 26.6.2, 2026-09-09), which the pile lacks. An
  APP name goes bare, a FILE name gets quotes (`Ẩn %@` vs `Mở “%@”`), so `menu.dock.openCmdr` = `Mở Cmdr`.
- Connect to Server… → `Kết nối với máy chủ…` (Finder `N84`): the LABEL takes Finder's `với`, while prose says `tới`.
- `{name} ({parent})` stays as English (AppKit keeps `%1$@ (%2$@)`); ❌ no preposition inside the brackets.
- `menu.dock.searchFiles` matches `menu.edit.searchFiles` word for word: one command, two menus.

## Lời mời về “Hiển thị trong Finder” và thông báo lần đầu (`main.revealNudge.*`, `main.revealActivation.*`, `settings.behavior.reveal*`)

- The action is quoted as `“Hiển thị trong Finder”` everywhere, like its settings card. The first-time notice explains,
  ❌ never apologizes.

## Phần thiết lập ban đầu: bảng kiểm, chú giải bước, tóm tắt tùy chọn (`onboarding.*`, `settings.revealHandler.notProductionBuild`)

- Why? → `Tại sao?` (PhotosUICore, live sweep 2026-09-09; the pile has none). Learn more → `Tìm hiểu thêm`, over
  `Thông tin khác` (macOS's "More Info" panel button).
- GitHub has no vi UI, so star → `gắn sao` rests on the shipped `checklist.star` (Nautilus has `đánh sao`; don't
  switch). `repo` stays a loanword; only a full "repository" is `kho`.
- The permission is Apple's name `Mạng cục bộ`, quoted as is; ❌ don't paraphrase it into `Truy cập mạng cục bộ`, which
  the user won't find in System Settings.
- "dumber" → `kém thông minh hơn hẳn`, over `dốt hơn`, which is for people and reads as an insult.
- `stepAi.local.tooltip` quotes `stepAi.cloud.label` exactly; `onboarding.wizard.stepTooltip` keeps `+1` literal and the
  `select` arm names.
- "come and go" (dev builds) → `chỉ tồn tại tạm thời`, over the word-for-word `đến rồi đi` (`tentative`).

## Trình xem tải tệp từ điện thoại, máy chủ hoặc tệp nén (`viewer.pull.*`, `viewer.error.stoppedResponding`)

- "Fetching" → `Đang tải` (the viewer's own verb), over Finder's `Đang tìm nạp…` (metadata, technical) and `Đang tải về`
  (a file inside an archive isn't downloaded from anywhere).
- "{done} of {total}" → `{doneText} trên {totalText}` (Foundation `Progress.loctable`); Finder's `/` is for dense
  counters. "so far" → `Đến giờ đã tải …`, since `… đến giờ` alone isn't a sentence.

## Chỉ mục lỗi thời của điện thoại qua ADB (`fileExplorer.navigation.driveIndex.tooltipStalePhone`, `indexing.staleDialog.titlePhone`, `.bodyPhone`)

- The phone is still plugged in, so ❌ never `ngắt kết nối`; each value swaps one noun of its `tooltipStale` /
  `staleDialog.*` sibling. Two subjects are in play, so the body repeats `chỉ mục của điện thoại` over an ambiguous
  `nó`.

## Thư mục gốc và thư mục bắt đầu của máy chủ đã lưu (`servers.sheet.rootFolder*`, `servers.sheet.startFolder*`, `servers.sheet.nameHelp`, `servers.refusal.startFolderOutsideRoot`, `.rootNotFound`, `.startFolderNotFound`, `.saveUnconfirmed`)

- start folder → `thư mục bắt đầu` (Shortcuts' `Vị trí bắt đầu`, `tentative`), over `thư mục khởi động` (macOS keeps
  `khởi động` for startup) and `thư mục mặc định` (another sense).
- host in `nameHelp` → `địa chỉ` (the field's own label), since host and server are both `máy chủ` and would collide.
- "Where the server opens" → a plain description of what the user sees, over `Nơi máy chủ mở ra` (reads as booting).
- The two `…NotFound` share their tail word for word; `saveUnconfirmed` takes `chưa lưu gì` because a retry is safe.

## Vì sao mục chia sẻ không gắn kết được hoặc danh sách không tải được (`errors.mount.*`, `errors.shareList.*`)

- Source: the installed `NetAuthAgent.app` (`Localizable.loctable`, macOS 26.6.2, 2026-09-11), absent from the pile.
- Keys with identical English share one value (`errors.mount.hostUnreachable` / `errors.shareList.hostUnreachable`,
  `…authFailed`). A Linux distribution → `bản phân phối` (`tentative`).

## F4 and its text editor (`settings.behavior.textEditorApp.label`, `settings.behavior.textEditorApp.description`, `settings.navigationAndFileOps.card.textEditor`, `fileExplorer.edit.appMissing`, `fileExplorer.edit.launchRefused`)

- text editor (the app kind) → `trình soạn thảo văn bản` (MS), never Apple's TextEdit, which arrives in `{app}`. The
  chooser rows reuse `settings.behavior.openTerminalHereApp.*` verbatim.

## Ổ đĩa rời đi giữa chừng (`fileExplorer.navigation.driveIndex.driveLeaving`, `indexing.needsFreshScan.afterDisconnect`, `errors.write.deviceDisconnected.sided.*`)

- A user-initiated disconnect takes `được` (`đang được ngắt kết nối`); a drive that dropped takes `bị`
  (`bị ngắt kết nối`).
- "{done} of {total} files" in prose → `{done} trên {total} tệp`; they arrive preformatted in a RAW family, so ❌ no ICU
  number syntax.
- all your files → `mọi tệp của bạn`, over `toàn bộ tệp`, which `contentsRule` uses for "whole files".
- The reassuring tails repeat the shipped `vẫn nguyên vẹn` / `ở chỗ cũ` / `không mất gì cả`, and the `sided.source.*`
  pair differs by one verb only.

## A move that could not be confirmed (`errors.write.moveNotConfirmed.title`, `errors.write.moveNotConfirmed.*`)

- Nothing was lost (Cmdr kept the originals BECAUSE it's unsure), so `chưa xác nhận được` over `không thể xác nhận`,
  which sounds like a flat refusal. ❌ No failure verdict.
- "haven't moved" → `vẫn ở nguyên chỗ cũ`, over `chưa di chuyển`, which reads as the operation still waiting to run.
  "the files just moved" → `các tệp vừa di chuyển`, avoiding a doubled `đã`.

## Thư mục làm việc còn lại sau một lần di chuyển dang dở (`fileOperations.leftovers.stagingFolderKept`)

- These are the USER's files, possibly the only copy: ❌ never suggest discarding (`dọn dẹp`, `xóa` are banned here),
  unlike Cmdr's own leftovers in `cancelRollback.stagedLeftover.*`.
- found → `tìm thấy` over `phát hiện` (automatic detection); unfinished move → `chưa hoàn tất` (a task) over
  `chưa hoàn chỉnh` (an object, the stagedLeftover family); kept → `giữ nguyên chúng ở đó`, since `ở chỗ cũ` would be
  false. `ẩn` in "hidden folder" stays: it's the one thing the reader must act on.

## Menu mục ưa thích (`commands.favoritesOpen.*`, `commands.favoritesOpenByNumber.label`, `commands.favoritesAdd.description`, `fileExplorer.navigation.favoritesAddCurrent` / `.favoritesAlreadyAdded` / `.favoritesCantAddHere` / `.seeFavorites`, `menu.go.showFavorites`, `shortcuts.scope.favoritesMenu`)

- favorites → `mục ưa thích` (AppKit) over MS `yêu thích`; the menu is `menu mục ưa thích`.
- show → `Hiển thị mục ưa thích` (the command and `menu.go.showFavorites` match word for word); see → `Xem` (Finder
  toolbar tooltips).
- `seeFavorites` has `=0` (no number: "you have none") plus `other`; ❌ never restore English's `one`.
- "press a number to jump to that favorite" → `… để đi tới thư mục đó`, over repeating `mục ưa thích` in one sentence.
- Row `0` quotes `favoritesAddCurrent`; keep it apart from `commands.favoritesAdd.label`, which names no folder.
- `favoritesCantAddHere` opens like `favoritesAlreadyAdded` and says `hoạt động trên`, never naming a protocol.

## macOS từ chối tháo ổ đĩa, và Cmdr nói rõ ai đang giữ (`errors.eject.unmountRefusedBy*`, `errors.eject.otherApps`)

- The family shares `… vẫn đang dùng ổ đĩa này. Hãy …, rồi tháo lại.` in the ACTIVE voice (siblings already are), over
  AppKit's passive `đang được “%@” sử dụng`, which needs a quoted name; `{app}` goes bare.
- `…ByApp` and `…ByApps` differ only by `nó` / `chúng`: the list just before already says the number, so ❌ no invented
  number marker. The list's last item `errors.eject.otherApps` MUST be `các ứng dụng khác`: without `các`,
  `… và ứng dụng khác` reads "and one more app".
- disk image → `ảnh đĩa` (Disk Utility, `DiskImages.framework`, live sweep 2026-09-16; the pile has none), ❌ never
  shortened to `ảnh` (a photo). "stored on this drive" → `nằm trên`, over `lưu trữ` (reads as backup).
- "macOS is still working with" → `vẫn đang làm việc với` (macOS `Đang làm việc với %@`): English changes the verb
  because nothing can be closed. "Wait a minute" (macOS, long) vs "Wait a moment" (Cmdr, short) keep `một phút` /
  `một chút`.
- "send a report" → a bare `gửi báo cáo`, the button's own label.

## Select all of the same kind (`menu.select.sameKind`/`.allFolders`/`.sameExtension`/`.noExtension`, `commands.selectionSelectSameKind.*`, `menu.context.selection`)

- `menu.context.selection` is a NOUN (the submenu title) → `Lựa chọn`, ❌ not the verb `Chọn` (`menu.bar.select`).
- Each `menu.select.*` / `commands.selectionSelectSameKind.*` twin shares one English string, so they stay identical
  apart from apostrophe escaping (RAW vs ICU). extension stays `đuôi`, over Total Commander's `phần mở rộng`.

## The title-bar full-disk-access badge (`onboarding.fdaBadge.*`, `onboarding.stepAi.bannerTitle.denied`, `settings.onboarding.fullDiskAccessChoice.label`)

- A NAME of the setting (the badge, its aria, settings labels) is Apple's pane name `quyền truy cập đầy đủ vào ổ đĩa`,
  long as it is, because the badge exists to be found in System Settings. Running prose (the FDA step) may say the
  everyday `quyền truy cập toàn bộ đĩa` (`proseAccept`); its two sentence-shaped status titles are `exceptions`.
- `Không có …` over `Chưa có …`: a flat present state. `bannerTitle.denied` shares the badge's English, so they match.
- `fdaBadge.ariaLabel` opens with the label verbatim (WCAG 2.5.3); ❌ reword neither alone.
- "files macOS keeps to itself" → `những tệp mà macOS giữ riêng cho mình`, plain, ❌ never a macOS feature name.

## The trash refusal dialog (`errors.write.trashRefused.title` + its `message.*` / `suggestion.*` siblings)

- The title shares English with four `errors.write.*.title.trash` keys, so all five stay identical, lowercase
  `thùng rác` (an action). `errors.mutation.trashRefused` capitalizes it because it names the location.

## Cảnh báo nội dung chỉ có trực tuyến (`fileOperations.delete.cloudOnlineOnlyMixedWarning` / `fileOperations.delete.cloudOnlineOnlyAllWarning` / `fileOperations.delete.cloudOnlineOnlyHandedBack`)

- All four facts stay, ❌ none softened: the trash would download the files; so Cmdr offers only deleting the whole
  selection; no copy lands in the trash (the service keeps its own); the listed ways out. The quoted `“Xóa”` is
  `fileOperations.delete.confirmDelete`; the offline way out names the command `Tải về để dùng ngoại tuyến`.

## Khi máy chủ nói là không có mục chia sẻ đó (`fileExplorer.network.osMountFallback.shareNotOnServer`, `fileExplorer.pane.directConnectionShareNotOnServerToast`)

- Retrying can't help here, so ❌ nothing temporary (`hiện`, `thử lại`): `Tình huống này sẽ không tự thay đổi`. The
  server `cho biết` (reports a state) over `nói` (personifies). Apple writes a bare `chia sẻ`; we keep `mục chia sẻ`.

## Tên trông giống hệt nhau trên máy chủ (`fileOperations.transferProgress.lookAlikeHint`, `errors.listing.ambiguousName.*`, `errors.volume.ambiguousName`)

- "the server spells them differently" → `máy chủ lưu chúng theo cách viết khác nhau` over `viết` (a server stores, it
  doesn't write); no Unicode talk. "the one that's there" → `mục hiện có`, tying to the dialog's `Hiện có` labels.
- The Overwrite button is cited as `Nút Ghi đè`, so the sentence doesn't open on a bare label.

## Công tắc "Cho phép AI đám mây" và các trạng thái AI đám mây đang tắt (`ai.cloudConsent.*`, `askCmdr.gate.*`, `settings.ai.cloudConsent.lockedHint`)

- The switch `Cho phép AI đám mây` (TCC's `Cho phép` + `settings.ai.provider.opt.cloud`) doubles as a natural
  imperative, so the off-state bodies repeat it verbatim; `lockedHint` quotes it. AI service → `dịch vụ AI`, kept apart
  from the provider (`nhà cung cấp`) as English does. `settings.askCmdr.enabled.label` = `Ask Cmdr` on purpose.

## Thoát toàn màn hình bằng phím Escape (`main.escapeFullScreenHint.*`, `settings.advanced.exitFullScreenOnEscape*`)

- full screen → `toàn màn hình` (AppKit), sentence-cased, over Apple's title-cased `Toàn`. The toast's switch label and
  the setting match word for word.

## Bản gốc đã thay đổi trong khi di chuyển (`transfer.changedDuringMove`, 2026-09-25)

- **"changed during the move" → `đã thay đổi trong khi di chuyển`** · Finder `PE56` "một hoặc nhiều mục đã thay đổi
  trong khi đang ghi" (Finder `LocalizableMerged` `PE56`, macOS 26.6.2, live bundle, 2026-09-25) · `high`.
  `trong khi di chuyển`, `vẫn ở lại` và `thư mục nguồn` lấy nguyên từ chuỗi chị em `transfer.appearedDuringMove`, hiện
  cùng một toast.

## Dòng chờ trong "Mở bằng" và "Chia sẻ" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`, 2026-09-24)

## Dòng chờ trong "Mở bằng" và "Chia sẻ" (`menu.context.openWithLoading`, `.shareLoading`, `.shareNone`)

- Loading rows follow macOS's `Đang …` pattern; the empty row is sentence-cased, over macOS's title-cased
  `Không có Dịch vụ Áp dụng`.
