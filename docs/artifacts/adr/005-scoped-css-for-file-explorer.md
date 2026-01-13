# ADR 005: Use Scoped CSS for File Explorer, Tailwind for Other UI

## Status

Accepted

## Context

The project uses Tailwind CSS v4 as configured in the build system. However, the file explorer components (FileList,
FilePane, DualPaneExplorer) need to render potentially large lists of files efficiently (stress tested with 50k+ files).

We need to decide whether to:

- Use Tailwind utility classes throughout the application consistently
- Use scoped CSS in Svelte components for performance-critical areas
- Mix both approaches based on use case

## Decision

We will use **scoped CSS for the file explorer components** (anything rendering file lists or performance-critical
virtual lists) and **Tailwind CSS for other UI components** (settings panels, modals, toolbars, dialogs, etc.).

## Consequences

### Positive

- **Better performance for large lists**: Scoped CSS produces smaller DOM (no repetitive utility classes on each file
  entry), resulting in faster rendering and lower memory usage with thousands of files
- **Precise control**: File managers need pixel-perfect column alignment, text overflow handling, and exact spacing
  that's easier to achieve with targeted CSS
- **Simpler HTML**: Each file entry has minimal attributes (`class="file-entry"`), making the virtual DOM diffing faster
- **Easier to optimize**: Can use CSS properties like `content-visibility` without fighting framework conventions
- **Still use Tailwind where appropriate**: Settings, modals, and other non-list UI benefit from Tailwind's rapid
  prototyping and consistency

### Negative

- **Mixed styling approach**: Developers need to know when to use which approach
- **Less visual consistency**: File explorer styling won't match Tailwind's design tokens unless manually aligned
- **Duplication of common patterns**: Things like spacing, colors may be defined in both scoped CSS and Tailwind config

### Notes

This follows the common practice in high-performance UIs like VS Code, Finder, and other file managers that use targeted
CSS for list rendering while using component libraries elsewhere.

The decision can be revisited if we implement a virtualized list library that better integrates with Tailwind, or if
performance measurements show no meaningful difference.

**Guideline**: If a component renders >100 repeated items, prefer scoped CSS. Otherwise, use Tailwind for consistency.
