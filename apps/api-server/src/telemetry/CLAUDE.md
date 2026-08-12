# Telemetry

Everything the app sends home: `telemetry.ts` (`/crash-report`, `/heartbeat`, `/download/:version/:arch`,
`/update-check/:version`), the `error-report*` trio (`error-report.ts` routes, `error-report-intake.ts` admission
control, `error-report-eviction.ts` capacity management), and `feedback.ts`.

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
- **`/error-report` reads bodies through `readCappedBody`, ❌ never `c.req.parseBody()`**: `content-length` is advisory,
  so the parser would buffer up to 100 MB inside a 128 MB isolate.
- **The error-report id comes from the client and is used as-is** (validated against `^ERR-[23456789A-Z]{5}$`); on an R2
  key collision retry with a fresh UUID, ❌ never a fresh id — the user already read it in the preview dialog.
- **A download's UA family is computed at WRITE time** into `downloads.ua_family` (`../user-agent.ts`), so the signal
  outlives the raw `user_agent` the retention sweep clears at 90 days.
- **`/download` never stores a guessed version.** `latest` resolves through `latest.json` (falling back to GitHub); when
  neither answers it 302s to the releases page and writes NO row, because a guess corrupts the per-version breakdown.
- **`sanitizeRef` (`[a-z0-9._:-]`) is a cross-repo contract** with the website's client-side normalizer, and
  `sanitizeRefererHost` keeps the HOST only, so a referring page's query string can't leak in.

Per-route payloads and columns, the R2 key shape, eviction and intake admission, and the UA-family model: `DETAILS.md`.
Read it before any non-trivial work here: editing, planning, reorganizing, or advising.
