# vi review queue

Open questions for a future native Vietnamese reviewer. Not translator input: every item below already ships a reasoned
value, recorded in `terms.json` or `decisions.md`. Remove an item once a reviewer settles it, and move the settled
wording into `terms.json` (and `decisions.md` when the reason is worth keeping).

## Voice

- **`bạn` for "you"** (high confidence, confirm the feel): Vietnamese has no neutral pronoun, and `bạn` can read
  slightly distant or flat to a native ear. Every major product accepts that tradeoff, since any kinship term would be
  wrong for most users. The catalog drops the pronoun wherever the sentence reads fine without it.
- **`Tiếp tục làm việc`** (`main.quit.keepWorking`): confirm it reads unambiguously as "you keep working" and never as
  "resume the operations".
- **`dọn dẹp`** for "clears away" a leftover partial file (`main.quit.*`): chosen over `xóa` so a quit-dialog
  reassurance doesn't read as deleting the user's file; confirm the register.
- **`… cũng vậy`** (`fileExplorer.rename.chainKeptOriginalNameAndOthers`): `“{name}” cũng vậy: {reason}` keeps English's
  two-clause scoping with the reason bound to the one file; confirm the "likewise" before a colon reads natural.
- **`và nó sẽ được thêm vào chính báo cáo mà nhóm đã có`** (`errorReporter.amend.*`): `chính … mà` stresses "the same
  report, not a second one"; no pile sentence to lean on.
- **`có gì đó đang nằm giữa bạn và máy chủ`** (`servers.hostKey.changedBody`, `servers.paneState.hostKeyChangedHint`):
  everyday wording for a man-in-the-middle, no source; confirm it isn't alarming or vague.
- **`Cách làm`** for the one-word "How" link (`adb.*`): the shortest form that keeps the meaning; no source has a bare
  "How".
- **`Thư mục hiện ra đầu tiên khi bạn mở máy chủ.`** (`servers.sheet.startFolderHelp`): describes what the user sees
  rather than translating "Where the server opens".

## Terms

- **pane → `khung`**: three sources, three words (Total Commander `bảng`, Microsoft `ngăn`, the catalog `khung`). Kept
  for catalog consistency.
- **volume → `ổ đĩa`**: no clean macOS "volume" string; `phân vùng` means a partition.
- **the focused pane**: the palette descriptions say `khung đang chọn` (`commands.navGoToPath.description`,
  `commands.favoritesAdd.description`, `commands.favoritesOpen.description`), the dialog tooltips say
  `khung đang hoạt động` (`search.action.*.tooltip`, `selection.action.*.tooltip`). The one Tier-1-sourced option is
  `khung có tiêu điểm` (Finder "Đặt tiêu điểm vào trường tìm kiếm", MS focus → `tiêu điểm`). Pick one and sweep.
- **Deferred whole-catalog migrations to pile-ideal forms**: `bấm đúp` → `bấm kép` (MS and macOS `kép`), `thư mục cha` →
  `thư mục chứa` (Finder's Enclosing Folder), `đuôi tệp` → `phần mở rộng tệp` (macOS). Each is catalog-consistent today;
  switching is one sweep, never a partial split.
- **`Lần trước`** (crash dialog) against Apple's `Lần cuối cùng bạn mở %@` (AppKitErrors).
- **`beta công khai`** for "open beta": confirm over keeping the English phrase.
- **`nhảy đến` / `nhảy tới`** for "jump to": both ship (`downloads.*` vs `commands.navFirstInFull.label`,
  `fileExplorer.typeToJump.ariaLabel`); `licensing.acknowledgements.jumpTo*` say `Chuyển đến`. Pick one.
- **Coined or descriptive terms**: `bảng lệnh` (command palette), `bộ chuyển ứng dụng` (app switcher), `phím bổ trợ`
  (modifier key), `huy hiệu` (badge and the corner chip), `thông báo nhỏ` (toast), `lần truyền` (a transfer),
  `Bỏ lưu trữ` (unarchive a chat), `chu trình đổi tên` (rename cycle), `kho ảnh` (photo archive), `thư mục bắt đầu`
  (start folder), `tiến triển` (in "no progress"), `đang đứng yên` (a stalled transfer), `vị trí chụp` and the
  classifier `một bức ảnh` (next to `máy ảnh`).
- **`với tư cách {username}`** (`servers.hub.shareAccount`, `.guestAccount`, `fileExplorer.network.share.signInAs`,
  `.useGuest`): natural with `khách`, slightly formal after a person's name; alternative `dưới tên`. And
  `Không bắt buộc` for the Optional placeholder against the catalog's parenthesized `(tùy chọn)`.
- **Spaces** (`shortcuts.system.spaces`): kept English, unverified whether macOS vi localizes it.
- **`bộ phận IT`** for "your IT team" (`ai.translateError.managed.body`, `ai.managed.hostNotAllowed`,
  `askCmdr.error.managedByOrganization`): no Tier 1 source (macOS says `quản trị viên`, "administrator"); confirm it
  reads everyday over `đội IT` / `bộ phận CNTT`.
- **List commas**: newer keys drop the comma before `và` / `hoặc`, older ones keep it. Decide a convention and sweep.
- **S3 wording** (`servers.sheet.s3*`, `*coldStorage*`): `bộ chứa` (bucket, MS + Google vi) against the English `bucket`
  many Vietnamese devs say; `Lưu trữ lạnh` / `kho lưu trữ lạnh` for "Archived"; `tự triển khai` for self-hosted.
  `servers.sheet.s3GcsKeyHelp` keeps Google's `Interoperability` tab name English: the vi Google Cloud console's own
  label is unverified.
- **Rollback captions** (`operationLog.dialog.rollbackOf`, `.rollbackOfUnlisted`, `.latestRollback`):
  `Hoàn tác cho thao tác “…” lúc {time}` puts `thao tác` before the quote so `lúc {time}` binds to the undone operation,
  not the rollback row. Confirm it reads as a caption, not a command.
- **`Khung đích = khung nguồn`** (`commands.paneClone.label`, `menu.view.clonePane`, tentative): Total Commander’s name
  for Clone pane, because `nhân bản` is Duplicate and `sao chép` is Copy. Only TC has it (no Double Commander vi);
  confirm a Vietnamese reader takes it as "show the same folder in the other pane".
- **Multi-rename wording** (`multiRename.*`): `mặt nạ tên` for the name mask (TC 6602 `Mặt nạ đổi tên`, MS mask →
  `mặt nạ`) may read technical to a non-TC user; `ký hiệu` for the `[N]`/`[C]` placeholders follows the catalog's
  `Ký hiệu định dạng`; `tùy chọn đặt trước` for a saved rename preset (the tentative preset ruling) runs long on
  `Lưu`/`Xóa`/`Tên tùy chọn đặt trước`, and `thiết lập đã lưu` is the alternative; `Viết Hoa Đầu Mỗi Từ` is title-cased
  on purpose to show the result (EN `Every Word Capitalized`); `Bỏ dấu` for Remove diacritics.

## Layout

- **Overflow checks against `en-XA`**: the queue-row status cell (`fileOperations.transferProgress.stallNotice`, about
  3× the English; fallback `Đứng yên {duration}`), the trash toast pair `Hoàn tác` / `Đi tới thùng rác`, the
  `Xem hoặc thêm ghi chú vào báo cáo` toast button beside `Đổi cài đặt`, and the long online-only warning band
  (`fileOperations.delete.cloudOnlineOnly*Warning`).
