# Newsletter signup plan

## Context

Cmdr doesn't have a released product yet. People interested in the project have no way to get notified about releases.
We need a newsletter signup so enthusiasts can follow the project. This is *not* a marketing blast tool — it's about
getting information to people who genuinely want it.

**Stack decision**: Listmonk (self-hosted on VPS) + AWS SES (SMTP relay). Rationale: free, full data ownership (
Postgres), no "marketing email" pricing premium, user already has AWS account and a VPS with Docker + daily NAS backups.

## Architecture

```
getcmdr.com (Astro static site)
  └─ signup form POSTs to /api/newsletter/subscribe (same origin)
        └─ Caddy proxies → listmonk:9000/api/public/subscription (Docker network)
              └─ Listmonk stores subscriber in Postgres
              └─ Sends double opt-in confirmation via AWS SES SMTP

mail.getcmdr.com (Listmonk admin UI)
  └─ Caddy proxies → listmonk:9000

SES bounce/complaint notifications:
  SES → SNS → https://getcmdr.com/webhooks/ses → Caddy → listmonk:9000/webhooks/service/ses
```

**Key design decisions:**

- **No CORS needed**: Caddy proxies `/api/newsletter/subscribe` on the main domain to Listmonk, so forms POST to same
  origin
- **No host port mapping for Listmonk**: Caddy and Listmonk are both on `proxy-net` Docker network — Caddy reaches
  Listmonk by container name. The webhook listener's port 9000 on the host is unaffected
- **Separate docker-compose**: Listmonk has persistent state (Postgres) and different lifecycle than the website. Lives
  in `infra/listmonk/`
