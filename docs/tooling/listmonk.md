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
{ "email": "<addr>", "lists": [<LISTMONK_BETA_LIST_ID>], "status": "unconfirmed" }
```

- **`status: "unconfirmed"` with NO `preconfirm_subscriptions`** makes it double opt-in: Listmonk sends its own
  confirmation email, so a prankster can't subscribe someone else's address. (Contrast the obsidian doc's newsletter
  recipe, which passes `preconfirm_subscriptions: true` for a confirmed add. The beta flow deliberately does not.)
- **No enumeration**: the Worker maps a Listmonk 409 ("already exists") to the same empty 204 as a fresh subscribe, so
  the response never reveals whether the address was already on the list.

## Worker secrets (api-server)

Set these as wrangler secrets on `cmdr-license-server` (see `apps/api-server/CLAUDE.md` § Configuration):

- `LISTMONK_API_URL` (for example `https://mail.getcmdr.com`)
- `LISTMONK_API_USER` and `LISTMONK_API_TOKEN` (sent as `token <user>:<token>`). At deploy a dedicated least-privilege
  Listmonk API user/token is preferred over the broad `agent` superadmin token.
- `LISTMONK_BETA_LIST_ID` (the numeric id of the "Cmdr beta testers" list)

## The privacy invariant

The beta email is decoupled, contact-only. It never travels with any analytics or diagnostics install id, by
construction: `POST /beta-signup` reads only the email, and `beta-signup.test.ts` asserts the inbound and outbound
bodies carry no `anal_`/`diag_` id. Unsubscribing is via Listmonk's own link in any message we send; the desktop app
only clears the locally-stored copy.
