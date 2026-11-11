# Design principles

- Prefer an elegant architecture over quick hacks. We have time to do outstanding work, and we are in this for the long
  run.
- **Platform-native, not generic.** The app should look and feel as if it was specifically made for the user's OS. On
  Windows that means a Windows app, not "a macOS app ported to Windows" and not a "cross-platform app": those are
  sporks, good at everything and great at nothing. Never generalize user-facing text, labels, or behavior to be
  "cross-platform". Instead, fork by OS:
  - **Words.** On macOS, say "Finder", "Trash", "System Settings". On Linux, say "file manager", "Trash" (FreeDesktop
    spec), and use DE-specific terminology where possible. Windows gets its own native terms ("File Explorer", "Recycle
    Bin", "Settings"). This applies to error messages, menu labels, tooltips, and any user-visible string. A few extra
    lines of platform-specific text are always better than one watered-down generic string.
  - **Controls and conventions.** Buttons, dropdowns, checkboxes, scrollbars, focus visuals, and menus look and behave
    like the platform's own, and dialogs follow its conventions (button order, default and cancel keys, access keys).
  - **Keys.** Each platform gets the shortcuts its users already have in their fingers, not a mechanical translation of
    the Mac ones.
  - **The goodies travel.** Whatever makes Cmdr good on macOS has its native counterpart elsewhere: VoiceOver support
    means Narrator and NVDA on Windows and Orca on Linux, the system accent and contrast settings are followed
    everywhere, and so on.
  - **Linux is its own platform**, never "whatever is left over". For each aspect it follows macOS, follows Windows,
    splits by desktop (GNOME, KDE), or goes its own way, and that choice is written down when the Windows side is built,
    so the Linux version can grow into it.
  - **How.** Ask the backend what the platform offers and how it behaves (`hasCapability()`, the platform conventions,
    `docs/guides/platform-seams.md`), never a bare "is this Windows" check: the same value then serves Linux without
    rework. The roadmap for Windows is `docs/specs/windows-port-plan.md` § "Native feel".
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
     (e.g. we loaded the data and now we're making calculations on the data), then it's a new state, show another state!
- All actions longer than ~1 second should be immediately cancelable, canceling not just the UI but any background
  processes as well, to avoid wasting the user's resources. If rolling back is an option, we should consider that too.
- Always keep accessibility in mind. Features should be available to people with impaired vision, hearing, and cognitive
  disabilities.
- **An approved action is the user's own.** When the user approves something Cmdr or Ask Cmdr suggested (a server card,
  a suggested file operation), it counts as user-initiated, exactly as if they had started it themselves: their stamp is
  on it, so it may do whatever the same user-started action may, and ❌ nothing downstream treats it as the agent's (no
  extra refusals, no weaker path). Why: the safety lives in the approval, so the work goes into showing the user what
  they're approving, and two paths to one action must never behave differently. The flip side holds too: what Cmdr does
  on its OWN initiative (a background wake, a follow-up turn, a probe nobody asked for) is not user-started, and waits
  for the user. Saving a server is one example: "Add anyway" and an approved card both save without contact, because
  saving is a store write and contact starts when the user connects (#777).

For concrete tokens and component patterns, see `design-system.md`.
