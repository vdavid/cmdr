# Nudges: architecture and decisions

Must-knows: `CLAUDE.md`. The two offers that use this: `apps/desktop/src/lib/dock/DETAILS.md` and
`apps/desktop/src/lib/reveal/DETAILS.md`. The ledger the thresholds count from:
`apps/desktop/src-tauri/src/usage/DETAILS.md`.

## The shape of a nudge

Every offer here is the same shape, and only the middle step differs:

1. `routes/(main)/+page.svelte` calls `maybeOfferNudges(startupGatesCtx)`, fire-and-forget, after
   `resolveOnboardingMount` and again from `handleWizardComplete`.
2. `startup-gates.ts::maybeOfferNudges` awaits each offer in turn. Each gathers the free `NudgeContext` (automated run,
   macOS, onboarding, wizard, the ledger, now) and runs `nudgeCouldFire`. Most launches stop there.
3. Otherwise the offer awaits its own two backend answers in parallel and runs its full `shouldShow…`.
4. The offer's raise module stamps the ledger, sends its `*_offered` event, and raises a persistent INFO toast.
5. The toast's buttons record an answer; the frame's × records `dismissed` through the `onDismiss` the raise installed.

## Decision: the ledger IS the cooldown, with no separate key

"When did we last nudge?" is the maximum over the per-offer stamps, so there is nothing to keep in sync. A dedicated
`behavior.lastNudgeAt` would be a second thing to write on every raise and a second thing to be wrong: one raise that
forgot it would silently switch the floor off for every other offer.

It also means "already asked" and "asked recently" read off the same value, so an offer can't be closed for good while
the cooldown thinks it never happened.

`emptyNudgeLedger()` exists so a test states "nothing has ever been offered" once rather than spelling out every kind,
and so adding a `NudgeKind` breaks one function instead of every fixture.

## Decision: three days, and it outranks the thresholds

The floor is about the EXPERIENCE of being asked, not about either feature: two offers in one week read as an app that
wants things from you, however individually reasonable each is. Three days is long enough that the second reads as a
separate moment, short enough that a weekly user still meets both within a month.

It is deliberately a property of ALL nudges rather than a rule about a pair. A third offer inherits the spacing by
declaring a `NudgeKind`; nobody has to remember to write "…and not within three days of the Dock offer" again.

The known interaction, which is the cooldown working: for someone who opens Cmdr every day, the reveal offer fires on
launch day 2, which pushes the Dock offer (threshold 4) to launch day 5 rather than 4. ❌ Don't exempt the Dock offer
from the floor. `should-show-dock-nudge.test.ts` pins this.

### The thresholds

- Reveal handler: **2** launch days. The more valuable offer of the two, so it gets the earlier slot.
- Dock pin: **4** launch days.

Both are "at least N", ❌ never "the Nth day is today": the launch-day ledger only started counting when it shipped, so
someone who has been here for months would otherwise never qualify.

## Decision: an ISO instant, not a local day

The comparison is an elapsed duration, which no calendar or timezone has an opinion about. The launch-day LEDGER is the
opposite (`usage/DETAILS.md`: "your third day using Cmdr" is a claim about the person's own calendar), and the two are
answering different questions.

Malformed and future stamps are read asymmetrically on purpose:

- `wasOffered` counts any non-empty string. A stamp nothing can parse still means the offer happened; only WHEN was
  lost, and reading it as "never asked" would offer a once-ever thing twice.
- `nudgeCooldownActive` ignores both. A machine where no offer ever fires again, with nothing saying why, is worse than
  two offers landing closer together after a clock jump.

## Decision: the toast body is shared, the copy is not

`NudgeToastContent.svelte` is markup only. The two offers' bodies were byte-identical apart from their message keys, and
`jscpd-frontend` would have counted that duplication fairly.

It takes RESOLVED strings rather than `MessageKey`s so each offer's `tString` calls stay in its own component: `tString`
read at raise time would freeze the launch language into a toast that lives until it's answered.

## Testing

- `nudge-ledger.test.ts`: the shared rule, including both sides of the cooldown, the two hostile stamps, and one offer
  holding another back.
- Each offer's own `should-show-*.test.ts` covers its threshold and its backend-state variants.
- `routes/(main)/startup-gates.test.ts` § `maybeOfferNudges` pins the sequencing: on a machine eligible for both,
  exactly one speaks.
