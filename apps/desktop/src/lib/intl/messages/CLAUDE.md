# Message catalogs

JSON message catalogs, one file per feature area, under `en/`. The base locale is English-only; the runtime
(`$lib/intl/messages.svelte.ts`) merges every `en/*.json` into one map at load. Runtime design and the error-pipeline
boundary: [`../CLAUDE.md`](../CLAUDE.md).

## Layout

- `en/<area>.json`: messages for one area. The key prefix maps 1:1 to the filename (`settings.fsWatch.title` →
  `settings.json`), so an agent editing one feature touches one file. `common.json` holds truly shared strings.
- `screenshots/`: capture artifacts referenced by `@key.screenshot`; one file serves many keys. PNGs are **gitignored**
  and regenerable; `capture-report.json` + `coverage-report.md` are tracked. Don't hand-edit `@key.screenshot` or commit
  PNGs — regenerate with `pnpm i18n:shots`. See [DETAILS.md](DETAILS.md) § Screenshots.

## Must-knows

- **Key shape: `area.feature.leaf`** — lowerCamel segments, dot-separated, at least two, first segment a known area.
  Enforced by `desktop-message-key-naming`. Add an area only by adding both a catalog file AND the area to that check's
  allowlist.
- **Double every apostrophe (`''`).** ICU treats `'` as an escape char; a lone `'` before `{`/`<`/`#` opens a quoted
  section and swallows text. `''` always collapses to `'` and is always safe, so it's the rule everywhere, even where a
  lone `'` would happen to render fine.
- **Embed counts as preformatted `*Text` STRING params, not ICU `{n, number}`.** Formatting is single-sourced in
  `$lib/intl`. Pass the raw integer alongside ONLY to drive `plural` selection (noun, was/were). See `transfer.json`.
- **`@key` metadata is ARB-style sibling entries** (`@transfer.trash`), holding `description` + a `placeholders` map +
  optional `screenshot`. The runtime and codegen strip every `@`-prefixed entry, so it never reaches `format()`. Keep a
  `@key` twin in sync when you rename a key. **Write the `description` to set a translator up for excellence**
  (surface + trigger + constraints + do-not-translate tokens; plain-language placeholder meanings via `placeholders`; NO
  ICU plumbing, NO tone — tone lives in the per-language style guide). Full guidance + the litmus test:
  [DETAILS.md](DETAILS.md) § `@key` metadata schema. Every migrated key SHOULD carry a `description` (and `placeholders`
  if it has any).
- **Never hand-edit `../keys.gen.ts`.** It's generated from these files by `pnpm intl:keys`; run that after any key
  add/remove/rename. The `desktop-message-keys-fresh` check fails if it's stale.
- **A new key needs a real call site, or it fails `desktop-message-keys-unused`.** A catalog key never referenced in
  `apps/desktop/src/` is an orphan (dead translation work) and is an ERROR, not just the codegen's dead-key warning.
  Keys built at runtime are carried by the closed dynamic-prefix allowlist in that check; don't add a key with no call
  site expecting the allowlist to cover it. See [DETAILS.md](DETAILS.md) § Dead-key honesty + the orphan check.

Depth (the `@key` schema, screenshots-by-filename, the dead-key honesty caveat, parity rules): [DETAILS.md](DETAILS.md).
