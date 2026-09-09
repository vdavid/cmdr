# Reveal in Cmdr (frontend): architecture and decisions

Must-knows: `CLAUDE.md`. The mechanism, the `NSFileViewer` key, and how a reveal arrives:
`apps/desktop/src-tauri/src/reveal/DETAILS.md`. The shared offer rule: `apps/desktop/src/lib/nudges/DETAILS.md`. The
Settings switch: `apps/desktop/src/lib/settings/DETAILS.md` § OS-backed rows.

## Two moments, one feature

The feature is one OS switch, but a person meets it twice and the two moments have nothing in common:

1. **The offer**, a couple of launch days in. One of the two nudges; the whole shape is in
   `apps/desktop/src/lib/nudges/DETAILS.md`.
2. **The first landing**, which can be a week later. That's what the activation notice is for.

## The offer

`shouldShowRevealNudge` adds two conditions to the shared `nudgeCouldFire`: at least `REVEAL_NUDGE_AFTER_DAYS` (2)
distinct launch days, and a handler state of `notRegistered`.

**Decision: only `notRegistered` speaks.** The other three states are each a deliberate silence.

- `registered`: reveals already land here.
- `heldByOtherApp`: Path Finder and ForkLift ship exactly this key and nothing else, so a person holding one chose it.
  An unprompted offer to take it over is the difference between a helpful app and a pushy one. The Settings row does
  offer the take-over, because there the person went looking for it.
- `unavailable`: a debug, worktree, or E2E build (which must never write the key) or a platform with no mechanism. The
  click could not deliver anything.

**Decision: accepting reports the state the OS was LEFT in.** `setRevealHandlerEnabled` answers with what it found, not
what it was asked for, and `acceptRevealNudge` words that answer. The race is real — the key is a shared global that any
app can take at any moment — and it's the same reason `RevealHandlerCard` re-derives its switch from the returned state.
`heldByOtherApp` earns its own line because it names who won; `notRegistered` and `unavailable` collapse into one "not
this time" plus the switch's address, because they look identical from here and lead to the same next step.

The answer records the PRESS, ❌ never the outcome: what's being measured is whether people want this.
`reveal_handler_not_taken` carries the rest, keyed by the typed variant so a new one reaches the dashboard under its own
name. The three events and how to read them: `apps/desktop/src-tauri/src/analytics/DETAILS.md`.

## The activation notice

**Why it exists.** Cause and effect are days apart. Someone accepts the offer, then a week later clicks "Show in folder"
in Chrome and an app they did not invoke jumps in front of them, in a window they were not looking at. Without a word
the first landing reads as Cmdr misbehaving. After the first one it reads as the feature working, which is why the
notice is once-ever rather than throttled.

**What it says**, in two beats: what just happened and why it was Cmdr, then where the switch is. It is ❌ not an
apology, because nothing went wrong.

**Decision: a link into Settings, ❌ never a "Disable" button.** A one-click undo sitting on an unexpected toast invites
people to switch off a thing they chose two minutes of reading ago. Turning it back off should cost one more click, in
the place the setting actually lives, where the same row also explains what it does.

**Decision: it rides its own backend event, ❌ not the drain's return value.** A reveal reaches an already-running Cmdr
too, and only the backend can tell a reveal-driven pane move from an MCP one — by the time `mcp-nav-to-path` reaches
this side, nothing remembers who asked. `RevealDelivered` is emitted only after the pane move actually lands, so a
reveal onto a dead mount doesn't spend the notice.

It's payloadless: the paths are already on their way over `mcp-nav-to-path`, and a second copy would be a second thing
that can disagree.

**The cold-launch path is the interesting one.** A reveal that launched Cmdr is delivered after `drain_pending_reveals`,
which the frontend calls at the end of `window-services.ts` phase 2. The subscription is started in the same phase,
immediately before that call, so the announcement of the very first reveal someone ever receives has a listener waiting.
❌ Don't reorder those two lines.

**Decision: it is not a nudge.** It answers something the person set up themselves rather than asking them for anything,
so it takes no part in the shared cooldown: suppressing it because the Dock offer spoke two days ago would leave the
surprise unexplained, which is the whole thing it's for. Its `behavior.revealActivationNoticeSeen` stays a bool, since
nothing needs to know how long ago it fired.

Transient, 10 seconds. Nothing on it needs an answer, and the pane the reveal just landed in is exactly what a
persistent toast would sit on top of.

## Testing

- `should-show-reveal-nudge.test.ts`: the rule, including each `RevealHandlerState` variant and both sides of the
  threshold.
- `reveal-nudge.test.ts`: both raise and answer modules, since they're one flow — the ledger stamp, the toast options
  (so the `onDismiss` wiring can't be lost), and one row per resulting state tying a typed reason to its copy.
- `reveal-activation-notice.test.ts`: once-ever across repeat events, the self-dismiss options, and that the flag is
  read per event rather than at subscribe time.
- `RevealNudgeToastContent.a11y.test.ts` / `RevealActivationToastContent.a11y.test.ts`: axe over each mounted body.
- `routes/(main)/startup-gates.test.ts` § `maybeOfferRevealHandler`: the wiring, including that a spent stamp costs no
  IPC.
