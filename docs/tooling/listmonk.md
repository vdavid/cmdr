# Listmonk (Cmdr-specific)

Self-hosted newsletter and mailing-list manager. The Cmdr beta-tester contact channel runs on it. Generic access (the
`agent` API user, the `LISTMONK_API_KEY` in macOS Keychain, full cURL recipes) lives in the obsidian listmonk doc; this
note documents only the Cmdr beta-list wiring.

- **Instance**: https://mail.getcmdr.com/
- **Beta-tester list**: a dedicated **double-opt-in** list "Cmdr beta testers", separate from the public "Cmdr
  newsletter" list. Created at deploy time; the numeric list id is recorded then and set as the `LISTMONK_BETA_LIST_ID`
  wrangler secret on the api-server. (Don't reuse the newsletter list id `3`: the two audiences and consent stories are
  different.)

## How the beta signup reaches Listmonk

The desktop Settings email field (and the onboarding email field) calls the `beta_signup` Tauri command, which POSTs the
email (and ONLY the email, never an install id) to the api-server's `POST /beta-signup`. The Worker forwards it to
Listmonk:

```
POST https://mail.getcmdr.com/api/subscribers
Authorization: token <LISTMONK_API_USER>:<LISTMONK_API_TOKEN>
{ "email": "<addr>", "lists": [<LISTMONK_BETA_LIST_ID>], "status": "enabled" }
```

- **Subscriber `status: "enabled"` with NO `preconfirm_subscriptions`** makes it double opt-in: Listmonk sends its own
  confirmation email, so a prankster can't subscribe someone else's address. (`"enabled"` is the subscriber-status enum,
  which only accepts enabled/disabled/blocklisted; `"unconfirmed"` is the per-LIST subscription status, set implicitly
  by omitting `preconfirm_subscriptions`. Contrast the obsidian doc's newsletter recipe, which passes
  `preconfirm_subscriptions: true` for a confirmed add. The beta flow deliberately does not.)
- **No enumeration**: the Worker returns the same empty 204 for a fresh subscribe, an already-subscribed address, and
  the 409 add-to-list path below, so the response never reveals whether the address was already on the list.

## 409 add-to-list recovery and the signup notification

A Listmonk 409 ("subscriber already exists" — for example the address is already on the public newsletter list) used to
map straight to 204, which left that person OFF the beta list. The Worker now recovers: on 409 it looks the subscriber
up (`GET /api/subscribers?query=subscribers.email='<addr>'`), and if they're not yet on the beta list it adds it
(`PUT /api/subscribers/lists` with `action: "add"`, `status: "unconfirmed"`, `target_list_ids: [<beta list>]`) and then
explicitly sends the opt-in confirmation email (`POST /api/subscribers/{id}/optin`). That optin call is required: the
list-add endpoint does NOT send the confirmation email on its own, so without it consent would be silently implied. A
subscriber already on the beta list is a quiet re-signup: no list change and no email.

Each newly-established subscription (a fresh signup, or the 409 add-to-list path) pings a Discord channel so the
maintainer sees signups in real time (`DISCORD_BETA_SIGNUP_WEBHOOK_URL`, falling back to `DISCORD_WEBHOOK_URL`). The
ping carries the email and the signup time only, never an install id. A Listmonk failure and a plain already-on-list 409
do not ping.

## Worker secrets (api-server)

Set these as wrangler secrets on `cmdr-license-server` (see `apps/api-server/DETAILS.md` § Configuration):

- `LISTMONK_API_URL` (for example `https://mail.getcmdr.com`)
- `LISTMONK_API_USER` and `LISTMONK_API_TOKEN` (sent as `token <user>:<token>`). At deploy a dedicated least-privilege
  Listmonk API user/token is preferred over the broad `agent` superadmin token.
- `LISTMONK_BETA_LIST_ID` (the numeric id of the "Cmdr beta testers" list)

## The privacy invariant

The beta email is decoupled, contact-only. It never travels with any analytics or diagnostics install id, by
construction: `POST /beta-signup` reads only the email, and `beta-signup.test.ts` asserts the inbound and outbound
bodies carry no `anal_`/`diag_` id. Unsubscribing is via Listmonk's own link in any message we send; the desktop app
only clears the locally-stored copy.
