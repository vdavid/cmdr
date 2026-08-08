# Status corner: details

Depth for `CLAUDE.md`. Up: `apps/desktop/src/CLAUDE.md`.

## What's here

- `StatusCorner.svelte`: the row. One optional `children` snippet, then `IndexingStatusIndicator`.
- `StatusCorner.svelte.test.ts`: the structural contract (always mounted, children render before the hourglass).
- `StatusCorner.a11y.test.ts`: tier-3 axe pass, idle and populated.

## Layout model

One absolutely positioned flex row, right-aligned by virtue of `right: var(--spacing-sm)` and shrink-to-fit width: the
row grows leftward as members join, so the hourglass keeps the exact pixels it had when it positioned itself. Members
are separated by `--spacing-xs`.

The row sits in the initial containing block because no ancestor between it and the viewport is positioned
(`.main-content` in `routes/(main)/+page.svelte` is static). That's inherited behavior, not an accident: the hourglass
resolved its own `top`/`right` against the same box before the corner existed, and reproducing it is what makes the
extraction visually identical.

## Member contract

A member is a plain inline box:

- ❌ no `position` / `top` / `right` / `z-index` of its own (the row supplies all four),
- ✅ its own colour, focus ring, and any animation,
- ✅ `pointer-events: auto` comes free from the row's `> :global(*)` rule; a member that wants to stay click-through
  opts out itself.

The hourglass renders last so the eye finds it in the same place regardless of what else is showing; new members render
through `children`.

## Decisions

- **Why a wrapper at all.** Two independently-absolute boxes in one corner have to know each other's widths to avoid
  overlapping. A flex row makes placement one concern in one place, and adding a member becomes markup rather than
  arithmetic.
- **Why the corner is always mounted, even when empty.** Mounting the row conditionally would mean the members' own
  visibility gates and the row's gate could disagree, and a mount/unmount on every indexing run is churn for nothing.
  `pointer-events: none` makes an empty row free.
- **Why `children` rather than a members array.** Members differ in props, gates, and lifetime; a snippet lets the host
  compose them without the corner learning about any of them.
