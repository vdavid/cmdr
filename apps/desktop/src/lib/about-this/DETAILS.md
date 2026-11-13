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
- `factsSettled` ends the facts and `unavailable` the whole card; no fact lands after either. The verdict's events
  (`about-verdict.ts`) need the plan and may run past the settle.
- An event whose `requestId` isn't the card's is dropped, so a reopen on another item never shows the first item's
  facts. `openAboutThis` closes (and cancels) any card already up before it starts the next.

**Decision: the dialog appears after the grace, not on the keypress.** `aboutThisState.shown` turns on once the plan is
and the verdict's answer (`verdictPlanned` or `verdictUnavailable`, which the backend sends right after the plan) are in
and `GRACE_MS` (100 ms, the context menu's grace) has passed, or the facts settled, or the card is unavailable, with
`SHOW_ANYWAY_MS` (1.5 s) as the cap. Why: drawing on the keypress would open an empty frame and then grow it when the
plan lands, and most rows answer inside the grace, so the first frame is already the card at its size.

The host is `routes/(main)/+layout.svelte` (beside the feedback dialog, inside its own `DialogBoundary`); the handler is
`file.tellMeAbout` in `routes/(main)/command-handlers/file-handlers.ts`, about the cursor entry, as Get info is.

## Layout

Top to bottom: the title (the item's icon and "About “name”", cut with a tooltip), the headline, the volume note, the
evidence sections in the plan's order, the verdict slot, What removing it means, and the footer (the actions on the
left, Close on the right). `ModalDialog` with `resizable` and `growDownward`, 40 rem wide, at most 85 vh, the body
scrolling.

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
- **The AI take's slot** (`[data-verdict-slot]`) sits right above the removal section and exists only when the take
  draws something (§ The AI take), so a card without one is exactly the card without AI.
- **Focus**: the card's body (`.card`, `tabindex=0`) takes focus on mount so the arrow keys and Page Up/Down scroll it;
  Tab then reaches each copy button and the footer. `ModalDialog` restores focus to the pane on close.
- **a11y**: pending rows carry `aria-busy`; one polite live region reads the headline's text once, at `factsSettled`; ❌
  no live region per fact.

## The AI take

`AboutVerdict.svelte`, fed by `about-verdict.ts` from the backend's verdict events (host DETAILS § The verdict, plan D19
for the placement).

- **Its own block, never a row's look**: a quiet surface and border, the sparkles glyph, and the heading "AI wrote
  this", so a model's sentence can't pass for one of Cmdr's facts. It's a labeled `<section>` (a region), outside every
  `<dl>`.
- **The text is one text node** under `white-space: pre-wrap`, ❌ never `renderAssistantMarkdown` or `{@html}`: plain
  text is the one rendering a model can't turn into a link, an image, or a heading that imitates a row. The template
  keeps that paragraph on one line, since `pre-wrap` would show any whitespace between its tags.
- **Phases** (`verdictPhase`): waiting ("Writing a short take…", and a Stop button), streaming (the text, Stop), done,
  stopped (the text so far plus a quiet "Stopped"), from earlier (a tag beside the heading), failed (the provider's
  answer as a quiet line after any text: `noAnswer`, `unreachable`, …).
- **The given-line** ("From the facts above and the first page") has its own reserved line under the heading and fills
  at `verdictGiven`, before any text, so the person sees what's leaving while it leaves.
- **No movement**: the heading row holds the Stop button's height after it goes, the given-line one line, and the text
  three lines, from the first frame; the text may grow past three while it streams, downward only, and that's the one
  growth the card allows. End marks sit inline after the text. Nothing animates (no caret, no fade).
- **Stop** calls `stopAboutVerdict` (`about_stop_verdict`): the take alone stops, the card stays, and the backend's
  `stopped` keeps the text. Esc closes the card, which stops both. **Decision: no Stop shortcut**: it's a Tab stop in
  the card, and a chord would be a new command and scope (a review item).
- **Unavailable**: `unavailableShowsALine` says which reasons draw one quiet line in the slot (cloud AI needs allowing,
  no key, the local server not running, the organization's policy through `managedAiRefusalMessage`, too many in a
  minute) and which draw nothing (`aiOff`, `turnedOff`, `localOff`). **Decision: `localOff` draws nothing**: it's every
  Local user's default until the eval says the local model's takes are good (plan D4), and a line on every card would be
  noise. AI off gets one quiet hint in the footer, under the actions ("AI is off…" plus a link to Settings > AI), never
  in the body.
