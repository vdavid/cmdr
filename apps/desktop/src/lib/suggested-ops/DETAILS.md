# AI suggestions details

Pull-tier docs for `lib/suggested-ops/`. Must-knows live in `CLAUDE.md`. The backend service:
`apps/desktop/src-tauri/src/agent/suggested_ops/DETAILS.md`.

## Why the disclosure is shaped this way

We do not trust the agent. A suggestion can be formally valid and factually hallucinated, and nothing in the app can
tell which, so the dialog's job is not to vet the suggestion; it is to lay it out for the user to decide. Everything
here follows from that.

**The AI's words and Cmdr's facts sit side by side, and only the AI's are labelled.** `AgentReason` leads the rationale
with a bold "Why:" on the same line (David's call, 2026-10-26): short to the eye, with a tooltip saying these are the
AI's own words, and a screen reader hears "The AI's reason:" in its place, so the source is named on every channel.
Cmdr's facts go unlabelled: the source and the full destination in a `From` / `To` list right under the header, each
volume as a soft pill with the volume picker's own icon (`VolumePill`, `getIconForVolume`), and the live Size and Last
modified columns. The juxtaposition is the mechanism: a user checks a claim against something the agent could not
invent. An unlabelled rationale would read as a finding, which is exactly the failure this surface exists to prevent.

**Top to bottom is the order a decision is made in** (`SuggestedOpsGroup.svelte`): title, file count, and the pills that
change the answer on one header row (wrapping inside a pill when a reason runs long); the facts; the AI's reason; the
file list; then Reject and the action button. A sweep opens with a heading for the groups under it, in words
(`sweepWhen`: "Suggested 15 minutes ago", "Suggested yesterday at 14:04", the user's date format for anything older,
with the exact moment in a tooltip), then the sweep's own reason. A lone sweep from the last ten minutes gets no heading
(`sweepHeadingHidden`): the user just asked for it, and the line would say nothing.

**The approval note is a warning the user can close for good.** "The AI suggested these. Nothing runs until you approve
it." opens the dialog in the house warning look, and its × sets `suggestedOps.suppressApprovalNotice`, the same
suppress-switch pattern as the other hints in Settings > Advanced > Hints and warnings, which is where it comes back.
Groups are separated by a hairline, not boxed: seven stacked bordered cards read as busy, and the file list is already
the one framed well in each group.

**The button names the action** (`approveKey`): "Move 4 files", "Move 5 files to Trash", "Delete 3 files", "Extract 2
archives", and the count is what would actually run (`approvableCount`, from the selection). It never changes wording
with the list's state. An irreversible group's button is the `danger` variant, so the one press that can't be taken back
looks like it.

**The undo pill says why** (`reversibilityKey`): "You can undo this: they go to the Trash", "…: Cmdr can move them
back", "…: Cmdr deletes the copies". The pills sit beside the count as real capsules (`--radius-full`, a hairline,
`--font-size-sm`), so they line up with the header text.

**Reversibility is disclosed, never enforced.** The pill renders whatever `Reversibility` says, including a permanent
delete. Refusing the group there would be an agent-specific safety behaviour on the execution path, which the guiding
principle rules out: an approved op is a user-started op.

**A group the engine would refuse outright says so before the click** (`runBlocker`, `runBlockerKeys`). A trash group on
a drive with no Trash can't run at all: the OS refuses every item and NOTHING is deleted, so it isn't "irreversible",
it's a no-op. Its pill reads "Can't run: this drive has no Trash" in place of the undo pill, and its action button is
`ariaDisabled` with the reason as its tooltip: still focusable so the reason reaches the keyboard, and pressing it runs
nothing. This isn't an agent-specific refusal: the engine refuses the same move when a user starts it; the dialog only
declines to offer a button that can only fail.

## The file list

Every group shows its files right away (`SuggestedOpFiles.svelte`): the first `INITIAL_ROWS` (10), then "Show 20 more"
and "Show all N" on the row under the list, opposite the group's Reject and action buttons (the group hands them in as
the `actions` snippet, so a short group spends one row on its footer, not two). The list is a `ColumnList` with `table`
semantics (its rows hold checkboxes) over a windowed source whose `count` is the rows shown. The frame is sized to
exactly the shown rows up to `MAX_VISIBLE_ROWS` (20) and scrolls past that, so "Show all" on 60,000 rows is a 20-row
viewport drawing a window, never 60,000 DOM rows.

