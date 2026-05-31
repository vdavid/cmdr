# `download_update` fetches a frontend-supplied URL with no scheme/host check

**Severity:** low **Lens:** F — Security **Confidence:** medium

## Location

`apps/desktop/src-tauri/src/updater/mod.rs:116-153`

## What

`download_update(url, signature, ...)` is a Tauri command whose `url` is whatever the frontend hands it. The frontend
gets that URL from `check_for_update` (the verified manifest), but the command itself does not re-validate that the URL
is HTTPS or points at the expected update host before issuing the GET. The minisign signature on the downloaded bytes IS
verified before anything is written to disk (`signature::verify` at `:137`), so this is NOT a code-execution path — a
wrong/malicious URL can only serve bytes that fail signature verification. The residual risk is a request-forgery
primitive: any caller that can reach the IPC surface (in prod, only the app's own trusted frontend) could point the
download client at an arbitrary target.

## Why it matters

In production the IPC surface is reachable only from Cmdr's own frontend (`withGlobalTauri: false`, CSP locks
`connect-src`), so the practical attacker would need to already control the webview — a much bigger compromise. The
signature gate means the worst outcome is a failed install, not RCE. Filing as low because the defense-in-depth check is
cheap and the current contract relies entirely on the caller passing a trusted URL.

## Evidence

```rust
pub async fn download_update(url: String, signature: String, state: State<'_, UpdateState>) -> Result<(), String> {
    log::info!("Downloading update from {url}");
    let client = reqwest::Client::builder()
        .connect_timeout(DOWNLOAD_CONNECT_TIMEOUT)
        .read_timeout(DOWNLOAD_READ_TIMEOUT)
        .build() ...;
    let response = client.get(&url).send().await ...;   // <-- no scheme/host assertion on `url`
    let bytes = response.bytes().await ...;
    signature::verify(&bytes, &signature)?;             // <-- integrity/authenticity gate (good)
```

## Suggested fix

Assert `url.starts_with("https://")` and that the host matches the known update host (`getcmdr.com` / the R2 download
domain the manifest uses) at the top of `download_update`, returning an error otherwise. This mirrors the
`validate_ai_base_url` HTTPS-only guard already used for the AI BYOK endpoint and removes the "trust the caller's URL"
assumption without changing the happy path.

## Notes

The signature verification is the load-bearing control and it's correctly placed before the disk write and the
atomic-rename install. This finding is purely about tightening the request surface.
