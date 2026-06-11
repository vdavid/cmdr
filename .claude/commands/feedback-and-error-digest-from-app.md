Pull the in-app feedback messages and error-report bundles for a date range and hand David a synthesized digest.

This reads the app's own stores directly (D1 + R2), not Discord. The full access recipe (auth, the D1 query, the R2 REST
listing, the bundle layout) is in
[docs/tooling/feedback-and-error-digest.md](../../docs/tooling/feedback-and-error-digest.md). Read it first. This is
**read-only**: never delete or write anything in either store.

Arguments (optional): a date range, e.g. `since 2026-05-30`, `2026-05-30..2026-06-04`, or `last 7 days`.

## Steps

1. **Resolve the range.** If `$ARGUMENTS` names a usable range, use it. If it's empty or ambiguous, **ask David** in
   plain text before fetching, suggesting the last 7 days as the default. Resolve to a concrete `from` (and `to`, if
   given) date. Today's date is available in the session context.
2. **Feedback.** Run the D1 query from the doc for `created_at >= '<from>'` (and `< '<to-plus-1>'` if a `to` was given).
   Handle an empty result gracefully (early beta, may be zero rows).
3. **Error reports (prod only).** Generate the list of dates in the range, list each day's `error-reports/prod/<date>`
   prefix via the REST API, and collect the keys + `custom_metadata`. Download the bundles into a fresh temp dir
   (`mktemp -d`, e.g. under `/tmp`), unzip each, and read `manifest.json` + `logs/cmdr.log`. Skip the `dev` prefix
   unless David asks. Tell David the temp dir path so the unpacked bundles are there if he wants to dig in (they outlast
   Discord's 7-day links).
4. **Synthesize and return the digest in your message** (don't write it to a file). Cover:
   - **Feedback:** theme clusters (requests vs bugs vs praise vs confusion), volume, any spike tied to a release, a few
     verbatim standout quotes, and an explicit list of **items with a reply-to email** (people awaiting a response).
   - **Errors:** clusters/top error kinds by frequency, affected `appVersion`/`arch`, and **regressions** (error types
     that are new or spiking right after a release). Group bundles that are the same incident (pairs minutes apart with
     overlapping log timestamps, per the doc).
   - **Cross-cut:** tie an error spike to a feedback complaint where they line up (the synthesis a human skimming two
     Discord channels misses).
   - Note anything you couldn't determine and what bundle/ID to look at for a deeper dive.

Keep the digest impact-first and concise. Follow `docs/style-guide.md` (sentence case, active voice).
