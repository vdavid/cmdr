# Dock nudge: architecture and decisions

The frontend half of the Dock integration. Must-knows: `CLAUDE.md`. The backend half (CFPreferences, the tile shape, the
Dock restart): `apps/desktop/src-tauri/src/dock/DETAILS.md`. The launch-day ledger the gate counts from:
`apps/desktop/src-tauri/src/usage/DETAILS.md`. The offer machinery this shares with the "Show in Finder" offer — the
raise-time stamp, the free prefix of the rule, the three-day cooldown, and the toast body:
`apps/desktop/src/lib/nudges/DETAILS.md`.

## What's this offer's own

The shape of a nudge, end to end, is in `apps/desktop/src/lib/nudges/DETAILS.md`. Only two things here are the Dock's:

- **`DOCK_NUDGE_AFTER_DAYS` is 4**, one behind the reveal offer, which is the more valuable of the two and takes the
  earlier slot. "At least four days", ❌ not "the fourth day is today".
- **`pinState.kind === 'offerable'`** is the second condition, and the whole of it (below).

`dock-pin-answer.ts` is separate from `dock-nudge.ts` because the toast body has to reach the answers, and the body is
what `offerDockPin` mounts: one module holding both would be a cycle, which `import-cycles` fails.

## Decision: `DockPinState` is asked once and believed

The brief's conditions included "not already in the Dock" and "running from an Applications folder". Both are answers
`pin_state()` already folds into one value, and re-deriving either on this side would mean a second opinion about the
same machine that could disagree with the one the pin itself re-checks. So the frontend rule is one line:
`pinState.kind === 'offerable'`.

`pin_cmdr()` re-checks everything anyway, which is what makes a stale `Offerable` harmless: the answer can only go from
"yes" to a typed refusal between the offer and the press, never into a write nobody would have offered.

## Decision: the wizard re-attempt

A person can reach their fourth launch day and finish onboarding on the same launch. The boot call runs after
`resolveOnboardingMount`, which may have just opened the wizard, so the gate would see `onboardingShowing: true` and
quietly skip — and the flag is unspent, so it would come back next launch. That's survivable but wasteful, so
`handleWizardComplete` re-attempts, mirroring the "What's new" and update-toast re-attempts. It goes through
`maybeOfferNudges`, so the re-attempt keeps the offers in order and one at a time.

`ctx.isOnboardingVisible()` is a live getter, ❌ never a captured value: the re-attempt reads it after
`setOnboardingVisible(false)` in the same handler, and a snapshot would answer `true`. `notifyOnboardingComplete()`
writes `onboarding.completed` synchronously before its first `await`, so the re-attempt sees it even though the call
isn't awaited.

## The failure copy

`dock-pin-answer.ts::acceptDockPin` maps a `DockPinFailure` to one of three messages. The reason it isn't one message
per variant: only two of them change what the person should do.

- `dockNotRestarted` → `main.dockPinNudge.addedButDockDidNotRestart`. The tile is stored; only the redraw is missing,
  and a login supplies it. Saying "couldn't add it" here would be false.
- `blocked` + `managedDock` → `main.dockPinNudge.managedDock`. Somebody else's policy owns the Dock, so no manual
  workaround would stick either. Naming who can lift it is the only useful thing to say.
- everything else → `main.dockPinNudge.notAdded`, which offers the manual drag from Applications.

All three are `warn`-level toasts and none uses the words "error" or "failed" (`docs/style-guide.md`).

## The three events

`dock_pin_offered` → `dock_pin_answered` is the whole funnel: offers raised, and how they were answered. Both are
frontend events through `trackEvent`, since the decision and the toast are both here; the check that pins them to
`analytics/DETAILS.md` § "Starter event set" recognizes the literal `trackEvent('name'` shape, so ❌ never route one
through a helper taking a runtime name.

`answer: 'yes'` records the press, not the outcome. Conditioning it on success would make the funnel measure the Dock's
cooperation instead of the person's intent, and would leave a failed pin with no answer at all. `dock_pin_failed`
carries the outcome, and its `reason` is a typed variant name on both sides, so a new `DockPinFailure` variant shows up
in the dashboard under its own name rather than folding into an existing bucket.

## Testing

- `should-show-dock-nudge.test.ts`: the rule, including each `DockPinState` variant, both sides of the threshold, and
  the reveal offer holding this one back.
- `dock-nudge.test.ts`: both modules, since they're one flow — the ledger stamp, the toast options (so the `onDismiss`
  wiring can't be lost), and one row per failure variant tying a typed reason to its copy.
- `DockPinNudgeToastContent.a11y.test.ts`: axe over the mounted body, like every other toast content component.
- `routes/(main)/startup-gates.test.ts` § `maybeOfferDockPin`: the wiring, including that a spent stamp costs no IPC and
  that the wizard flag is read live.
