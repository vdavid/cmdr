# Dock nudge (frontend)

The one-time "keep Cmdr in your Dock?" offer: when to ask, the toast that asks, and the three events that answer "how
many saw it, how many said yes". Every decision that has to look at the machine lives in Rust
(`src-tauri/src/dock/CLAUDE.md`); this side only decides _when_ to ask.

## Module map

- **`should-show-dock-nudge.ts`**: pure. `shouldShowDockNudge` plus `DOCK_NUDGE_AFTER_DAYS`. The half every offer shares
  — automated run, macOS, onboarding, "already asked", the cooldown — is `$lib/nudges/`.
- **`dock-nudge.ts`**: raises the toast, stamps the nudge ledger, sends `dock_pin_offered`.
- **`dock-pin-answer.ts`**: the three exits, `dock_pin_answered` / `dock_pin_failed`, and the failure copy. Separate so
  the toast body can reach it without an import cycle.
- **`DockPinNudgeToastContent.svelte`**: this offer's copy over the shared `$lib/nudges/NudgeToastContent.svelte`. The
  gate that calls in is `routes/(main)/startup-gates.ts::maybeOfferDockPin`.

## Must-knows

- **The ledger is stamped when the toast is RAISED, never when it's answered.** A crash mid-toast then costs one offer;
  the other order risks repeating it forever. Same rule as `maybeFireUpgradeNudge`.
- **Four launch days, and the offer can still land later than that.** The shared three-day nudge cooldown outranks the
  threshold, so a daily user who met the reveal offer on day 2 hears about the Dock on day 5. ❌ Don't exempt this offer
  from the floor; `$lib/nudges/CLAUDE.md`.
- **`get_dock_pin_state` already answers "already pinned" and "installed where a tile may point".** ❌ Don't re-derive
  either here: one call, one `DockPinState`, and only `offerable` speaks up.
- **`nudgeCouldFire(ctx, 'dockPin')` runs before either IPC**, so the ordinary launch (the offer already made) costs
  nothing. `shouldShowDockNudge` calls it too, so the two can't drift.
- **The copy says "a few days", ❌ never a number.** The threshold can move and the ledger only started counting when it
  shipped; a number would make the sentence a lie for half the users.
- **Every exit lands on a `dock_pin_answered`**, and the toast frame's × reports `dismissed`, not `no`. That split is
  the interesting half of the question, so ❌ don't collapse it. `yes` records the press, never the outcome;
  `dock_pin_failed` carries the rest.
- **A refusal is worded from its typed variant**, ❌ never from a message string (`cmdr/no-error-string-match`).
  `dockNotRestarted` is the trap: the tile IS stored, so its copy must not say the pinning didn't happen.
- **Accepting blinks the whole Dock for about a second** while it restarts. Expected, and approved; the toast is retired
  before the write so the blink doesn't read as "nothing happened".

Why the decision splits in two, the failure-copy table, and the wizard re-attempt: `DETAILS.md`.
