# CSP allows `style-src 'unsafe-inline'`

**Severity:** low
**Lens:** F — Security
**Confidence:** high

## Location

`apps/desktop/src-tauri/tauri.conf.json` line 33

## What

The configured CSP for the prod build is:

```
default-src 'self'; script-src 'self'; style-src 'self' 'unsafe-inline'; img-src 'self' data:; font-src 'self'; connect-src 'self' https://getcmdr.com; frame-src 'none'; object-src 'none'; base-uri 'self'; form-action 'self'
```

Strong baseline (no inline scripts, no remote scripts, no frames, `connect-src` pinned to `self` + `getcmdr.com`). The one soft spot is `style-src 'self' 'unsafe-inline'`, which lets any inline `<style>` or `style=""` attribute run.

In Tauri-shipped apps with no XSS surface this is mostly cosmetic. Cmdr's only remote content path is the updater (which doesn't render HTML) and the AI streaming (text, rendered as text). But the file viewer renders file content. If a future viewer mode (markdown render, HTML preview) reflects untrusted file contents into the DOM without sanitization, `'unsafe-inline'` on styles is the difference between "weird-looking page" and "exfil via CSS background-image: url(...)".

## Why it matters

Defense-in-depth. Removing `'unsafe-inline'` from `style-src` is purely additive: it means a future XSS-style sink can't paint visible content the user mistakes for app chrome and can't exfil via CSS-loaded URLs. The cost is migrating Svelte's inline style attributes to CSS classes + variables (a fair amount of work, given how the app uses `style="--row-height: …"` patterns).

## Evidence

`<element style="...">` and `<style>` are common in the codebase; `apps/desktop/src/lib/file-explorer/pane/FilePane.svelte` heavily uses inline `style` for dynamic widths. Removing `'unsafe-inline'` requires migrating to `style.setProperty('--name', value)` calls so the values flow through CSS custom properties on a class-scoped sheet.

## Suggested fix

Not for pre-launch unless time permits. Track as a follow-up:

1. Audit inline `style=""` usage. Convert dynamic ones to `el.style.setProperty('--var', val)` + class-defined `style="…var(--var)…"` blocks (the class style is fine; the dynamic property write is fine too, since it's a JS API not a CSP target).
2. Replace `'unsafe-inline'` with a CSP nonce or `'self'` only. Tauri 2's CSP injection (`__TAURI_NONCE__`) can supply the nonce.
3. While editing, also re-evaluate `img-src 'self' data:` — `data:` URIs are how the app embeds icons, but they're a small exfil vector for future bugs. Probably worth keeping; just flag in the doc.

## Notes

- Dev mode is the other path to check; the wrapper's generated `tauri.instance.json` doesn't override CSP, so the canonical config governs. Verified.
- `withGlobalTauri: true` in the wrapper-generated dev config IS a known dev-only concession (documented in `apps/desktop/src-tauri/src/config.rs`). Prod is `false`. Not an issue.
