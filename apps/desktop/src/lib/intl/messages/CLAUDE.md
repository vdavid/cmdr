# Message catalogs

JSON message catalogs, one file per feature area, under `en/` plus one dir per translated locale. The runtime
(`$lib/intl/messages.svelte.ts`) merges every `en/*.json` into one map at load. Runtime design and the error-pipeline
boundary: `../CLAUDE.md`.

## Layout

- `en/<area>.json`: messages for one area. The key prefix maps 1:1 to the filename (`settings.fsWatch.title` →
  `settings.json`), so an agent editing one feature touches one file. `common.json` holds truly shared strings.
- `screenshots/`: capture artifacts referenced by `@key.screenshot` (`@key.screenshotNote` for stand-ins); one file
  serves many keys. PNGs are **gitignored**; `capture-report.json` + `coverage-report.md` are tracked. Don't hand-edit
  those `@key` fields or commit PNGs; regenerate with `pnpm i18n:shots`. `DETAILS.md` § Screenshots.
- `en-XA/`: the generated **pseudolocale** (accented, expanded, structure-preserving) for overflow testing.
  **Gitignored, regenerable** with `pnpm i18n:pseudo`; never hand-edit or hand-justify it (it auto-emits
  `sameAsSourceJustification`). Committed fixture: `test/fixtures/i18n-pseudolocale/`. `docs/guides/i18n.md` §
  Pseudolocale.
- `<lang>-<REGION>/` (`en-GB`, `pt-PT`): an **overlay**. It holds ONLY the keys that differ from its language base;
  everything else resolves through the fallback chain. A key identical to what it overrides fails `i18n-coverage`
  (delete it), and `sameAsSourceJustification` doesn't apply. Rules: `docs/guides/i18n.md` § Overlay catalogs.

## Must-knows

- **Key shape: `area.feature.leaf`**: lowerCamel segments, dot-separated, at least two, first segment a known area.
  Enforced by `desktop-message-key-naming`. Add an area only by adding both a catalog file AND the area there.
- **Double every apostrophe (`''`).** ICU treats `'` as an escape char; a lone `'` before `{`/`<`/`#` opens a quoted
  section and swallows text. `''` always collapses to `'`, so double it everywhere, even where a lone `'` would render
  fine.
- **The RAW families never meet ICU**, so their apostrophes stay SINGLE and their `{token}`s are literal replacement
  targets: `errors.*`, plus the NATIVE ones Rust draws (`menu.*`, `licensing.windowTitle.*`, `main.instanceLock.*`)
  through `menu_t`, never `t()`. No capture can photograph a native surface, so its `@key` description is the whole
  translator aid: which menu, what it does, VERB or NOUN. `isRawKey()` is the single source; `DETAILS.md`.
- **A `<tag>` name must never equal a param name in the same message.** `Trans.svelte` merges the tag snippets into the
  interpolation params, so a shared name resolves to the tag handler and the sentence renders a stringified function
  instead of the value. Name the tag for its role, the param for its content: `<processName>{process}</processName>`.
  Enforced by `i18n-tag-param-collision` (ERROR) across every locale: nothing else catches it, since the message is
  valid ICU and renders without throwing.
- **Embed counts as preformatted `*Text` STRING params, not ICU `{n, number}`.** Formatting is single-sourced in
  `$lib/intl`. Pass the raw integer alongside ONLY to drive `plural` selection. See `transfer.json`.
- **`@key` metadata is ARB-style sibling entries** (`@transfer.trash`), holding `description` + a `placeholders` map +
  optional `screenshot`. The runtime and codegen strip every `@`-prefixed entry, so it never reaches `format()`. Keep
  the twin in sync on a rename. **Write the `description` to set a translator up for excellence** (surface + trigger +
  constraints + do-not-translate tokens; plain-language placeholder meanings via `placeholders`; NO ICU plumbing, NO
  tone, which lives in the per-language style guide). Every key SHOULD carry one. Litmus test: `DETAILS.md` § `@key`
  metadata schema.
- **Never hand-edit `../keys.gen.ts`.** It's generated from these files by `pnpm intl:keys`; run that after any key
  add/remove/rename. The `desktop-message-keys-fresh` check fails if it's stale.
- **A new key needs a real call site, or it fails `desktop-message-keys-unused`.** A catalog key referenced in neither
  `apps/desktop/src/` nor `src-tauri/src/` is an orphan (dead translation work) and an ERROR, not just the codegen's
  dead-key warning. Runtime-built keys are carried by that check's closed dynamic-prefix allowlist. `DETAILS.md` §
  Dead-key honesty.

Depth (the `@key` schema, screenshots-by-filename, the dead-key honesty caveat, parity rules): `DETAILS.md`.
