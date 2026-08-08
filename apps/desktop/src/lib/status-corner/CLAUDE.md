# Status corner

The main window's top-right ambient-status row: `StatusCorner.svelte`, mounted once by `routes/(main)/+page.svelte`. It
hosts `$lib/indexing/IndexingStatusIndicator.svelte` (the hourglass) and renders any `children` to its left.

## Must-knows

- **The corner owns placement, its members don't.** `StatusCorner` carries `position: absolute`, the `--spacing-sm`
  offsets, and `--z-sticky`; each indicator inside is a plain inline box. A member that positions itself would overlap
  its neighbours instead of sitting beside them.
- **No positioned ancestor, on purpose.** `.main-content` is static, so the corner's offsets resolve against the initial
  containing block, which is where the hourglass has always sat. ❌ Don't add `position: relative` to an ancestor to
  "fix" it: that moves the corner.
- **The row is always mounted, so it's `pointer-events: none`** with `auto` on its children (the `ToastContainer`
  pattern). An empty or gap-sized box over the pane must not eat clicks.
- **The hourglass stays last.** It's the most ambient member; new affordances go through `children`, left of it.

Layout model, member contract, and decisions: `DETAILS.md`.
