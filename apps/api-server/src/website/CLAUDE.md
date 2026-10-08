# Website endpoints

What getcmdr.com, the blog, and the app call: `beta-signup.ts` (`POST /beta-signup`, the app) and `newsletter-signup.ts`
(`POST /newsletter-signup`, getcmdr.com's form), both thin over `listmonk-signup.ts`; `likes.ts` (`/likes/:slug` blog
hearts in KV), `link-codes.ts` (`GET /r-codes.json` plus the `/admin/r-codes` CRUD behind it), and `csp-report.ts`
(`POST /csp-report`, the site's CSP violations, alerted to Discord).

## Must-knows

- **Signups go through Listmonk's PUBLIC endpoint (`/api/public/subscription`), ❌ never the admin
  `POST /api/subscribers`**: the admin one answers 200 when the opt-in mail fails to send, which hid a dead SMTP
  credential for two months. The public one fails loudly, handles existing subscribers, and can't preconfirm, so double
  opt-in holds. Both lists must stay `public` in Listmonk.
- **Every signup outcome returns an identical empty 204** (new, re-sent, already confirmed), so the response can't
  enumerate addresses. The exceptions carry no such signal: 400 (bad address), 429, and a soft 502 when the confirmation
  mail didn't go out.
- **`/beta-signup` reads ONLY the email**: no `anal_`, no `diag_`, not in the request and not in the Discord ping. The
  email and the analytics ids never co-occur on our servers. No Discord ping carries an email.
- **`/likes/:slug` validates the slug BEFORE any KV touch.** `POST` is unauthenticated and creates the key it writes, so
  the blog's charset plus an 80-char cap plus `LIKES_LIMITER` are the only bound on KV growth (and on the bill).
- **The likes pseudonym is salted with the post SLUG, ❌ never the daily salt telemetry uses**: it has to stay stable
  per reader for years. It still goes through the shared `hashCallerIp`, so the pepper does the one-way work. DETAILS §
  Likes.
- **`sanitizeUtmValue`'s charset (`[a-z0-9._-]`) is a cross-repo contract** with the blogs' client-side sanitizer and
  `/download`'s `ref` rule, so a stored value and a pass-through value normalize identically. Keep them in sync.
- **The whole `?r=` map lives under ONE KV key (`codes`)**, so `/r-codes.json` is a single KV get and a write is a
  read-modify-write of one value. The public response strips the admin-only `note`.

The Listmonk call shape and its outcomes, the likes KV model and its decisions, and the link-code CRUD: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
