# Telemetry

Everything the app sends home: `telemetry.ts` (`/crash-report`, `/heartbeat`, `/download/:version/:arch`,
`/update-check/:version`), the `error-report*` quartet (`error-report.ts` routes, `error-report-amend.ts` the
`report:{id}` index and `/error-report/:id/amend`, `error-report-intake.ts` admission control,
`error-report-eviction.ts` capacity management), and `feedback.ts`.

## Must-knows

- **The `anal_` analytics id and the `diag_` diagnostics id never co-occur on a request.** A crash report carries a
  `diag_` id only (an `anal_`-shaped `diagId` is a 400), and feedback carries neither. That's what keeps the analytics
  stream unjoinable to an identity.
- **Optional fields from the Rust client arrive as `null` OR `undefined`** (serde `Option::None` → JSON `null`): a
  `!== undefined`-only validator silently drops exactly the upgrade-window reports we want. Pattern:
  `value !== undefined && value !== null && <shape check>` (`validateCrashReportShape` is the canonical form).
- **`top_function` is the only crash-grouping key and must skip the panic machinery** (`extractTopFunction`), else every
  panic groups under `install_panic_hook` and the nightly email can't tell unrelated bugs apart.
- **Only `/feedback` AWAITS its D1 write** (a failure returns a soft 502 so the app can retry); crash-report, heartbeat,
  download, and update-check are fire-and-forget `waitUntil`. ❌ Don't flip either.
- **Eviction spares bundles under `EVICTION_MIN_AGE_DAYS` (60) and is all-or-nothing** (it pauses intake instead of
  half-evicting, and resumes at the LOW watermark). Drop either property and unauthenticated `/error-report` becomes a
  delete primitive. DETAILS § Eviction.
- **Both error-report routes read bodies through `readCappedBody` (`../types.ts`), ❌ never `c.req.parseBody()` or
  `c.req.text()`**: `content-length` is advisory, so those buffer up to 100 MB inside a 128 MB isolate before any cap
  can look. A header pre-check is a fast-fail, never the cap.
- **Only hand-written error reports (`kind: 'user'`) are emailed**, straight from `postUploadWork`; auto-sends stay
  Discord-only, because one bad install can produce dozens a day. `kind` is client-supplied, so the mail path carries
  its own daily cap on its own KV key. DETAILS § Notification email.
- **The `report:{id}` KV index write is AWAITED before the 200**, alone among this route's side effects: the same
  response hands out the amend credential, so an index written later opens nothing. ❌ Never move it to
  `postUploadWork`; a failed put answers 200 with `amendKey: null`.
- **Only an amend key's SHA-256 is stored, and `ERR-XXXXX` is never proof of ownership** (31^5 values, shown to the
  user). Compare via `constantTimeEqual`. An `.amend.json` sidecar is evicted with its bundle, ❌ never on its own.
  DETAILS §§ The report index, Amendments.
- **The error-report id comes from the client and is used as-is** (validated against `^ERR-[23456789A-Z]{5}$`); on an R2
  key collision retry with a fresh UUID, ❌ never a fresh id — the user already read it in the preview dialog.
- **A download's UA family is computed at WRITE time** into `downloads.ua_family` (`../user-agent.ts`), so the signal
  outlives the raw `user_agent` the retention sweep clears at 90 days.
- **`/download` never stores a guessed version.** `latest` resolves through `latest.json` (GitHub as fallback); when
  neither answers it 302s and writes NO row, because a guess corrupts the per-version breakdown.
- **`sanitizeRef` (`[a-z0-9._:-]`) is a cross-repo contract** with the website's normalizer, and `sanitizeRefererHost`
  keeps the HOST only, so a referring page's query string can't leak in.

Per-route payloads and columns, the R2 key shape, eviction and intake admission, the notification email, and the
UA-family model: `DETAILS.md`. Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
