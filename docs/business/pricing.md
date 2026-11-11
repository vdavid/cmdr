# Pricing

The live tiers, and every place a price or a plan name is hardcoded. The second half is the point of this file: a price
change that misses one spot leaves the site quoting two different numbers, or worse, charges the wrong one.

## The live tiers

Two tiers, one of them paid.

- **Personal**: free. The whole file manager, unlimited machines of your own, automatic updates. Personal use only. Cmdr
  AI is included **"free during the beta"**, in those words: the page says it runs on your own key or on-device, that a
  Pro plan with hosted models **may come later**, and that the file manager stays free for personal use. ❌ Never write
  "free forever" on any surface, and never describe Cmdr AI as part of the free tier without the beta qualifier: the
  wording has to leave room for the Pro plan without it reading as a rug pull. ❌ Equally, never promise the Pro plan
  ("moves to", "is coming", a date): it's under consideration, not decided (`product-facts.md`).
- **Commercial**: **$59, paid once**, per person, on as many of that person's own machines as they like. Every feature,
  Cmdr AI included. One year of updates is included; after that year, **$39/year** keeps new versions coming, and the
  last version the buyer was entitled to stays theirs forever whether or not they renew.
- **Organizations**: quoted by hand. ❌ **Nothing is published**: no price, no seat threshold, no list of inclusions.
  The page carries a "Buying for an organization?" block with one line pointing at `sales@getcmdr.com`, and the FAQ says
  a handful of people should just buy Commercial licenses one by one. Every number or promise on that block is something
  a buyer negotiates down from. The internal volume ladder and the deal floors live in David's vault (`Cmdr pricing.md`
  there); ❌ they don't belong in this repo, which is public.

No recurring commercial plan is sold. No separate perpetual tier: Commercial _is_ the perpetual one.

**Cmdr AI** is named as a distinct component in the pricing copy. It's bring-your-own-key today (`product-facts.md`), so
unlimited AI on a one-time license costs nothing to serve. The page and FAQ promise that a Commercial license bought now
keeps Cmdr AI on the buyer's own key or the on-device model for the length of their update window, ❌ so the license
server has to keep honoring that however the plans change later.

A **Pro plan** for hosted AI models is mentioned on the page as something that may come later, without a price or a
date. Its design and pricing live in David's vault (`Cmdr pricing.md` there) and ❌ don't belong in this repo until it
ships; here, only the copy constraint above matters.

### Retired

- **Commercial subscription, $59/year** (struck through from $79, badged "Most popular" and "First 1,000 licenses").
- **Perpetual, $199 one-time.**

❌ **Never unset a retired tier's price mapping while licenses bought under it are still live**, and archive its Paddle
price rather than deleting it. A price ID that stops resolving takes every license bought under it with it.
`PRICE_ID_COMMERCIAL_SUBSCRIPTION` stays set for exactly this reason.

❌ **$59 now means two different things, and this is the trap in this file.** The retired subscription was $59 per year;
the current Commercial license is $59 paid once. A grep for `59` cannot tell them apart, and neither can a reader
skimming. The distinguishing token is `/year` or `/yr` next to the number, which is why the pricing page's E2E test
asserts on `$59/year`being absent rather than on`$59`. When writing about either, always say which.

## Every place a price or plan is hardcoded

### Website copy (`apps/website/`)

- `src/pages/pricing.astro`: the tier cards, the organization block, the `faq` array in frontmatter (which feeds BOTH
  the visible cards and the `ld+json` `FAQPage`), and the `<Layout description>`.
- `src/pages/index.astro`: the `ld+json` `SoftwareApplication` `offers` array. Structured data, so it is what Google
  quotes; it is easy to miss because no visitor sees it.
- `src/components/Hero.astro`: the pricing hint under the home fold.
- `src/components/Download.astro`: the line under the download card.
- `src/pages/renew.astro`: the whole page is about the $39 update renewal.
- `src/pages/llms.txt.ts` and `src/pages/llms-full.txt.ts`: the "Key links" pricing line, the Pricing section, and the
  pricing FAQ answers in the full version. Agent-facing, so a stale number here is what an AI tells a buyer.
- `src/pages/terms-and-conditions.astro`: names subscriptions and perpetual licenses in the renewal, liability, and
  change-of-terms clauses. Legal text, so it changes on David's call, not as a copy edit.
- `src/content/blog/total-commander-for-macos/index.md`: the comparison table's "Free for personal use" row.

### Paddle and checkout wiring

- `PUBLIC_PADDLE_PRICE_ID_COMMERCIAL` (website build env; `.env.example`, `src/env.d.ts`, and the `paddleConfig` object
  in `pricing.astro`). Holds the $59 one-time price ID. Unset leaves the buy button disabled, which is the safe failure:
  a wrong price ID charges a real customer the wrong amount with no error anywhere. Astro bakes `PUBLIC_*` into the
  static build, so the value lives in `/opt/cmdr/apps/website/.env` on the VPS and only takes effect on the next deploy.
- The `data-paddle-price="commercial"` attribute on the buy button is the key into `paddleConfig.priceIds`.
- `PRICE_ID_COMMERCIAL_PERPETUAL` (api-server wrangler secret; declared in `src/types.ts`, read in
  `src/licensing/licensing.ts`). Must point at the SAME $59 price ID as the website var. The name says "perpetual"
  because that's the license type the webhook issues, not because a $199 tier still exists.
