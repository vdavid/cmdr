# Tell me about this (card)

The "Tell me about this" fact card (#1104): a resizable dialog that streams what Cmdr knows about the item under the
cursor. The backend (`apps/desktop/src-tauri/src/about/CLAUDE.md`) gathers; `crates/cmdr-about` owns every rule; this
directory only renders.

## Module map

- `about-card.ts`: the card's data and `applyAboutEvent`, the one reducer (pure, shared with the gallery).
- `about-this-state.svelte.ts`: the open card (`openAboutThis` / `closeAboutThis`), fed by `startAbout`.

## Must-knows

- ❌ **No rule here.** Which rows exist, prominence, the headline, and the removal row arrive typed from Rust; the card
  words them. A "should we show this" question goes to `cmdr-about`.
- ❗ **Drop what isn't this card's.** `applyAboutEvent` ignores another request's events, a fact outside the plan, and
  anything after `factsSettled`, so a quick close-and-reopen never shows the first item's facts.
- ❗ **Close cancels through `about_cancel`**, ❌ never by dropping the channel (`send` succeeds after the handler is
  gone).

Design, flows, and the copy list: `DETAILS.md`.
