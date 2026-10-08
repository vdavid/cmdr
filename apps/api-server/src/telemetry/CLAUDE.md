# Telemetry

Everything the app sends home: `telemetry.ts` (`/crash-report`, `/download`, `/update-check`), `heartbeat.ts` (plus its
event relay), the `error-report*` quartet, and `feedback.ts`. File map: DETAILS § Files.

## Must-knows

- **The `anal_` analytics id and the `diag_` diagnostics id never co-occur on a request.** A crash report carries a
  `diag_` id only (an `anal_`-shaped `diagId` is a 400), feedback neither. That keeps analytics unjoinable to identity.
- **Optional fields from the Rust client arrive as `null` OR `undefined`** (serde `Option::None` → JSON `null`): a
  `!== undefined`-only validator silently drops the upgrade-window reports we want. Pattern:
  `value !== undefined && value !== null && <shape check>` (`validateCrashReportShape` is the canonical form).
- **`top_function` is the only crash-grouping key and must skip the panic machinery** (`extractTopFunction`), else every
  panic groups under `install_panic_hook` and the nightly email can't tell unrelated bugs apart.
- **`/feedback` and `/heartbeat` AWAIT their D1 writes** (failure → soft 502, the app retries); the rest are
  fire-and-forget. ❌ Don't flip either. A beat's 2xx lets the app clear its spool, so beat and events commit in ONE
  `batch`, and a malformed EVENT is dropped, never a 400 (it would block the spool forever). DETAILS § Heartbeat.
- **PostHog lives only in `posthog-forward.ts` plus one call**, run after the D1 commit (earlier double-counts retries).
- **Eviction spares bundles under `EVICTION_MIN_AGE_DAYS` (60) and is all-or-nothing** (it pauses intake rather than
  half-evicting, resuming at the LOW watermark). Drop either and unauthenticated `/error-report` becomes a delete
  primitive. DETAILS § Eviction.
- **Capped bodies go through `readCappedBody` (`../types.ts`), ❌ never `c.req.parseBody()` / `c.req.text()`**:
  `content-length` is advisory, so those buffer up to 100 MB in a 128 MB isolate before any cap looks.
- **Only hand-written error reports (`kind: 'user'`) are emailed**, from `postUploadWork`: one bad install makes dozens
  of auto-sends a day. `kind` is client-supplied, so the mail path caps itself daily. DETAILS § Notification email.
- **Those reports and every feedback message also become issues** in the private reports repo (`../github-issues.ts`),
  release builds only; an amendment comments on its card or files one. ❌ Never pass a note, reply-to, or bundle link
  into an issue title or body, and ❌ stamp an amendment `reportExpiryDate(entry.date)`, never a fresh 90 days.
  `apps/api-server/DETAILS.md` § The reports repo.
- **The `report:{id}` KV index write is AWAITED before the 200**, alone among this route's side effects: that same
  response hands out the amend credential, so a later index opens nothing. ❌ Never move it to `postUploadWork`; a
  failed put answers 200 with `amendKey: null`.
- **Only an amend key's SHA-256 is stored, and `ERR-XXXXX` is never proof of ownership** (31^5 values, shown to the
  user). Compare via `constantTimeEqual`. An `.amend.json` sidecar is evicted with its bundle, ❌ never on its own.
  DETAILS §§ The report index, Amendments.
- **The error-report id comes from the client and is used as-is** (validated against `^ERR-[23456789A-Z]{5}$`); on an R2
  key collision retry with a fresh UUID, ❌ never a fresh id — the user already read it in the preview dialog.
- **A download's UA family is computed at WRITE time** into `downloads.ua_family` (`../user-agent.ts`), outliving the
  raw `user_agent` the retention sweep clears at 90 days.
- **`/download` never stores a guessed version.** `latest` resolves through `latest.json` (GitHub as fallback); when
  neither answers it 302s and writes NO row, because a guess corrupts the per-version breakdown.
- **`sanitizeRef` (`[a-z0-9._:-]`) is a cross-repo contract** with the website's normalizer, and `sanitizeRefererHost`
  keeps the HOST only, so a referring page's query string can't leak.

Payloads, columns, R2 keys, eviction, intake, and email: `DETAILS.md`. Read it before any non-trivial work here:
editing, planning, reorganizing, or advising.
