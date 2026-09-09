# Reveal in Cmdr (frontend)

The two moments a user meets this feature: the once-ever offer to catch "Show in Finder", and the once-ever notice the
first time a reveal actually lands. Everything that touches the `NSFileViewer` key lives in Rust
(`src-tauri/src/reveal/CLAUDE.md`); the Settings switch is `$lib/settings/sections/RevealHandlerCard.svelte`.

## Module map

- **`should-show-reveal-nudge.ts`**: pure. `shouldShowRevealNudge` plus `REVEAL_NUDGE_AFTER_DAYS`. The shared half of
  the rule is `$lib/nudges/`.
- **`reveal-nudge.ts`** raises the offer toast and stamps the nudge ledger; **`reveal-nudge-answer.ts`** holds the three
  exits and the refusal copy, separate so the toast body can reach it without an import cycle.
- **`reveal-activation-notice.ts`** + **`RevealActivationToastContent.svelte`**: the first-landing notice.
- **`reveal-settings-link.ts`**: the card's anchor id and the one deep link to it.

## Must-knows

- **The offer fires ONLY on `notRegistered`, and only with `blockedBy === null`.** ❌ Never on `heldByOtherApp`: someone
  chose Path Finder or ForkLift on purpose, and offering to take a working setup over unprompted is rude. A take-over
  belongs in the Settings row, where the person asked for it. `registered` has nothing to offer, `unavailable` couldn't
  deliver, and a `blockedBy` copy (Cmdr running outside an Applications folder) would have its click refused by the
  backend.
- **Accepting renders what the OS was LEFT holding, ❌ never what the click asked for.** Another app can take the key
  between the toast being drawn and the button being pressed. Same rule the Settings card follows.
- **The activation notice is ❌ NOT a nudge**: it explains something the person set up themselves, so it takes no part
  in the shared cooldown and answers to its own `behavior.revealActivationNoticeSeen`.
- **The notice offers a way INTO Settings and ❌ never a "Turn it off" button.** Switching this back should cost one
  more click, in the place the setting lives.
- **`startRevealActivationNotice` must be subscribed BEFORE `drainPendingReveals`** (`window-services.ts`, phase 2), or
  a cold-launch reveal — the first one many people ever see — delivers with nobody listening.
- **The flag is read PER EVENT, ❌ never snapshotted at subscribe time.** The subscription outlives the window's whole
  session; a snapshot would arm a second notice for the rest of it.
- **A refusal is worded from its typed `RevealHandlerState` variant**, ❌ never from a message string
  (`cmdr/no-error-string-match`).

Why the notice exists at all, the copy's two beats, and the event that drives it: `DETAILS.md`.
