# Tell me about this (card)

The "Tell me about this" fact card (#1104): a resizable dialog that streams what Cmdr knows about the item under the
cursor. The backend (`apps/desktop/src-tauri/src/about/CLAUDE.md`) gathers; `crates/cmdr-about` owns every rule; this
directory only renders.

## Module map

- `about-card.ts`: the card's data and `applyAboutEvent`, the one reducer (pure, shared with the gallery).
  `about-verdict.ts`: the AI take's data and its part of the reducer; `AboutVerdict.svelte` draws it.
- `about-this-state.svelte.ts`: the open card (`openAboutThis` / `closeAboutThis`) and its first-frame grace.
  `about-analytics.ts`: the `tell_me_about_used` event, fired once at each card's close, tokens only.
- `AboutThisDialog.svelte` (the frame), `AboutHeadline`, `AboutRow` (one planned row), `AboutAbsence` (an unknown's
  line), `AboutPath`, `AboutRemoval`. `AboutDigest` (What's inside), `AboutAppData` (an app's Library data), and
  `AboutPlaceList` (their rows, each taking the pane there). `AboutActions` + `about-actions.ts`: the footer's actions
  and their words. `about-card-view.ts`: sections, the collapse. `about-words.ts`: enum → key.

## Must-knows

- ❌ **No rule here.** Which rows exist, prominence, the headline, and the removal row arrive typed from Rust; the card
  words them. A "should we show this" question goes to `cmdr-about`.
- ❗ **Drop what isn't this card's.** `applyAboutEvent` ignores another request's events, a fact outside the plan, and
  anything after `factsSettled`, so a quick close-and-reopen never shows the first item's facts.
- ❗ **Close cancels through `about_cancel`**, ❌ never by dropping the channel (`send` succeeds after the handler is
  gone).
- ❗ **Nothing moves once drawn**: rows reserve their usual height until `factsSettled`, and the one layout change
  (releasing those and collapsing all-routine sections) happens there. Nothing animates.
- ❌ **Item strings are text nodes**: never `{@html}`, never a link built from them. Self-reported facts read "recorded"
  / "carries a signature from"; only a matched executable reads "signed by", and nothing reads "verified".
- ❌ **No removal line says it's safe**: `AboutRemoval` words exactly the variant it gets.
- ❌ **The AI take is untrusted text**: one text node (`pre-wrap`), ❌ never Markdown, `{@html}`, or a link, and nothing
  else reads it: rows, removal, actions, and focus are the same with and without it (`AboutVerdict.svelte.test.ts`). Its
  block exists only after `verdictPlanned`, reserves its lines, and grows downward only. ❌ No live region over the
  stream; one announcement at its end.
- ❌ **The card never grants cloud consent**: the `noCloudConsent` hint opens Settings at the disclosure.
- ❗ **A row or footer action goes through the opener's hooks** (`openAboutThis(subject, { goTo, act })`, built in
  `apps/desktop/src/routes/(main)/command-handlers/about-this-opener.ts`): the card closes FIRST, then the pane
  navigates, the delete dialog opens, or Finder shows it. ❌ The card never navigates, opens, or deletes by itself.
- ❌ **The footer is Rust's** (`derived.actions`): which actions, their order, and what's behind "More". Nothing in it
  takes focus on open, and Move to trash goes through the pane's delete dialog for the cursor row alone (`cursorPath`).
- The gallery renders the shipping dialog from recorded event streams (`lib/dialog-gallery/fixtures/about-this.ts`).

The stream, layout and AI-take decisions, the footer, and the test map: `DETAILS.md`. Read it before any non-trivial
work here: editing, planning, reorganizing, or advising.
