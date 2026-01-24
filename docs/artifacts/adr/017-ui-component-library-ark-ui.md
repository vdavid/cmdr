# ADR 017: Adopt Ark UI as the headless component library

## Status

Proposed

## Summary

We need a consistent, accessible component library for UI elements like buttons, dialogs, tabs, dropdowns, and
checkboxes — starting with a Settings dialog. After evaluating 10+ libraries, we chose Ark UI: a headless, Svelte 5
component library built on Zag.js finite state machines, offering 45+ components with full a11y, vanilla CSS support,
and explicit focus/escape control.

## Context, problem, solution

### Context

Cmdr is a Tauri 2 desktop app (Svelte 5, TypeScript, vanilla CSS with custom properties). Key constraints:

- **No Tailwind** — removed for ~15s dev startup improvement. The app uses vanilla CSS with variables in `app.css`.
- **Must look native** — macOS today, Linux/Windows soon. Must not look like a web app.
- **Good a11y** — keyboard navigation, focus management, ARIA, screen reader support.
- **AI writes code** — learning curve and boilerplate are irrelevant; robustness and feature count matter more.
- **Close focus control** — the file manager needs precise control over focus trapping, escape key behavior, and
  focus restoration between panels and dialogs.

### Problem

The app has inconsistent button/control styling, no shared component primitives, and is about to add a Settings dialog
requiring buttons (primary/secondary), tabs, checkboxes, dropdowns, switches, sliders, and more. Building all of these
from scratch with correct a11y is a large effort with many subtle bugs.

### Possible solutions considered

#### Opinionated/styled libraries (rejected)

| Library | Why rejected |
|---|---|
| shadcn-svelte (~8.2k stars) | Tailwind-dependent, web-app aesthetic, copy-paste maintenance burden |
| Skeleton UI v3 | Tailwind-heavy, web-focused design language |
| Carbon Components Svelte | IBM design language, not OS-native |
| Flowbite Svelte | Bootstrap-like web aesthetic |
| DaisyUI | CSS-only (no reactive components) |

All of these impose a visual style that fights a native desktop look. They also typically depend on Tailwind.

#### Headless libraries (shortlisted)

Evaluated three headless options in depth:

| Aspect | Bits UI | Ark UI | Melt UI |
|---|---|---|---|
| Stars / forks | ~3,000 / 180 | ~4,900 / 236 | ~4,100 / 224 |
| License | MIT | MIT | MIT |
| Svelte 5 | Full | Full (Svelte 5 only) | Full (runes branch) |
| Component count | ~20–25 | 45+ | ~20 |
| Bundle (unpacked) | ~1.95 MB | ~1.33 MB | Smallest |
| Tree-shaking | Known issues (137 KB for DatePicker) | Clean | Good |
| CSS approach | `[data-dialog-content]` flat attributes | `[data-scope="dialog"][data-part="content"]` scoped | Bring your own |
| State management | Manual runes + callbacks | Zag.js FSMs | Svelte stores/runes |
| Focus control | Direct `preventDefault()` on callbacks | Disable FSM defaults + callbacks (equivalent) | Direct |
| Svelte idiomacy | Native Svelte-first | Adapter over Zag.js | Native |
| Animation | Svelte transitions (native) | Svelte transitions or CSS | Svelte transitions |
| Maintained by | huntabyte (solo + community) | Chakra UI team | Community |

**Melt UI** — lowest-level (builder pattern), good for maximum control but more boilerplate with no benefit given AI
writes the code. Fewer components than Ark.

**Bits UI** — middle ground, Svelte-native, but fewer components (~20–25 vs 45+), known tree-shaking/bundle issues
(a single DatePicker adds ~137 KB gzipped), and maintained primarily by one person.

### Solution

**Ark UI** (`@ark-ui/svelte`).

#### Why Ark UI wins

1. **Component count (45+)** — Switch, Slider, Segment Group, Field, Tree View, Splitter, Toggle Group, Number Input,
   Tags Input, and more. These are exactly what a Settings dialog and a file manager need. Bits UI lacks most of these.

2. **Better bundle** — 1.33 MB unpacked, clean tree-shaking. No known per-component bloat issues.

3. **FSM robustness** — Zag.js state machines prevent invalid states automatically. Complex interactions (nested
   dialogs, focus traps, popover-inside-dialog) are handled at the FSM level, reducing subtle bugs.

4. **Full focus/escape control** — despite FSM approach, you get full override:
   ```svelte
   <Dialog.Root
     closeOnEscape={false}
     closeOnInteractOutside={false}
     trapFocus={false}
     restoreFocus={false}
     onEscapeKeyDown={(e) => { e.preventDefault(); /* custom */ }}
     onInteractOutside={(e) => { e.preventDefault(); }}
     initialFocusEl={() => myRef}
     finalFocusEl={() => panelRef}
   >
   ```
   Pattern: disable FSM defaults with `={false}`, then implement custom logic in callbacks.

5. **Scoped CSS selectors** — `[data-scope="dialog"][data-part="content"][data-state="open"]` avoids collisions and
   works perfectly with our vanilla CSS custom properties.

6. **Team-maintained** — Chakra UI team, frequent releases (v5.15.0 released Jan 2026), multi-framework investment
   ensures longevity.

7. **TypeScript** — FSM types prevent invalid state transitions at compile time.

#### How we'll use it

- Install `@ark-ui/svelte` as a dependency.
- Style components with vanilla CSS using `data-scope`/`data-part`/`data-state` selectors and existing CSS variables.
- Build a thin `<Button>` wrapper ourselves (a button needs no headless library — it's already accessible natively).
- Use Ark UI for complex interactive components: Dialog, Tabs, Select, Checkbox, Switch, Slider, Radio Group, etc.
- Use Svelte transitions with `present` prop for animations.

## Consequences

### Positive

- Consistent, accessible UI across the entire app with minimal custom a11y code.
- 45+ components available — covers current and foreseeable needs without building from scratch.
- Clean DOM output (asChild snippet pattern, minimal wrappers).
- No Tailwind dependency — works with our vanilla CSS approach.
- Cross-platform consistency: same behavior on macOS, Linux, Windows webviews.

### Negative

- Framework adapter layer means slightly less Svelte-idiomatic code than a Svelte-native library.
- Zag.js FSM is an abstraction layer to understand when debugging edge cases.
- Overriding FSM defaults requires explicit `={false}` props (slightly more verbose than direct control).
- Dependency on Chakra UI team's continued investment in Svelte adapter.

### Notes

- Ark UI docs: https://ark-ui.com
- Ark UI GitHub: https://github.com/chakra-ui/ark
- Zag.js (underlying FSM): https://zagjs.com
- npm: https://www.npmjs.com/package/@ark-ui/svelte
- `<Button>` and other simple styling-only components will be our own thin wrappers, not from Ark UI.
- Bits UI remains a viable fallback if Ark UI's Svelte adapter quality degrades.