- `PRICE_ID_COMMERCIAL_SUBSCRIPTION` (api-server): the retired $59/year price ID. **Stays set**: licenses sold under it
  are still live, so the variable and the `commercial_subscription` license type stay in code as long as they are.
- ❌ **An unmapped price ID falls back to `commercial_subscription`** (`getLicenseTypeFromPriceId` in `paddle-api.ts`
  returns `null`, and `licensing.ts:343-345` substitutes the subscription type). So a Paddle price nobody wires into
  `PRICE_ID_COMMERCIAL_PERPETUAL` silently sells a license that expires, with a fulfillment email promising annual
  renewal. This is the single most expensive thing to get wrong here. Whether to fail loudly instead is an open decision
  in David's vault, so ❌ don't change the fallback without checking there first.
- Sandbox and live never share price IDs (`apps/api-server/src/licensing/CLAUDE.md`).

**The live price IDs** (not secret: they ship in the built site's client-side JS, and Paddle authorizes by API key):

- Commercial, $59 one-time, `tax_mode: external` so VAT is added on top of the $59:
  - sandbox `pri_01m2qzb07a4vvnqqy5w0m6atcf`, live `pri_01m2qzb5frfcey74rz1vr4ryx9` (both created 2026-09-17)
- Commercial subscription, the retired $59/**year**: sandbox `pri_01kf761pbd4afpaw32p1ryhk0p`, live
  `pri_01kjz6psgfsp1fan7j462ved55`. ❌ **Left ACTIVE on purpose**: a live subscription renews against it. Paddle does
  keep billing an existing subscription on an archived price, but there's no upside to testing that on real recurring
  revenue.
- Perpetual, the retired $199: `pri_01kf7649f72fyfvweeg7b5qwgg` sandbox (**archived**), `pri_01kjz6psnyp383dz9nbb7ba0zv`
  live (**still active, archive it after this change deploys**). Nothing was ever sold on it, so archiving strands no
  license.

❌ **Archive a price only after the page that links to it stops being served.** Archiving is not a soft retirement:
Paddle refuses the id outright, so a live page still carrying that button gets a checkout that can't open. Archiving the
live $199 on 2026-09-17 broke the then-current pricing page's perpetual button until it was set back to active (caught
by quoting the id against `POST /pricing-preview`, which is read-only and the cheap way to test this). The website's
`.env` on the VPS is baked in at build time, so the button survives until the next deploy, which means the safe order
is: deploy the new page first, then archive.

- No $39 renewal price exists yet: `/renew` is a `mailto:`, so there's nothing to attach one to. Create it with the
  renewal delivery path, ❌ as a one-time price and never a subscription (see the license-type note above).

### License server behavior

- `apps/api-server/src/licensing/license.ts`: `licenseTypes = ['commercial_subscription', 'commercial_perpetual']`.
- `apps/api-server/src/email/license.ts`: the per-type sentence in the fulfillment email.
- `apps/api-server/README.md` § first-time setup, and the secret list in `apps/api-server/wrangler.toml`.
- `apps/api-server/src/licensing/DETAILS.md`: the Paddle-fee comparison quotes a sample transaction amount.

### Desktop app

- `apps/desktop/src/lib/intl/messages/en/licensing.json` plus 12 other catalogs: the commercial-reminder copy quotes a
  price. Changing the `en` base string means re-translating every catalog and updating `licensing-i18n-parity.test.ts`,
  so it is never a one-line edit.
- `apps/desktop/src/lib/licensing/CLAUDE.md` and `apps/desktop/src-tauri/src/licensing/CLAUDE.md`: the license-type list
  documents prices and update windows.
- `apps/desktop/src/lib/licensing/DETAILS.md`: cites the reminder's price as the example of a literal that stays out of
  interpolation.
- `apps/desktop/src/lib/licensing/licensing-i18n-parity.test.ts` pins the `en` reminder string exactly, so the price
  can't drift without a failing test.

### Repo and off-site

- The repo-root README's "Commercial use" section.
- Off-site listings nothing in this repo can reach: AlternativeTo, any directory listing, the Listmonk newsletter
  description, Homebrew's cask description, and anywhere the old model was announced. Grep can't find these; the release
  checklist can't either.

## Changing a price

1. Grep for the old figures (`59`, `79`, `199`, `49`, `39`, `29`) plus `perpetual` and `subscription`, and walk the
   lists above. Also grep for `sales@getcmdr.com`: the organization block and the FAQ both carry it.
2. Create the new Paddle price in BOTH environments. ❌ Never edit an existing price's amount: a price id is the thing
   every past transaction points at.
3. Set `PUBLIC_PADDLE_PRICE_ID_COMMERCIAL` (in `/opt/cmdr/apps/website/.env` on the VPS) and
   `PRICE_ID_COMMERCIAL_PERPETUAL` (`wrangler secret put`) to the new id, **adding beside the old vars rather than
   replacing them**, so a deploy that fires before the merge still builds the page that's on `main`.
4. Refresh the `pricing-tiers.png` visual baseline (`apps/website/scripts/update-visual-baselines.sh`): the tier grid is
   one of the six committed region shots.
5. Run a sandbox purchase end to end: `docs/guides/test-purchase-flow.md`.
6. Merge, and let the deploy ship the new page.
7. **Only now** archive the retired prices, and only the ones with no live subscription against them (§ The live price
   IDs). Then delete the stale `.env` vars.

Terms, the personal-use grant, and what a buyer actually receives: `licensing.md`. Facts that constrain what the pricing
can be: `product-facts.md`.
