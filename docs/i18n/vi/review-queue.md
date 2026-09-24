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
- **`… cũng vậy`** (`fileExplorer.rename.chainKeptOriginalNameAndOthers`): the sentence-final "likewise" keeps English's
  two-clause scoping. Fallback if it reads thin: `và {othersText} tệp khác cũng giữ nguyên tên`.
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
- **Full Disk Access in running prose**: a string that NAMES the setting says `quyền truy cập đầy đủ vào ổ đĩa` (Apple's
  pane name); the FDA step's prose (`onboarding.stepFda.*`, `askCmdr.wake.needsFullDiskAccess`) still says
  `truy cập toàn bộ đĩa`. Decide whether the prose moves onto the pane name too.
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
- **Spaces** (`shortcuts.system.spaces`): kept English, unverified whether macOS vi localizes it.
- **List commas**: newer keys drop the comma before `và` / `hoặc`, older ones keep it. Decide a convention and sweep.

## Layout

- **Overflow checks against `en-XA`**: the queue-row status cell (`fileOperations.transferProgress.stallNotice`, about
  3× the English; fallback `Đứng yên {duration}`), the trash toast pair `Hoàn tác` / `Đi tới thùng rác`, the
  `Xem hoặc thêm ghi chú vào báo cáo` toast button beside `Đổi cài đặt`, and the long online-only warning band
  (`fileOperations.delete.cloudOnlineOnly*Warning`).
