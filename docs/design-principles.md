# Design principles

- Prefer an elegant architecture over quick hacks. We have time to do outstanding work, and we are in this for the long
  run.
- **Platform-native, not generic.** The app should look and feel as if it was specifically made for the user's OS. Never
  generalize user-facing text, labels, or behavior to be "cross-platform" — instead, fork by OS. On macOS, say "Finder",
  "Trash", "System Settings". On Linux, say "file manager", "Trash" (FreeDesktop spec), and use DE-specific terminology
  where possible. Windows (later) gets its own native terms too. This applies to error messages, menu labels, tooltips,
  and any user-visible string. Use `isMacOS()` / `cfg(target_os)` to branch — a few extra lines of platform-specific
  text are always better than one watered-down generic string.
- Always apply radical transparency: make the internals of what's happening available. Like, don't just put a "Syncing"
  spinner but write exactly what's happening. Don't overshare/overcomplicate, but the user must understand what's
  happening to an extent that they could explain it to someone else if asked.
- Always make features extremely user-friendly. The UI should help the user accomplish their goals with minimal
  friction.
- This is a keyboard-first app. Everything must work with the mouse, too, but we should make it easy and straightforward
  to use all features with the keyboard.
- When shortcuts are available for a feature, always display the shortcut in a tooltip or somewhere less prominent than
  the main UI.
- For longer processes:
    1. Always run the process in the background. Blocking the UI or other actions is an absolute no-go.
    2. Show some anim to communicate that the app is doing something.
    3. If we know what the end state looks like and we can quantify the progress, show a progress bar and counter.
    4. If we have a guess how long the operation will take, show an ETA.
    5. Progress bars staying longer at 100% than at 99% or any other percentage is NOT allowed. If we're done with the
       part of an operation that we could quantify and displayed a progress bar for it but we have something else to do
       (e.g. we loaded the data and now we're making calculations on the data), then it's a new state, show another
       state!
- All actions longer than ~1 second should be immediately cancelable, canceling not just the UI but any background
  processes as well, to avoid wasting the user's resources. If rolling back is an option, we should consider that too.
- Always keep accessibility in mind. Features should be available to people with impaired vision, hearing, and cognitive
  disabilities.

For concrete tokens and component patterns, see [design-system.md](design-system.md).