# Updater accepts arbitrary URL from frontend; integrity protected only by minisign

**Severity:** medium
**Lens:** F — Security
**Confidence:** high

## Location

`apps/desktop/src-tauri/src/updater/mod.rs::download_update` (lines 114–153)

## What

The `download_update` Tauri command signature is:

```rust
pub async fn download_update(url: String, signature: String, state: State<'_, UpdateState>) -> Result<(), String>
```

Both `url` and `signature` come from the frontend; the backend doesn't cross-check that the URL matches what `check_for_update` returned, doesn't validate the scheme is `https://`, and doesn't constrain the host to `getcmdr.com` / `api.getcmdr.com`. The integrity contract leans entirely on minisign verifying the response bytes against the compiled-in pubkey before the tarball is written to disk.

That's a real defense — an attacker would need our minisign private key to install a malicious update. But:

- `reqwest::get(url)` will happily fetch `http://10.0.0.5/payload` or `https://evil.example.com/payload`. If an attacker can briefly poison the response (e.g., compromise the bundle host, then revert), they get one shot at sneaking past signature verification via any minisign quirk (e.g., a hash collision, a future minisign-verify CVE, or a base64-decoding side channel).
- A compromised renderer (XSS via a future CSP relaxation, a malicious settings dialog, or a malicious viewer page that gains script execution) could call `download_update` with an attacker-controlled URL and exfiltrate the user's egress traffic destination via DNS / TLS SNI. Minor info-disclosure but still avoidable.
- The download has no size cap. `bytes()` reads the whole response into RAM. A hostile server can hand back gigabytes and OOM the renderer.

## Why it matters

Defense-in-depth. The signature is load-bearing; one weak link in the URL contract puts it under more load than necessary. Allowed-host pinning costs nothing and means the only attack path is "compromise our R2 / `getcmdr.com` host AND the minisign key." Either alone shouldn't be sufficient.

## Evidence

```rust
pub async fn download_update(url: String, signature: String, ...) -> Result<(), String> {
    log::info!("Downloading update from {url}");
    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_READ_TIMEOUT)
        .build()
        ...;
    let response = client.get(&url).send().await?;
    let bytes = response.bytes().await?;  // unbounded read into memory
    signature::verify(&bytes, &signature)?;
    ...
}
```

No allowed-host check, no scheme check, no Content-Length cap, no streaming-to-disk.

## Suggested fix

1. Require the URL to start with `https://` and match one of a small allowlist (e.g., `cmdr-updates.r2.cloudflarestorage.com`, `getcmdr.com`, `api.getcmdr.com` — whatever the prod release pipeline actually uses). Reject otherwise. Easy to add the constants next to the `MANIFEST_*_TIMEOUT` block.
2. Use a `Content-Length` check + streaming to a temp file with a hard cap (say 200 MB) instead of `response.bytes()`. Verify signature against the on-disk file (minisign-verify accepts byte slices fine, just `mmap` or read the file).
3. Optionally: have `check_for_update` stash the manifest's `url` + `signature` in `UpdateState` and let `install_update` (or a new `download_update_after_check`) use the stashed pair — no FE-supplied URL needed at all. Cleaner contract.

## Notes

- This is genuinely lower risk than F1 because minisign verification gates the install. Filed medium so it lands on the "nice cleanup before launch" list.
- The TLS stack is `rustls` (good, no system OpenSSL surface).
- `installer.rs` uses `osascript do shell script ... with administrator privileges` plus `rsync -a --delete` — verify (out of scope for this lens) that the temp-extracted tree path can't be poisoned between extract and rsync; the staging dir is per-instance under `$TMPDIR` so OS-level isolation already helps.
