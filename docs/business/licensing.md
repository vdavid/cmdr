# Licensing

What the code license says, what a commercial buyer gets, and how commercial use is encouraged inside the app. Prices
and where they're hardcoded: `pricing.md`.

## The code license: BSL 1.1

`LICENSE` at the repo root is the authority; this is the shape of it.

- **Licensor**: Rymdskottkärra AB. Licensed work: Cmdr, © 2026 Rymdskottkärra AB.
- **The default BSL grant**: anyone may copy, modify, create derivative works, redistribute, and make **non-production**
  use of the source. Production use needs either an Additional Use Grant below or a commercial license.
- **Source-available, not open source.** The source is readable and buildable by anyone; reuse and republishing are not
  granted.

### Additional Use Grants

Two, and only two, cover production use without paying:

- **Personal use**: personal, non-commercial purposes, on as many of your own machines as you like, as long as you're
  the only user and the installation isn't shared.
- **Evaluation**: up to 14 days, to decide whether Cmdr suits your organization.

Anything else in production, including employment, contract work, and business activity, needs a commercial license.

### The AGPL-3.0 conversion

- The `Change Date` field in `LICENSE` names the day that shipped version's terms become **AGPL-3.0-or-later**.
- **It's a static field, rolled forward per release** by `scripts/release.sh`: each release sets it to three years from
  its own ship date, so every shipped build converts three years after that build shipped. Left alone, every version
  ever released would convert on one shared date and the protection window would shrink with each release.
- BSL takes whichever comes first, the Change Date or the version's fourth anniversary, so the anniversary can only pull
  conversion earlier. Three years leaves a year of headroom under that cap.
- **The pricing page sells the conversion on the Commercial card**, as the answer to "what happens if the one developer
  stops?". ❌ Keep that copy true to what `LICENSE` actually says: it's a promise a corporate buyer may rely on. Why it
  earns card space rather than a FAQ line: David's vault.

## What a commercial buyer receives

- A license key, delivered by email after the Paddle transaction completes. Keys are Ed25519-signed, self-contained
  (`base64(payload).base64(signature)`), and verify offline against a public key compiled into the app. The short form
  is `CMDR-XXXX-XXXX-XXXX`, exchanged server-side for the full key; the email carries the full key too, so a new Mac
  activates even with no server.
- **Per person, not per machine.** One license covers all of that person's own machines.
- **All features, Cmdr AI included.** AI is bring-your-own-key, so there is nothing metered and nothing extra to buy
  (`product-facts.md`). When the Pro plan ships (`pricing.md`), a license bought before it keeps AI with the buyer's own
  key or the on-device model for the length of the update window; only the hosted allowance is Pro-only.
- **One year of updates**, then the last entitled version stays theirs forever, renewal or not.
- A 30-day, no-questions-asked refund (`apps/website/src/pages/refund.astro`).
- Paddle is the merchant of record, so the invoice comes from Paddle and VAT/sales tax is handled there. EU business
  buyers with a VAT number are reverse-charged. The checkout collects a company name and email up front so the invoice
  carries them.

**Two license types exist in the code**: `commercial_subscription` and `commercial_perpetual`
(`apps/api-server/src/licensing/license.ts`). Only the perpetual one is sold now; the subscription type stays because
existing subscribers still validate against it. The app checks in with the server every 7 days when it can, only to
learn a revocation, expiry, or renewal (answers are signed, so nobody else can forge one). **A perpetual license stays
valid offline forever** and drops to Personal only on a verified revocation, never because the server is unreachable. A
time-limited license runs to a date: one signed into the key, or for a renewing subscription the last reported period
end plus 30 days. Rules and hostile cases: `apps/desktop/src-tauri/src/licensing/DETAILS.md` § Offline policy.

## How commercial use is encouraged in-app

Cmdr asks rather than enforces. What ships today, and the whole of it:

- **"Personal use only" sits in the main window title, always**, for every unlicensed user. Awkward to screen-share in a
  work meeting, and visible to an IT department walking past.
- **A reminder modal every 30 days** for unlicensed users (`CommercialReminderModal.svelte`). The 30-day timer starts on
  the first check rather than on first launch: showing it on day one would be a hostile first impression.
- **An expired commercial license reverts to Personal behavior, never a lockout.** The expiry modal shows once, then the
  app behaves as Personal. Nobody loses access to their files because a card expired.

❌ **Nothing here inspects what the user's files are.** The reminder is on a timer and knows nothing about the person
seeing it. Don't add a nag that reads the index, or describe one as shipping, without David deciding it deliberately: it
trades a privacy promise the product currently keeps for compliance.

## Where the words live

- `LICENSE`: the authority. Changed only deliberately; `scripts/release.sh` touches the Change Date and nothing else.
- `apps/website/src/pages/terms-and-conditions.astro`: the commercial terms a buyer agrees to. Still describes
  subscription renewals, because subscribers exist.
- `apps/website/src/pages/refund.astro`: the refund promise.
- `apps/website/src/pages/pricing.astro`: the source-availability and AGPL-conversion pitch, plus the FAQ definition of
  "commercial use".
- `README.md` and `CONTRIBUTING.md`: the short version for anyone landing on the repo.