**Rows read like file rows**: the icon from the shared icon cache (keyed `ext:<ext>` like the backend's
`derive_icon_id`), the name mid-truncated to keep the extension, then the facts. When every op shares one folder
(`sourceFolder` on the group), the folder is said once in the facts and the rows show names only; when they don't, the
facts say "Several folders" and a Folder column appears. A new name follows the old one after an arrow, for ANY verb
whose op carries one (a move or copy that renames on the way, not only a rename), and a screen reader hears "New name"
for the arrow.

**Sizes read exactly as the file panes show them**: the size column's own `formatSizeForDisplay` with the pane's options
(`listing.sizeUnit` and the 1000/1024 base), not `<Size>`, which forces the dynamic unit. A user who picked "MB" for
their panes sees MB here too.

**Size and Last modified are what the disk says now** (`rowFacts` in `suggested-ops-copy.ts`):

- **Live, unchanged**: the value, plain.
- **Live, moved since the suggestion**: a warning `StatusGlyph` beside the value, its tooltip "When suggested: X / Now:
  Y". A suggestion that recorded nothing has nothing to have changed FROM, so it never flags.
- **Gone**: "Moved or deleted" in the warning color. Approving skips it; the engine re-checks every source.
- **Couldn't check (or not checked yet)**: the creation snapshot in tertiary text behind a clock `StatusGlyph` labelled
  "From when the AI suggested this". No value at all says "Unknown". ❌ Never a zero: it reads as an empty file.

**Decision: the snapshot is the fallback, not the column.** The columns used to show only the creation snapshot, headed
"Size when suggested", which was honest but answered the wrong question: the user is deciding about the file as it is
NOW. The snapshot stays as the comparison point and the fallback, always labelled when it's what's on screen.

## Scale: windows, live facts, and the wire

A group of 60,000 ops is legitimate, so nothing may grow with it.

**The windows.** Each group has its own `GroupReview` in `suggestedOpsState.reviews`. `COUNT(*)` gives the total and
only the rows the list draws are fetched (`OP_WINDOW_SIZE`, centred on the request so scrolling either way stays
inside). `ColumnList` reports the drawn range through `onRangeChange` → `showRange`, which pages, then checks. A page is
fetched when ANY row of the range is missing (`windowHolds(start, end)`), ❌ never on `start` alone: that left the
bottom of the list blank whenever the range ran past the held page's end (regression test "holds every drawn row…"). One
page is in flight per group; a scroll made meanwhile is turned away, so a page that lands follows the range if it moved
(the list won't report the same range twice). A row whose window isn't loaded renders `ColumnList`'s placeholder and
`opAt` answers `null` rather than a neighbouring row. Opening the dialog reads every group's first page side by side
(`Promise.all`), so seven groups cost one round trip of latency, not seven.

**Live facts** are read for the drawn rows only, once per op (`ensureLiveFacts` →
`liveSuggestedOpFacts(groupId, offset, limit)`, one call per contiguous span of rows not yet asked about): one bounded
stat per row the user actually sees, through the backend's shared stat budget. A page that lands after the list drew its
rows still gets them (the review remembers its last `range`). The answer is typed:

- **A source that can't be asked at all** (`sourceNotConnected`, `sourceGone`) sets the review's `sourceUnreachable`:
  the group stops asking, and one line above the list says why its sizes and dates are from suggestion time, rather than
  a clock glyph on every row standing in for an explanation. "Show the new version" clears it and asks again.
- **Anything else** (a store refusal, an unknown group, a throw) un-marks the span, so the next scroll asks again, and
  is logged rather than surfaced: the labelled snapshot is an honest answer while the live one is missing.

**The wire, and the selection.** A group's selection (`OpSelection`) records only what the user flipped by hand, over
one of three bases:

- **`fromStored`** (a review opens here): the stored ticks. An op an earlier review turned off (its approval then
  refused to start) arrives `excluded` and opens unticked, so a reopened review keeps the user's "no" without loading
  every op to find it. A row's box flips into `exclude` (a pending op turned off) or `include` (an excluded op turned
  back on); the count is `liveOpCount - exclude + include`.