- ❌ **No one-click allow**: the consent hint's link opens Settings > AI at the Allow cloud AI switch, where the whole
  disclosure is read (`openCloudConsentSettings('about-this-card')`). A card that's open when consent arrives doesn't
  start a call by itself (nothing re-asks); the next open does.
- **a11y**: no live region over the stream; a second polite region beside the headline's says "AI wrote this: …" once,
  when the take ends with text (or arrives whole from earlier).
- **The cost** rides `verdictEnd` into the card's data and isn't shown (a review item).

## The footer's actions

`AboutActions` draws `derived.actions` in `ModalDialog`'s `footerLeading`, with Close in `footer`: the shown ones as
buttons, the rest in a house `Menu` under "More". Which actions, their order, and the split are Rust's
(`crates/cmdr-about/DETAILS.md` § Next actions); the words are `about-actions.ts`.

- **Every action leaves the card**, so a press calls `actFromCard`: it closes the card (cancelling the gathering), then
  hands the action to the opener's `act` hook (`apps/desktop/src/routes/(main)/command-handlers/about-this-opener.ts`).
  Focus lands where the action went: the pane, the delete dialog, Finder, the browser.
- **What each does, through the pane's own path**: Open is `openItemUnderCursor` (Enter's chain, so a downloaded app
  gets whatever the pane would ask), only while the cursor is still on the item; Show in Finder is `showInFinder`; the
  download page is `openExternalUrl` with the address Rust re-parsed; the owning app and the app's data folder are a
  `navigate()` (resolved to their volume first: an app is on the boot disk); Move to trash and Delete are
  `openDeleteDialog({ cursorPath })`, which reads the cursor row alone (❌ never the selection) and opens nothing when
  that row isn't the item any more. Continue in Ask Cmdr is `openNewChatWith([{ path, kind }], draft)`: a new chat with
  the item attached and the FIXED draft "Tell me more about this.", unsent (`../ask-cmdr/DETAILS.md` § Attachments by
  reference). ❌ The draft never quotes a fact or the AI take; the agent reads the facts through `about_item`.
- **Continue in Ask Cmdr shows only while Ask Cmdr is on** (plan D5): the host reads `askCmdr.enabled` fresh per card
  (`settings::load_ask_cmdr_switch`, beside the verdict's gate) into `ActionCaps::ask_cmdr`, and Rust places it after
  the facts' own next steps, before removal. It doesn't depend on the AI take: Local, AI off, and a refused take give
  the same footer.
- **Keyboard**: Tab from the body reaches each action, then "More", then Close. ❌ No action takes focus on open, and
  removal is never the first button (Rust). Single-key accelerators are a review item, not built.
- **Decision: the 40 rem default width** fits three labels, "More", and Close on one line. **Decision: the labels are
  short and fixed** ("Open", "Download page"), with the app and the site in the tooltip (hover and keyboard focus) and
  in the rows above ("Opens with", "Downloaded"): "Open with DiskImageMounter" and "Open releases.example.com" cut to
  "Open with DiskImage…" in the first screenshots, and a label whose host is cut hides the very thing it's there to
  show. A narrower card (resized) drops the actions to a row of their own above Close: `ModalDialog`'s own footer rule,
  ❌ never a squeezed or ellipsized label.
- **The footer can grow as facts land** (Open appears when the default app answers, the download page with the record):
  on a local disk both answer inside the grace, so the first frame already has them. ❌ Nothing in the body moves for
  it.

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
- `AboutActions.svelte.test.ts`: the footer over the gallery's cards (what shows, what waits behind "More", removal
  never first, no action focused on open, a press reaching the opener). `about-this-opener.test.ts` (beside the opener):
  what each action does once the card closed.
- `AboutDigest.svelte.test.ts`: What's inside on a big folder, `node_modules`, an empty and an unlisted folder, an app's
  Library data, and that each place row hands `onGoTo` the right target.
- `AboutThisDialog.a11y.test.ts`: axe over the gallery's cards and each part, the `<dl>`, `aria-busy`, body focus, and
  the single announcement.
- `about-verdict.test.ts`: the verdict's reducer (each phase, the given-line before any text, what's dropped out of
  order) and which unavailable reasons draw a line. `AboutVerdict.svelte.test.ts`: a hostile take renders literally
  (`<img onerror>`, a Markdown link), the label and the given-line, Stop, every unavailable reason, the consent hint
  opening Settings with no allow, one announcement at the end, and the same rows, removal, actions, and focus with a
  reassuring take, a refused one, and none.
- `apps/desktop/test/e2e-playwright/about-this.spec.ts`: shortcut, palette, and menu, a download record the spec writes
  (host shown, full address not), the type row, a folder's size row, Escape giving focus back to the pane.
