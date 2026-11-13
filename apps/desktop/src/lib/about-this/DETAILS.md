# Tell me about this (card) details

The card for `file.tellMeAbout` (`⌘⇧I` on macOS, `Ctrl+Shift+Enter` elsewhere; the palette; File > Tell me about this;
the right-click row below Get info). Every string is a draft for David: the list with keys is
`docs/specs/tell-me-about-this-copy.md`.

## The stream into the card

`openAboutThis(subject)` calls `startAbout` (`apps/desktop/src/lib/tauri-commands/about.ts`), which mints the request
id. Each `AboutEvent` folds in through `applyAboutEvent`:

- `planned` sets the rows (once). Facts before it, or for a kind it doesn't list, are dropped.
- `fact` sets one row and replaces `derived` (the headline and removal row as the backend recomputed them).
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

Top to bottom: the title (a glyph by target and "About “name”", cut with a tooltip), the headline, the volume note, the
evidence sections in the plan's order, the verdict slot, What removing it means, and the footer (Close; the actions join
it later). `ModalDialog` with `resizable` and `growDownward`, 34 rem wide, at most 85 vh, the body scrolling.

- **Rows are `<dl>` groups**: a label column (8.5 rem) and the answer. Identity draws its own lines (Size for a file,
  Modified, Created, Other names past one hard link, Points to for a link); a folder's size is the `recursiveSize` row,
  never its own entry size. The size on disk joins the size only when it differs by at least 1 MB and 10%
  (`showsOnDisk`): every small file takes a few KB more for block rounding, which isn't news.
- **No movement**: the headline, the removal list, and rows whose usual answer takes two lines (download, Cmdr's
  history, signature, owner, git, importance) reserve two lines until `factsSettled`. At the settle the reservations go
  and an all-routine section collapses to its heading plus "Nothing on record", in the same frame. **Decision**: the
  headline's second line goes too, since a one-line headline over a blank line read as a gap on every plain card.
  Pending rows are a static bar (no shimmer at all, so `prefers-reduced-motion` has nothing to stop).
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
- `about-card-view.test.ts`: section order, the collapse only at the settle and never over a Notable reason, and the
  calm-card budget: the gallery's plain document has more known rows than unknown ones.
- `AboutThisDialog.a11y.test.ts`: axe over the gallery's cards and each part, the `<dl>`, `aria-busy`, body focus, and
  the single announcement.
- `apps/desktop/test/e2e-playwright/about-this.spec.ts`: shortcut, palette, and menu, a download record the spec writes
  (host shown, full address not), the type row, a folder's size row, Escape giving focus back to the pane.
