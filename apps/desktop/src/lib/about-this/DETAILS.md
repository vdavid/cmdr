# Tell me about this (card) details

The card for `file.tellMeAbout` (`⌘⇧I` on macOS, `Ctrl+Shift+Enter` elsewhere; the palette; File > Tell me about this;
the right-click row below Get info). Every string is a draft for David: the list with keys is
`docs/specs/tell-me-about-this-copy.md`.

## The stream into the card

`openAboutThis(subject)` calls `startAbout` (`apps/desktop/src/lib/tauri-commands/about.ts`), which mints the request
id. Each `AboutEvent` folds in through `applyAboutEvent`:

- `planned` sets the rows (once). Facts before it, or for a kind it doesn't list, are dropped.
- `fact` sets one row and replaces `derived` (the headline and removal row as the backend recomputed them).
- `volumeStoppedAnswering` turns the plan's volume note into its "stopped answering" words, in the slot the note already
  had (a card reached through its volume always plans one), so nothing moves. The rows that were waiting land right
  after as Routine one-liners ("No answer from the drive").
- `factsSettled` and `unavailable` end the stream; nothing after them lands.
- An event whose `requestId` isn't the card's is dropped, so a reopen on another item never shows the first item's
  facts. `openAboutThis` closes (and cancels) any card already up before it starts the next.