- **`allExcept`** (after the header's select all): every op row but `ids`, counted down from `totalOpCount`.
- **`only`** (after the header's select none): just `ids`.

Approving sends the base and the flipped ids, so the wire stays small in every direction at any group size: "all but
three", "only these three", and "what I left last time, plus one" of 60,000 are each a few ids. The header reads mixed
whenever the count sits between none and all; a group reopened with earlier "no"s starts mixed.

**Decision: select all / none exists, as flips over a base** (David, 2026-10-26). It replaced a deliberate "no deselect
all", whose reason was the wire: a one-mode "ids turned OFF" set would have had to enumerate 60,000 ids to say "none".
The `only` base removes that cost, so the affordance is cheap.

## A group that changes under an open review

The agent may re-propose a pending group while the user reads it. Two signals raise the group's `changedUnderReview`,
and the rows stay exactly where they are; the notice offers "Show the new version" (`reloadGroup`).

- **The event**: `suggestions-changed` with `reason: 'amended'` and that group's id.
- **The revision**: a re-read that finds the group's `revision` past the review's `seenRevision` (the revision whose
  rows are on screen). The counts are no signal: the user's own refused-to-start approval moves `liveOpCount` without
  changing the group. It catches a change while the subscription is down. Both stay.

❌ Never swap the rows silently: the user is midway through a decision keyed to row positions. Approve waits for the
reload (`canApprove`), and sends `seenRevision` with the selection, so even a race between a click and an amendment
refuses as `listChanged` rather than running rows nobody read. `reloadGroup` moves `seenRevision` to the new revision as
it loads the new rows.

**Decision: the notice keys on `reason`, never on `groupId` alone.** An amendment and the user's own approval carry the
SAME id. Keying on the id would raise "The AI changed this" the instant the user approved something. A test pins it.

**Decision: a reload keeps the selection.** An op the user turned off stays off if the new version still has it.
Clearing the set would quietly turn their "no" into a "yes" on the next approve, which is the unsafe direction. A stale
id (an op the new version dropped) costs nothing: the backend ignores ids outside the group's live set.

**A re-read never blanks the dialog.** Only the first read (no sweeps yet) shows the loading line; a refresh after an
event or a decision updates the groups in place, so no list loses its scroll position.

## The badge, and why it is a separate module

`suggested-ops-badge.svelte.ts` is mounted for the whole session; the dialog's state only exists while a review is open.
Folding them would leave the corner reading a store nothing populates until the dialog opens, so the indicator would sit
at zero forever.

It does two things that look redundant and are not:

- **Seeds once at startup**, because suggestions never expire. A group proposed last week is waiting before this session
  emits anything.
- **Subscribes**, because after that first read only being told moves it. ❌ Never poll.

A count it cannot read is logged and dropped rather than propagated: an approval that already succeeded must not fail
because a badge could not refresh. Neither half throws: window services start the badge with `void`, where a rejection
escapes as an unhandled one and auto-sends an error report.

## When a read or an answer doesn't happen

Every command wrapper throws a `SuggestedOpsFailure` (`suggested-ops-failure.ts`, a `TypedFailure`), ❌ never
`throwIpcError`. `SuggestedOpsError` is tagged by `type` and carries its cause in `detail`, with no `message`, so the
generic path kept only the variant name; the failure's message carries both.

- **A page read that throws sets that group's `windowError`.** The list says the files couldn't load and offers Try
  again, which calls `ensureOpWindow` for the same rows. The list only asks for rows when its range moves, so without it
  the rows would sit on their placeholder for good.
- **An Approve or Reject that throws re-reads FIRST, then sets `decisionNotice`.** The store is the only thing that
  knows whether the decision landed. The notice sits above the list, not in the group, because after the re-read the
  group may be gone.
- **The notice keys on the typed variant.** `approvalDidntFinish` means the approval worker died, so whether the claim
  committed is unknown: "check the operation queue before you approve it again". Every other variant is a refusal that
  lands before the claim transaction commits, so "try again" is safe
  (`agent/suggested_ops/bridge/mod.rs::approve_and_execute`).
- **An Approve the engine refused to START gives the group back.** `refused`, `nothingToRun`, and `couldNotStart` all
  mean nothing ran and the group is `pending` again (`apps/desktop/src-tauri/src/agent/suggested_ops/DETAILS.md` § "The
  approval bridge"). The reason shows under the group's buttons and sits in `refusals` by group id until that group is
  answered again or the dialog next opens. `refused` carries the `WriteOperationError` itself, so
  `suggested-ops-refusal.ts` words it through `getUserFriendlyMessage` with the verb mapped to the transfer wording.
  Only the two suggestion-only refusals carry `suggestedOps.refusal.*` copy.
- **Closing leaves the review state in place; the next open drops it** (`forgetLastReview`). Clearing `reviews` in the
  tick that unmounts the dialog pulls each row's op out from under its cells mid-teardown (Ark's checkbox reads
  `checked` on the way out), and the throw takes the main window down to its crash panel. Regression anchor:
  `SuggestedOpsDialog.close.test.ts`; the E2E dialog-inset sweep first caught it as a wedged app.
- **A re-read drops a gone group's review only after the flush that unmounts its rows** (`refreshSuggestions` awaits
  `tick()` first), for the same teardown read: approving a group used to crash the window the moment its move started.
  Regression anchor: `SuggestedOpsDialog.approve.test.ts`; E2E caught it as every spec after `suggested-ops-named-move`
  failing on the crash panel.
- **Decision: Approve stays enabled while a group's rows are loading or couldn't load** (David, 2026-09-25, #155). Its
  count comes from `COUNT(*)`, so the button says exactly how many ops it approves even before any row shows. ❌ Don't
  gate Approve on the op window.

## The gallery preview

Debug > Soft dialogs opens the dialog store-seeded (`dialog-gallery/fixtures/store-seeded.ts`, `suggestedOpsFixtures`):
the patch sets `open`, `sweeps`, and pre-filled `reviews` (windows and live facts), so every row state shows without a
backend. The invoices move files three of its five under new names, so a move's `old → new` rows show beside ones that
keep their name. "Server cards" shows an SFTP add at a new address and a NAS host's edit with its shares line; "Server
cards given back" shows the same two refused. Seeded live facts are never re-read (`ensureLiveFacts` skips ops it
already holds). Rows past a seeded window (the 60,000-file "Raw camera files") page through the patch's `rowSource`
(`suggestedOpsState.rowSource`, `null` in the app), which answers `page` and `liveFacts` from the fixture: the real
store has no fixture group and answers a total of 0, which emptied the list on the first scroll past row 200 (regression
test in `dialog-gallery/fixtures.test.ts`). Approve and Reject reach the real store, which has no such group, so the
dialog re-reads and shows what's really waiting; the row says so.

## A server card

A group carrying `server` (`addServer` / `editServer`, zero ops) is the same header → facts → reason → actions, with no
file list. The backend: `apps/desktop/src-tauri/src/agent/suggested_ops/DETAILS.md` § Approving a server.

- **The header** keeps the server's name as the title, and where a file group shows its count it says "Add a server" or
  "Change server settings". The reversibility pill reads from `serverSettings` (`reversibilityKey`).
- **The facts are what the click saves** (`SuggestedServerFacts.svelte`, rows inside the group's own `<dl>`, so they
  read like From / To): Type (the hub's protocol label), then each `fields` row under the servers sheet's own label
  where it has one. An edit's changed rows show `old → new` (a screen reader hears "changes to"), unchanged rows plain,
  and an absent value says "Not set". `addressIsNew` puts an "A new address" pill beside the address: a fact Cmdr read
  now, beside an AI that may call it "your NAS". An SMB edit says under the facts how many saved shares on that host it
  changes (`sharesAffected`, counted when the card was read). Unlabelled, like every Cmdr fact; only the rationale goes
  through `AgentReason`.
- **❌ No input of any kind.** The card is read-only by design, so a password field can't appear, and none of the
  draft's fields could hold one (`agent/store/proposals/DETAILS.md`).
- **Nothing pages.** The review exists for `seenRevision` and the change notice only; `ensureOpWindow` returns at once
  for a server card, so no window, no live facts, no selection. `canApprove` is true while it hasn't changed under the
  review, so Approve stays enabled (there's nothing to load).
- **The button** is "Add server" or "Save changes" (`approveKey`), and `approveGroup` routes it to
  `approveSuggestedServer` with the revision the user read.
- **A refusal gives the card back** under "Why this wasn't saved". It sits in `refusals` as `{ kind: 'server', answer }`
  (its `refused` carries a `SavedServerOutcome`, not a write error, so it can't share the file kinds). A refused save
  and an SMB add's `AddServerError` read through the servers sheet (`readSavedServerOutcome`, `wordConnectRefusal`), one
  sentence carrying its own fix; `serverChanged`, `alreadySaved`, and `nameTaken` have their own two-line copy. The
  message is escaped whole: it renders through the same `{@html}` boundary as a write error. `listChanged` raises the
  change notice, as for a file group.
- **No "partial" note yet**: the backend's `saved` answer carries no note (`ServerSaveNote` has no producer), so the
  card has nothing to show after a save; the group leaves the list.

## What isn't here yet

- **`interrupted` groups**: re-approving one mints a NEW group with a fresh preflight, which is spine machinery rather
  than dialog work. Deliberate, not missing.
- **The degraded-state actions**: `WakeReadiness` types Ask Cmdr off / AI off / cloud AI not allowed / Full Disk Access
  / no-API-key as distinct states, each with its own answer. The indicator renders a count today and should render the
  closable ones with their specific action.