- **Double opt-in**: Listmonk sends a confirmation email. Subscription only activates after clicking the link
- **Honeypot field**: Hidden form field for basic bot protection (bots fill it, humans don't)

## Signup form placement (3 locations)

1. **Header**: "Newsletter" nav item → click slides open a panel below the header bar with email field, "Sign up"
   button, and friendly text
2. **Footer**: Full-width newsletter row between the link columns and the copyright bar
3. **Download section**: Replace "Windows and Linux coming soon. Star on GitHub to get notified." with an inline
   newsletter signup form

All three use the same shared `NewsletterForm.astro` component with `variant` prop (`inline` | `stacked`).

## Files to create

| File                                               | Purpose                                               |
|----------------------------------------------------|-------------------------------------------------------|
| `infra/listmonk/docker-compose.yml`                | Listmonk + Postgres containers                        |
| `infra/listmonk/.env.example`                      | Template for DB password, admin creds                 |
| `infra/listmonk/README.md`                         | Setup, maintenance, backup, and troubleshooting guide |
| `apps/website/src/components/NewsletterForm.astro` | Shared signup form component                          |
| `apps/website/e2e/newsletter.spec.ts`              | Playwright e2e tests                                  |

## Files to modify

| File                                          | Change                                         |
|-----------------------------------------------|------------------------------------------------|
| `apps/website/src/components/Header.astro`    | Add "Newsletter" nav item + slide-open panel   |
| `apps/website/src/components/Footer.astro`    | Add newsletter signup row                      |
| `apps/website/src/components/Download.astro`  | Replace "Star on GitHub" CTA with signup form  |
| `apps/website/src/pages/privacy-policy.astro` | Add Listmonk + AWS SES to data processors list |
| `apps/website/.env.example`                   | Add `PUBLIC_LISTMONK_LIST_UUID`                |

## Implementation details

### NewsletterForm.astro (shared component)

Props: `variant` (`'inline'` | `'stacked'`), optional `message` string, optional `class`.

Structure:

- `<form>` with `data-list-uuid` attribute (from `PUBLIC_LISTMONK_LIST_UUID` env var)
- Accessible: `<label class="sr-only">` for each input, unique `id` per variant to avoid collisions
- Email `<input>` with placeholder `name@example.com` (per style guide)
- "Sign up" button
- Hidden honeypot `<input>` (visually hidden, `tabindex="-1"`, `autocomplete="off"`)
- `<div aria-live="polite">` for feedback messages

Client-side `<script>`:

- Wrapped in `document.addEventListener('astro:page-load', ...)` to survive view transitions (Astro uses `ClientRouter`)
- Client-side email validation before submit
- Skips honeypot-filled submissions silently
- `POST /api/newsletter/subscribe` with `{ email, name: '', list_uuids: [uuid] }`
- Shows "Check your inbox to confirm your subscription!" on success (accent color)
- Shows error messages on failure (warning color)
- Loading state: button text changes to "Signing up...", disabled during request

### Header.astro changes

- Add `{ href: '#newsletter', label: 'Newsletter' }` to nav links, rendered as a `<button>` (not `<a>`, since it toggles
  UI rather than navigating)
- Add collapsible panel below `<nav>`:
  ```html
  <div id="newsletter-panel" aria-hidden="true" class="newsletter-panel">
      <p>How cool that you want to hear about Cmdr news! We promise we won't spam you. ❤️</p>
      <NewsletterForm variant="inline" />
  </div>
  ```
- CSS: `max-height: 0` → `max-height: 200px` transition with `--ease-out-expo` timing, respecting
  `prefers-reduced-motion`
- Script: toggle `aria-hidden`, focus the email input on open, close on Escape key

### Footer.astro changes

Insert a full-width newsletter row between the existing link columns grid and the bottom copyright bar:

```html

<div class="newsletter row: flex-col md:flex-row, left side has heading + description, right side has inline form">
    <div>
        <p class="font-semibold">Stay in the loop</p>
        <p class="text-sm secondary">Product updates and Cmdr news. No spam, ever.</p>
    </div>
    <NewsletterForm variant="inline" />
</div>
```

### Download.astro changes

Replace lines 69–78 (the "Other platforms" `<p>` with GitHub star link) with:

```html

<div class="mt-8 text-center">
    <p class="mb-3 tertiary">Windows and Linux coming soon. Get notified when they're ready:</p>
    <div class="mx-auto max-w-sm">
        <NewsletterForm variant="inline" />
    </div>
</div>
```

### Privacy policy update

- Section 5 (data processors): add entries for Listmonk (self-hosted, stores email + subscription status on our server)
  and AWS SES (email delivery, link to AWS privacy policy)
- Update `lastUpdated` date

### Listmonk docker-compose

```yaml
services:
  listmonk-db:
    image: postgres:17-alpine
    container_name: listmonk-db
    restart: unless-stopped
    environment:
      POSTGRES_DB: listmonk
      POSTGRES_USER: listmonk
      POSTGRES_PASSWORD: ${LISTMONK_DB_PASSWORD}
    volumes:
      - listmonk-data:/var/lib/postgresql/data
    networks:
      - listmonk-internal

  listmonk:
    image: listmonk/listmonk:latest
    container_name: listmonk
    restart: unless-stopped
    depends_on:
      - listmonk-db
    command: >
      sh -c "./listmonk
      --db.host=listmonk-db --db.port=5432
      --db.user=listmonk --db.password=$${LISTMONK_DB_PASSWORD} --db.database=listmonk
      --app.address=0.0.0.0:9000"
    environment:
      - TZ=Europe/Stockholm
      - LISTMONK_DB_PASSWORD=${LISTMONK_DB_PASSWORD}
    networks:
      - listmonk-internal
      - proxy-net

volumes:
  listmonk-data:

networks:
  listmonk-internal:
  proxy-net:
    external: true
```

Postgres isolated on `listmonk-internal`. Listmonk bridges both networks. No host ports exposed.

### Caddy config changes (manual on VPS)

```caddy
mail.getcmdr.com {
    reverse_proxy listmonk:9000
}

getcmdr.com {
    # Newsletter subscription API (proxied to Listmonk)
    handle /api/newsletter/subscribe {
        rewrite * /api/public/subscription
        reverse_proxy listmonk:9000
    }

    # SES bounce webhook (proxied to Listmonk)
    handle /webhooks/ses {
        rewrite * /webhooks/service/ses
        reverse_proxy listmonk:9000
    }

    # Existing rules unchanged
    handle /hooks/* {
        reverse_proxy localhost:9000
    }
    handle {
        reverse_proxy getcmdr-static:80
    }
}
```

## Task list

### Milestone 1: Infrastructure files

- [x] Create `infra/listmonk/docker-compose.yml`
- [x] Create `infra/listmonk/.env.example`
- [x] Create `infra/listmonk/README.md` (setup, SES config, Caddy config, maintenance, backups, troubleshooting)
- [x] Add `PUBLIC_LISTMONK_LIST_UUID` to `apps/website/.env.example`

### Milestone 2: Shared signup component

- [x] Create `NewsletterForm.astro` with inline/stacked variants
- [x] Include client-side JS with `astro:page-load` handling, validation, honeypot, aria-live feedback

### Milestone 3: Website integration

- [x] Modify `Header.astro`: "Newsletter" toggle button + slide-open panel
- [x] Modify `Footer.astro`: full-width newsletter signup row
- [x] Modify `Download.astro`: replace "Star on GitHub" with newsletter form
- [x] Update `privacy-policy.astro`: add Listmonk + AWS SES, update date

### Milestone 4: Testing

- [x] Create `e2e/newsletter.spec.ts` (form visibility, panel toggle, client-side validation, accessibility)
- [ ] Run
  `./scripts/check.sh --check website-prettier,website-eslint,website-typecheck,website-build,website-e2e`

### Milestone 5: Server deployment (manual, one-time, documented in README)

- [ ] Add DNS: `mail.getcmdr.com` A record → VPS IP
- [ ] AWS SES: verify domain, add DKIM CNAMEs, request production access, create SMTP credentials
- [ ] AWS SNS: create topic, subscribe to `https://getcmdr.com/webhooks/ses`
- [ ] SES: configure bounce + complaint feedback → SNS topic
- [ ] Create `/opt/cmdr/infra/listmonk/.env` with real secrets on VPS
- [ ] `docker compose up -d` + `docker exec -it listmonk ./listmonk --install`
- [ ] Update Caddyfile, reload Caddy
- [ ] Configure Listmonk: SMTP (SES), create mailing list, configure double opt-in template, set "from" address
- [ ] Note list UUID, add to website `.env`, redeploy website
- [ ] End-to-end test: signup → confirmation email → click confirm → verified in Listmonk admin

## Verification

After all code changes, run:
`./scripts/check.sh --check website-prettier,website-eslint,website-typecheck,website-build,website-e2e`

Manual testing after server deployment:

1. Submit each of the 3 forms with a valid email → confirm Listmonk receives subscription
2. Verify double opt-in confirmation email arrives, click confirm, verify in Listmonk admin
3. Submit invalid email → client-side validation message shown
4. Submit duplicate email → appropriate server message shown
5. Header panel: click to open, Escape to close, Tab to email field, Enter to submit
6. Screen reader (VoiceOver): verify feedback is announced via `aria-live`
7. Responsive: check all 3 forms at mobile/tablet/desktop widths
8. `prefers-reduced-motion`: verify header panel transition respects it
