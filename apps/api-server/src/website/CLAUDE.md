# Website endpoints

What getcmdr.com and the blog call: `beta-signup.ts` (`POST /beta-signup` → Listmonk), `likes.ts` (`/likes/:slug` blog
hearts in KV), and `link-codes.ts` (`GET /r-codes.json` plus the `/admin/r-codes` CRUD behind it).

## Must-knows

- **`/beta-signup` stays double-opt-in**: ❌ no `preconfirm_subscriptions` (Listmonk must send its own confirmation, or
  a prank signup subscribes someone else's address), and the 409 add-to-list path MUST call
  `POST /api/subscribers/{id}/optin` — the list-add endpoint does NOT send that mail on its own, so skipping it implies
  consent silently.
- **Every `/beta-signup` outcome returns an identical empty 204** (new, added, already subscribed), so the response
  can't be used to enumerate addresses. A Listmonk failure is the one exception: a soft 502, so the user knows it didn't
  land.
- **`/beta-signup` reads ONLY the email**: no `anal_`, no `diag_`, not in the request and not in the Discord ping. The
  email and the analytics ids never co-occur on our servers.
- **`/likes/:slug` validates the slug BEFORE any KV touch.** `POST` is unauthenticated and creates the key it writes, so
  the blog's charset plus an 80-char cap plus `LIKES_LIMITER` are the only bound on KV growth (and on the bill).
- **The likes pseudonym is salted with the post SLUG, ❌ never the daily salt telemetry uses**: it has to stay stable
  per reader for years. It still goes through the shared `hashCallerIp`, so the pepper does the one-way work. DETAILS §
  Likes.
- **`sanitizeUtmValue`'s charset (`[a-z0-9._-]`) is a cross-repo contract** with the blogs' client-side sanitizer and
  `/download`'s `ref` rule, so a stored value and a pass-through value normalize identically. Keep them in sync.
- **The whole `?r=` map lives under ONE KV key (`codes`)**, so `/r-codes.json` is a single KV get and a write is a
  read-modify-write of one value. The public response strips the admin-only `note`.

Listmonk call shapes, the 409 recovery, the likes KV model and its decisions, and the link-code CRUD: `DETAILS.md`. Read
it before any non-trivial work here: editing, planning, reorganizing, or advising.
