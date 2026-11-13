# Tell me about this (card) details

## The stream into the card

`openAboutThis(subject)` calls `startAbout` (`apps/desktop/src/lib/tauri-commands/about.ts`), which mints the request
id, and puts an empty card on screen at once. Each `AboutEvent` folds in through `applyAboutEvent`:

- `planned` sets the rows (once). Facts before it, or for a kind it doesn't list, are dropped.
- `fact` sets one row and replaces `derived` (the headline and removal row as the backend recomputed them).
- `factsSettled` and `unavailable` end the stream; nothing after them lands.
- An event whose `requestId` isn't the card's is dropped, so a reopen on another item never shows the first item's
  facts. `openAboutThis` closes (and cancels) any card already up before it starts the next.
