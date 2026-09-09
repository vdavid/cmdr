# Nudges

The shared rule behind every unsolicited once-ever offer: what Cmdr has already asked for, and how long one offer keeps
the next one quiet. Two offers use it today, `$lib/dock/` and `$lib/reveal/`; each keeps its own thresholds and its own
paid-for conditions.

## Module map

- **`nudge-ledger.ts`**: pure. `NudgeKind`, the `NudgeLedger` of ISO stamps, `NUDGE_COOLDOWN_DAYS`, `wasOffered`,
  `nudgeCooldownActive`, and `nudgeCouldFire` (the free prefix of every offer's rule).
- **`nudge-store.ts`**: the one table mapping a `NudgeKind` to its `behavior.*NudgeOfferedAt` setting, plus
  `readNudgeLedger` / `markNudgeOffered`.
- **`NudgeToastContent.svelte`**: the title + body + note + decline/accept body both offers wear. Presentational; it
  takes resolved copy, so each offer keeps its own `tString` calls and the strings still follow a live language switch.

## Must-knows

- **A stamp is a DATE, ❌ never a bool.** "Already asked" is `!== ''`; "how long ago" is what makes one cooldown cover
  every pair of offers instead of a new rule per pair.
- **`markNudgeOffered` runs when the toast is RAISED**, never when it's answered. A crash mid-toast then costs one
  offer; the other order risks repeating it forever. Same rule as `maybeFireUpgradeNudge`.
- **The gates must run SEQUENTIALLY** (`routes/(main)/startup-gates.ts::maybeOfferNudges`). Each stamps only after its
  own IPC round trips, so two started together both read an unstamped ledger and both speak on the same launch.
- **The cooldown outranks a threshold, and that's the design.** A daily user hears about the reveal handler on launch
  day 2, so the Dock offer lands three days later rather than exactly on launch day 4. ❌ Don't exempt an offer to "fix"
  it.
- **An unreadable or future stamp still counts as asked, but never blocks.** A bad clock must not re-offer a once-ever
  thing, and must not silence every remaining offer forever either.
- **A stamp setting is `type: 'string'`, which keeps it OUT of PostHog.** `analytics/config_shape.rs` auto-ships every
  bool and number setting; a string only ships if it's in `CATEGORICAL_STRING_KEYS`. ❌ Don't add one there.

Why the ledger has no separate "last nudge" key, and the per-offer thresholds: `DETAILS.md`.