**Decision: the dialog appears after the grace, not on the keypress.** `aboutThisState.shown` turns on once the plan is
in and `GRACE_MS` (100 ms, the context menu's grace) has passed, or the facts settled, or the card is unavailable, with
`SHOW_ANYWAY_MS` (1.5 s) as the cap. Why: drawing on the keypress would open an empty frame and then grow it when the
plan lands, and most rows answer inside the grace, so the first frame is already the card at its size.

The host is `routes/(main)/+layout.svelte` (beside the feedback dialog, inside its own `DialogBoundary`); the handler is
`file.tellMeAbout` in `routes/(main)/command-handlers/file-handlers.ts`, about the cursor entry, as Get info is.

## Layout

Top to bottom: the title (the item's icon and "About “name”", cut with a tooltip), the headline, the volume note, the
evidence sections in the plan's order, the verdict slot, What removing it means, and the footer (Close; the actions join
it later). `ModalDialog` with `resizable` and `growDownward`, 34 rem wide, at most 85 vh, the body scrolling.

- **The title icon** is the pane's own: the subject carries the entry's `iconId`, and the title reads the icon cache the
  pane already filled (a folder's custom icon first, by path). ❌ Never a fetch from the card; a miss is the glyph by
  target.
- **A floored importance** reads "Not ranked" with Cmdr's reason as Cmdr's choice ("Cmdr doesn't rank hidden, system, or
  temporary folders"), ❌ never "Low": on a folder of app data that would read as "this doesn't matter".
- **What's inside** (`AboutDigest`, a folder's first section) draws its own `<dl>` rows: Contents (the count and kind
  mix, plus the folder's class when its name gives one), Biggest inside, Largest files (anywhere below, each with where
  it sits), and Untouched (the share unchanged for a year). An empty folder is the one "Empty" line. A Largest files
  that's `NotIndexed` reads "Cmdr lists these once Search has loaded this drive": the search arena isn't in memory, and
  the card never loads it. Untouched shows only beside answered Largest files, which come from the same search.
- **An app's Library data** (`AboutAppData`): the total ("at least" when the crate says so), each folder with its
  Library place first (every folder carries the same id, so the place is what tells them apart), and the places the
  index hasn't listed. The removal row's total is the crate's `totalBytes`, ❌ never a sum the card does.
- **Places go to the pane**: each `AboutPlaceList` row is a button; Enter or a click calls `onGoTo`, which
  `goToFromCard` turns into close-then-navigate (a folder opens, a file shows in its folder with the cursor on it, via
  `file.tellMeAbout`'s handler). The name keeps its room and the muted "in …" gives way first.
- **A collapsed section names a shared reason**: when every row is the same unknown other than `noRecord`, the line is
  that reason's words ("Not indexed yet"), else "Nothing on record" (`SectionView.collapsedWhy`).
- **An archive's entry**: the download row's first line names the archive ("Inside the archive “Photos.zip”."), then
  either the archive's own record under "What macOS recorded about the archive:" or the usual absence. The headline says
  "inside Photos.zip", never the archive's site.
- **A file in git's history** (`gitSnapshot`, Origin's first row): which branch, tag, commit (seven characters, the file
  list's `SHORT_ID_LENGTH`, the full id behind Copy), or stash entry, the repo by its folder's name, and when a commit
  last changed it.
- **Rows are `<dl>` groups**: a label column (8.5 rem) and the answer. Identity draws its own lines (Size for a file,
  Modified, Created, Other names past one hard link, Points to for a link); a folder's size is the `recursiveSize` row,
  never its own entry size. The size on disk joins the size only when it differs by at least 1 MB and 10%
  (`showsOnDisk`): every small file takes a few KB more for block rounding, which isn't news.
- **No movement**: the headline, the removal list, and rows whose usual answer takes two lines (download, Cmdr's
  history, signature, owner, git, importance) reserve two lines until `factsSettled`. What's inside reserves its whole
  section (`CONTENTS_RESERVED_LINES`, 19: the summary, eight biggest, eight largest, the untouched share), pending or
  landed, because its searches scan the whole arena and land after the first frame on a big drive (host DETAILS § A
  folder's digest). At the settle the reservations go and an all-routine section collapses to its heading plus "Nothing
  on record", in the same frame. **Decision**: the headline's second line goes too, since a one-line headline over a
  blank line read as a gap on every plain card. Pending rows are a static bar (no shimmer at all, so
  `prefers-reduced-motion` has nothing to stop).
- **Prominence is the wire's**: `AboutAbsence` draws a Routine unknown (or a not-applicable) as one muted line and a
  Notable one as a sentence with a quiet mark. A row-specific absence word (`ROW_UNKNOWN`) wins over the generic one.
- **The verdict slot** is an empty `[data-verdict-slot]` div right above the removal section, `display: none` while
  empty, so it takes no height until a verdict block fills it.
- **Focus**: the card's body (`.card`, `tabindex=0`) takes focus on mount so the arrow keys and Page Up/Down scroll it;
  Tab then reaches each copy button and the footer. `ModalDialog` restores focus to the pane on close.
- **a11y**: pending rows carry `aria-busy`; one polite live region reads the headline's text once, at `factsSettled`; ❌
  no live region per fact.

## Item strings

Every string the item reported arrives through `model_text` (`ItemText`) and renders through `t()` params or `{…}`: a
text node. A download source shows its host only (punycode stays punycode, as the wire has it); the whole address is
behind a Copy button (`WhereFrom.url`), and opening it is a later action. Mail's sender and subject, an AirDrop name,
the downloading app, a signer, a branch, and paths render the same way. Paths go through `AboutPath` (ellipsis plus a
tooltip with the full text); the row puts its Copy button at the end of the line (`.with-copy`), never mid-sentence.

## Tests

- `about-this-state.svelte.test.ts`: the reducer (out-of-order facts, a fact outside the plan, before the plan, after
  the settle, unavailable), the live slice (cancel on close, a late event after close, a reopen elsewhere never showing
  the first item's facts), and the grace.
- `about-card-view.test.ts`: section order, the collapse only at the settle and never over a Notable reason, the shared
  reason of a collapsed section, the calm-card budget (the gallery's plain document has more known rows than unknown
  ones), and that no fixture sets a fallback removal line beside a real one.
- `AboutDigest.svelte.test.ts`: What's inside on a big folder, `node_modules`, an empty and an unlisted folder, an app's
  Library data, and that each place row hands `onGoTo` the right target.
- `AboutThisDialog.a11y.test.ts`: axe over the gallery's cards and each part, the `<dl>`, `aria-busy`, body focus, and
  the single announcement.
- `apps/desktop/test/e2e-playwright/about-this.spec.ts`: shortcut, palette, and menu, a download record the spec writes
  (host shown, full address not), the type row, a folder's size row, Escape giving focus back to the pane.
