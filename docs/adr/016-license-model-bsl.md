# ADR 016: License model — BSL with personal use grant

## Status

Accepted (supersedes [ADR-015](015-license-model-agpl-trial.md))

## Summary

Cmdr moves from AGPL-3.0 with a 14-day trial to BSL 1.1 with free personal use. Commercial users pay $59/year (subscription) or $199 (perpetual). The source converts to AGPL-3.0 after 3 years.

## Context

The AGPL + trial model (ADR-015) had friction:
- Trial countdown felt pushy for hobbyists
- "Nagware" after trial expiry wasn't a great experience
- Trial bypass was trivial (just delete the timestamp)

We wanted:
1. **Friction-free personal use** — hobbyists should just use it without any nags
2. **Clear commercial terms** — businesses know they need to pay
3. **Simpler enforcement** — honor system beats trial timers
4. **Source availability** — developers can still see and learn from code

## Solution

**BSL 1.1** with Additional Use Grant:

- **Personal use**: Free forever, unlimited machines, no nags
- **Evaluation**: 14 days to try it at work
- **Commercial**: Paid license required ($59/year or $199 perpetual)
- **Conversion**: Becomes AGPL-3.0 after 3 years (rolling per release)

### Pricing tiers

| Tier                    | Price    | Commercial?     |
|-------------------------|----------|-----------------|
| Personal                | Free     | No              |
| Supporter               | $10      | No (badge only) |
| Commercial subscription | $59/year | Yes             |
| Commercial perpetual    | $199     | Yes             |

### Enforcement

- **Title bar shows license type** — "Cmdr – Personal use only" is visible in screen shares
- **No trial countdown** — honor system for evaluation
- **Organization name displayed** — commercial licenses show "Licensed to: Acme Corp" in About window

## Consequences

### Positive

- Hobbyists use it freely without nags
- Clear "source-available" positioning (no confusing "open source but not really")
- Simpler codebase (no trial tracking)
- Better alignment with BSL-using companies (HashiCorp, Sentry, etc.)

### Negative

- Can't say "open source" anymore (BSL isn't OSI-approved)
- Honor system means some commercial users won't pay
- Requires API call for subscription status (vs offline-only trial)

## Notes

- The change date (3-year conversion) will be updated with each major release
- Machine IDs are not tracked — one license works on unlimited personal machines
- Subscription status is validated via Paddle API, cached locally with 30-day grace
