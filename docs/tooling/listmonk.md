# Listmonk (Cmdr-specific)

Self-hosted newsletter and mailing-list manager, at https://mail.getcmdr.com/. Generic access (the `agent` API user, the
`LISTMONK_API_KEY` in the sops secrets store, full cURL recipes) lives in the obsidian listmonk doc; the deployment,
Caddy, SMTP, and backups live in `infra/listmonk/README.md`. This note covers only the Cmdr list wiring.

## Lists

- **"Cmdr newsletter"** (id `3`): getcmdr.com's newsletter form.
- **"Cmdr beta testers"** (id `4`): the desktop app's beta contact email (Settings and onboarding). Kept separate from
  the newsletter because the two audiences and consent stories differ.

Both are **public** and **double opt-in**. Public is required, since the signup routes go through Listmonk's public
subscription endpoint, which refuses private lists; double opt-in is what stops a prank signup of someone else's
address. Their ids and UUIDs are wrangler `[vars]` on the api-server (`LISTMONK_{BETA,NEWSLETTER}_LIST_{ID,UUID}`).

## How a signup reaches Listmonk

Both surfaces POST only the email to the api-server: the app via the `beta_signup` Tauri command to `/beta-signup`, the
website form to `/newsletter-signup`. The Worker subscribes it through Listmonk's public endpoint and answers a soft 502
when the confirmation mail didn't go out. Call shape, outcomes, why it's the public endpoint and not the admin API, and
the Discord ping: `apps/api-server/src/website/DETAILS.md` § Signups.

The api-server's `LISTMONK_API_USER` / `LISTMONK_API_TOKEN` secrets are only for `GET /admin/funnel`'s per-day signup
counts; the signup routes need no credentials.

## The privacy invariant

The beta email is decoupled, contact-only. It never travels with any analytics or diagnostics install id, by
construction: `POST /beta-signup` reads only the email, and `listmonk-signup.test.ts` asserts the outbound bodies carry
no `anal_`/`diag_` id. Unsubscribing is via Listmonk's own link in any message we send; the desktop app only clears the
locally-stored copy.
