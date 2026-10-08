# S3 price table details

What the desktop app does with the table (schema, strict parsing, the fallback): `crates/cmdr-s3/DETAILS.md` § "Cost
estimates". This doc covers only how the Worker serves it.

## The response

- `200` with the table re-serialized compactly from the bundled JSON (`JSON.stringify` once per isolate). So the BODY
  isn't byte-identical to the file, only its parsed value is; the byte-identity guarantee is between the two source
  files.
- `Content-Type: application/json; charset=UTF-8`.
- `Cache-Control: public, max-age=3600`: the table changes a few times a year, and an hour's lag after a deploy is
  harmless for an estimate.
- `ETag` from Hono's `etag()` middleware (a SHA-1 of the body), so a client sending `If-None-Match` gets a bodyless
  `304`.

## Decisions

- **Decision: its own area.** It's app-facing reference data, which fits neither telemetry (nothing is recorded) nor
  website (the site never calls it), and areas can't import each other.
- **Decision: no rate limit.** The route reads no binding and writes nothing, so a flood costs only Worker invocations.
  `enforceIpRateLimit` gates intake routes, and this isn't one.
