# Product facts with business consequences

Code facts that change the answer to a business question. Each one has been gotten wrong by an agent reasoning from
assumptions, and each one moved a recommendation when corrected. Keep this list short and current.

## AI is bring-your-own-key

Every cloud provider preset in `apps/desktop/src/lib/settings/cloud-providers.ts` sets `requiresApiKey: true`. Ollama
covers local models, and image search runs an on-device CLIP model with no provider at all.

**We pay nothing for inference; the user brings and funds their own key.** Consequences:

- A perpetual license with unlimited AI carries no growing cost liability for us. Bundled inference would.
- Equally, "AI has to be paid to keep this sustainable" is a weaker story than it would be with inference on us. A
  paid AI tier sells the integration, not the tokens.
- AI adoption sits around 12% of installs. That is partly setup friction (sign up with a provider, create a key, paste
  it), so read it as a friction number as much as a value verdict.

## Commercial use is encouraged in-app, not left to conscience

Cmdr is free for personal use and requires a license for commercial use. That is not a pure honour system:

- **"Personal use only" is always in the main window title.** Awkward to screen-share in a work meeting, and visible
  to an IT department.
- **A monthly reminder shows for all unlicensed users.**
- The app indexes the user's files, so it can escalate the reminder when it recognises likely work-use patterns.
  **Not implemented yet**, and it is the largest remaining lever on compliance. Agentic feature usage would make that
  signal stronger still.

Model compliance meaningfully above the rate an unenforced honour system would get.

## Onboarding exists and is decent

There is a real first-run flow, including an optional "Stay in touch" email field that feeds Listmonk. Do not describe
Cmdr as having no onboarding. Plans and history: the vault's `Cmdr onboarding experience plans.md`.

## Analytics is opt-out and on by default

For the beta, `analytics.enabled` defaults to on. Install counts and active-user numbers are therefore close to true
rather than a heavy undercount, which is unusual and makes the funnel trustworthy.

## Distribution and payments

- macOS only today. Signed and notarized, distributed as a direct DMG download plus Homebrew, with a self-hosted
  updater that preserves Full Disk Access across updates.
- Paddle is the merchant of record, so it handles VAT and sales tax. EU business buyers with a VAT number are
  reverse-charged; individuals buying personally pay VAT-inclusive.
- Source-available under BSL 1.1, converting to AGPL-3.0 after three years. See `licensing.md`.

## Two different metrics are both called "new installs"

They differ by roughly 10x. The Daily funnel's `installs` column counts first-ever heartbeats and is the real number;
the Download section's "New installs (deduped)" counts deduplicated download requests and is inflated by bots and
mirrors. Details: `docs/tooling/analytics-dashboard.md`.
