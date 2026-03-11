# Design principles

- Always make features extremely user-friendly.
- Always apply radical transparency: make the internals of what's happening available. Hide the details from the surface
  so the main UI is not cluttered.
- For longer processes: 1. show a progress indicator (an anim), 2. a progress bar and counter if we know the end state
  (for example, how many files we're loading), and 3. a time estimate if we have a guess how long it'll take.
- Always keep accessibility in mind. Features should be available to people with impaired vision, hearing, and cognitive
  disabilities.
- All actions longer than ~1 second should be immediately cancelable, canceling not just the UI but any background
  processes as well, to avoid wasting the user's resources.
- Prefer elegant architecture over quick hacks, but don't refactor beyond what the task requires.
- When shortcuts are available for a feature, always display the shortcut in a tooltip or somewhere less prominent than
  the main UI.
For concrete tokens and component patterns, see [design-system.md](design-system.md).

- **Platform-native, not generic.** The app should look and feel as if it was specifically made for the user's OS. Never
  generalize user-facing text, labels, or behavior to be "cross-platform" — instead, fork by OS. On macOS, say "Finder",
  "Trash", "System Settings". On Linux, say "file manager", "Trash" (FreeDesktop spec), and use DE-specific terminology
  where possible. Windows (later) gets its own native terms too. This applies to error messages, menu labels, tooltips,
  and any user-visible string. Use `isMacOS()` / `cfg(target_os)` to branch — a few extra lines of platform-specific
  text are always better than one watered-down generic string.
