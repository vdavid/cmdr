# Website details

Deep docs for `apps/website`. The always-loaded counterpart is [CLAUDE.md](CLAUDE.md).

## Client-side storage policy (no consent banner, ever)

The site must never need a cookie consent banner. This section holds the legal reasoning; the imperatives live in
`CLAUDE.md` § Analytics.

The banner requirement comes from the ePrivacy directive (Article 5(3)), not GDPR itself. It covers ALL storage on the
visitor's device (cookies, localStorage, sessionStorage alike), but exempts storage that's strictly necessary for a
service the user requested, including "UI customization" (WP29 Opinion 04/2012, still the reference guidance). So the
line that matters is **preference vs tracking**:

- **Preference flags need no banner.** The theme choice, download-arch choice, and the newsletter form's
  dismissed/subscribed flags live in localStorage. They carry no identifier, never leave the device, and exist for the
  user's benefit — squarely inside the UI-customization exemption. They're not personal data, so GDPR isn't triggered
  either. David has settled this; don't re-raise it or flag it as a compliance problem.
- **Tracking storage is never okay.** Anything that identifies, follows, or attributes a visitor (analytics IDs,
  attribution state, session cookies) is consent-requiring storage and would force a banner. The trade-off is settled:
  anonymous aggregate insight is a win; individual tracking at the cost of a banner is not.

How the site tracks anonymously instead:

- Umami: cookieless by design, proxied to dodge adblockers.
- PostHog: `persistence: 'memory'` and `person_profiles: 'identified_only'`, so no device storage.
- Download attribution: pure URL state (`?ref=`, see `CLAUDE.md` § Analytics), nothing persisted client-side.
- Server side: daily-hashed IPs, no raw IP storage.

When adding analytics tooling or any feature that wants client-side persistence, classify it with the preference vs
tracking test above. Preference: localStorage is fine. Tracking: find a storage-free design (URL state, server-side
aggregation) or drop the feature.
