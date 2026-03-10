# Viewer

The file viewer opens files in a separate Tauri window with virtual scrolling and text search.

## Files

| File                      | Contents                                                                           |
| ------------------------- | ---------------------------------------------------------------------------------- |
| `+page.svelte`            | Top-level component: lifecycle, key handling, indexing poll, window management, UI |
| `viewer-scroll.svelte.ts` | Virtual scroll composable: line cache, fetch debounce, scroll compression, effects |
| `viewer-search.svelte.ts` | Search composable: start/poll/cancel/navigate, match highlighting, debounce        |

## Architecture

The page component creates two composables via `createViewerScroll` and `createViewerSearch`. Both use callback-based
deps (getters) so they can read reactive state from the page without receiving `$state` directly (which would lose
reactivity). The page owns session-level state (`sessionId`, `totalLines`, `backendType`, etc.) and wires the
composables together.

Effects live in the page component but delegate to `run*Effect()` methods on the composables, because `$effect()` only
works in `.svelte` or `.svelte.ts` files at the top level of a component or `createXxx` function scope.

## Gotchas

- `$state(false)` in `.svelte.ts` triggers `@typescript-eslint/no-unnecessary-condition` because the linter doesn't know
  the value is mutated via Svelte reactivity. Use an inline eslint-disable comment with a reason.
- `LINE_HEIGHT` (18px) must stay in sync with the `.line { height: 18px }` CSS rule in `+page.svelte`.
