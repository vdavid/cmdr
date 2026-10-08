# Proxies, PAC files, and TLS inspection (2026-10)

How Cmdr's outbound connections behave on a corporate network: behind an HTTP(S) proxy, with a PAC file, and behind a
TLS-inspecting proxy whose root CA is trusted on the Mac. Verified on Cmdr 0.50.0 (`/Applications/Cmdr.app`) and the
worktree at `3095412ec`, macOS 27.0 (Darwin 27.0.0), 2026-10-06. This answers the `/trust` gap "Not tested behind a
TLS-inspecting proxy" (vdavid/cmdr#118).

The gaps it found are fixed (§ "Fixes"), and § "After the fixes" re-runs the system-proxy tests against the new client
builder. The mechanism now lives in `crates/cmdr-http/DETAILS.md`; this note keeps the evidence.

## Summary (0.50.0, before the fixes)

- **Every HTTP request Cmdr makes goes through one reqwest configuration**, so they all behave the same: reqwest 0.13.4,
  rustls with `rustls-platform-verifier` 0.7.0, and hyper-util 0.1.20's system-proxy matcher. No call site sets
  `.proxy()`, `.no_proxy()`, a custom root store, or `danger_accept_invalid_certs` (verified by grep over
  `apps/desktop/src-tauri/src` and `crates/`).
- **Honored**: the static HTTP and HTTPS proxy from System Settings (read live from `SCDynamicStore` each time a client
  is built), and the `HTTPS_PROXY` / `HTTP_PROXY` / `ALL_PROXY` / `NO_PROXY` env vars.
- **Not honored**: PAC files (automatic proxy configuration), WPAD (auto proxy discovery), and the system proxy's bypass
  list ("Bypass proxy settings for these hosts"). With a PAC file, Cmdr connects directly.
- **TLS trust comes from macOS**: certificates are checked by Security.framework (`SecTrust`), so a root CA trusted in
  the keychain (as MDM deploys a company's inspection CA) is trusted by Cmdr too. An untrusted inspection CA fails
  cleanly with `errSecNotTrusted` (-67843).
- **Loopback and LAN traffic gets proxied** when a system proxy is on, because the bypass list is ignored and nothing
  exempts `127.0.0.1`. That breaks Cmdr's own local AI server and LAN WebDAV/S3 on a proxied network.

## Outbound connections (static inventory)

All use `reqwest::Client::builder()` (or `Client::new()`) with timeouts and, for AI, a redirect policy; nothing else.

- **Update check and download** (`updater/mod.rs`): `api.getcmdr.com/update-check/…` → `getcmdr.com/latest.json`, then
  the tarball from GitHub Releases (`github.com` → `release-assets.githubusercontent.com`). Custom updater, not
  `tauri-plugin-updater`'s client (the plugin's crate is in the graph but its download path isn't used on macOS).
- **License validate / activate** (`licensing/validation_client.rs`): `api.getcmdr.com`.
- **Usage stats** (`analytics/heartbeat.rs`): `api.getcmdr.com/heartbeat`.
- **Crash reports** (`crash_reporter/pending_delivery.rs`), **error reports and amend** (`error_reporter/`):
  `api.getcmdr.com`.
- **Feedback, beta signup** (`feedback.rs`, `commands/beta_signup.rs`): `api.getcmdr.com`.
- **S3 price table** (`s3_costs/price_source.rs`): `api.getcmdr.com/s3-prices/v1`.
- **Cloud AI** (`ai/client.rs` via `genai`, `ai/connection_check.rs`): the provider the user configures.
- **Local AI health check** (`ai/client.rs` `health_check`): `http://127.0.0.1:<port>/health`, plain HTTP. The chat
  calls to the local llama-server go to the same loopback address through `genai`.
- **Model downloads** (`ai/download.rs`, image-search model via `cmdr-index` `clip/install.rs`): `huggingface.co`, which
  302-redirects to a CDN host under `hf.co` (seen: `us.aws.cdn.hf.co`, `curl -sI`, 2026-10-06).
- **Remote file backends**: WebDAV and S3 use the same reqwest (`crates/cmdr-webdav`, `crates/cmdr-s3`), so they follow
  the same proxy rules. SMB, SFTP, MTP, and ADB open their own sockets and never use an HTTP proxy (no SOCKS either).
- **Webview**: no frontend `fetch` to an external host (grep, plus the CSP's `connect-src`). Links open in the browser.

## Why it behaves this way (source evidence)

- **Proxy**: reqwest's builder defaults to `Matcher::system()`, which calls hyper-util's `Builder::from_system()`: env
  vars first, then on macOS `mac::with_system` reads only `HTTPEnable/HTTPProxy/HTTPPort` and the `HTTPS` trio from
  `SCDynamicStore::get_proxies()`. It never reads `ProxyAutoConfigEnable`, `ProxyAutoDiscoveryEnable`, or
  `ExceptionsList`, and it has no implicit loopback exemption (`hyper-util-0.1.20/src/client/proxy/matcher.rs`).
- **Fragile**: the app's own `reqwest` line (`apps/desktop/src-tauri/Cargo.toml`) has `default-features = false` without
  `system-proxy`. The feature reaches the build only through `genai` 0.6.5's dependency, by Cargo feature unification
  (`cargo tree -e features -i reqwest`). If `genai` dropped it, the system proxy would silently stop working and only
  env vars would remain.
- **TLS**: reqwest's `rustls` feature pulls in `rustls-platform-verifier`, whose Apple backend (`verification/apple.rs`,
  present in the release binary per `strings`) evaluates the chain with `SecTrust`. The release binary links
  `Security.framework` and `SystemConfiguration.framework` (`otool -L`).
- **Proxy auth**: only credentials embedded in a proxy URL (`http://user:pass@host:port`) work. reqwest doesn't read the
  system proxy's stored credentials and can't do NTLM or Kerberos (`407` stays a failure). Static finding, not tested.

## Empirical tests

Setup: mitmproxy isn't installed, so a ~150-line Go proxy stood in for it (scratchpad only, bound to `127.0.0.1`): a
passthrough mode that logs each `CONNECT` host, and an inspecting mode that terminates TLS with a leaf cert minted by
its own throwaway CA, the way a corporate TLS-inspecting proxy does. It also served the PAC file. A probe binary pinned
to Cmdr's exact `Cargo.lock` and reqwest feature set
(`json, rustls, stream, multipart, system-proxy, gzip, charset, http2`) and built like Cmdr's clients made the requests.
System proxy changes went on the Wi-Fi service with `networksetup`, restored by a shell `trap`.

- **Env var, inspecting proxy, CA not trusted**: `HTTPS_PROXY=http://127.0.0.1:18443`, GET
  `https://getcmdr.com/latest.json` → went through the proxy, then failed:
  `invalid peer certificate: … certificate is not trusted: -67843`. The proxy saw the client abort the handshake
  (`unknown certificate`). Fails closed, as it should.
- **Env var, passthrough proxy**: → 200 through the proxy.
- **Loopback over plain HTTP with `HTTP_PROXY` set**: `http://127.0.0.1:18444/…` went to the proxy. With
  `NO_PROXY=127.0.0.1,localhost` it went direct.
- **Static system proxy**: → the probe's HTTPS request and its `http://127.0.0.1` request both went through the proxy.
- **Static system proxy plus bypass list** containing `getcmdr.com` and `127.0.0.1`: both still went through the proxy.
  The bypass list is ignored.
- **PAC file** (`ProxyAutoConfigURLString` set, PAC returning `PROXY 127.0.0.1:18444` for every non-local host): the
  probe connected directly; the proxy saw no `CONNECT` from it and the probe never fetched the PAC file. Chrome,
  Dropbox, Obsidian, and `CFNetworkAgent` fetched the PAC within a second. On a network where only the proxy can reach
  the internet, Cmdr would fail every request.
- **The real release app, static system proxy**: held a passthrough system proxy across Cmdr 0.50.0's scheduled update
  check. The proxy logged `CONNECT api.getcmdr.com:443` at 10:40:07.38 and `CONNECT getcmdr.com:443` at 10:40:07.95, and
  Cmdr's own log (`~/Library/Logs/com.veszelovszki.cmdr/cmdr.log`) shows
  `reqwest::connect proxy(http://127.0.0.1:18444/) intercepts 'Some("api.getcmdr.com")'`. That DEBUG line is the
  quickest way to see whether a user's Cmdr is using their proxy.
- **Env var, inspecting proxy, CA trusted**: the test CA was trusted at the admin level, the domain MDM-deployed company
  roots live in (`security add-trusted-cert -d -r trustRoot`, which shows a GUI password prompt). GET
  `getcmdr.com/latest.json`, `api.getcmdr.com/`, and `huggingface.co/` → all 200, and the proxy logged a completed
  handshake on its own minted cert for each, then the decrypted request. Trust and cert were removed right after
  (`security remove-trusted-cert -d`, `security delete-certificate`), and `find-certificate` and `dump-trust-settings`
  confirm both are gone.

Not run against the live app: license validate, crash or error report upload, and model download. They share the client
and configuration above, and sending test traffic to prod endpoints wasn't worth it.

Side note: this network intermittently gave `No route to host (os error 65)` for direct requests while `curl` worked
(the host resolves to IPv6 first). It's unrelated to proxies, but it can make a PAC test look like a proxy failure.
Rerun before drawing conclusions.

## Fixes

All three landed on 2026-10-06, after 0.50.0:

1. **`system-proxy` declared on the app's own reqwest line** (`a2a4f302d`), so it no longer rode in on `genai`. Made
   moot by fix 2 the same day: a `Proxy::custom` switches reqwest's own system reader off, so the feature was dropped
   again.
2. **One client builder, `cmdr_http::client_builder()`** (`8ff40f9c5`), that every client goes through (`clippy.toml`
   refuses a bare reqwest client): loopback and link-local always direct, then `NO_PROXY` and the `*_PROXY` variables,
   then macOS's own verdict per URL via `CFNetworkCopyProxiesForURL`, which applies the bypass list and "Exclude simple
   hostnames". The `genai` clients get it through `.with_reqwest`.
3. **PAC and WPAD** (the commit after it): a PAC entry in macOS's answer is run by
   `CFNetworkExecuteProxyAutoConfigurationURL` on a short-lived run-loop thread, with a 3-second limit, a direct
   fallback, and a per-host cache. WPAD needed nothing extra: with discovery on, macOS hands over `http://wpad/wpad.dat`
   as an ordinary PAC URL.

How it all works: `crates/cmdr-http/DETAILS.md`.

4. **SOCKS** (2026-10-08, after 0.50.0): from env vars, the macOS "SOCKS proxy" setting, and a PAC's `SOCKS` answer. §
   "SOCKS" below has the evidence.

Still open: **proxy authentication** beyond credentials in a proxy URL (Basic from the keychain, NTLM, Kerberos). Out of
scope until someone asks.

## After the fixes

Same Go proxy, now with a probe built on `cmdr-http` (the app's reqwest features, `client_builder()` plus timeouts), on
macOS 27.0, 2026-10-06. Each run set the Wi-Fi proxy with `networksetup`, requested `https://getcmdr.com/latest.json`,
`http://127.0.0.1:18443/` (loopback), and `http://intranet-cmdr-test.local:18443/` (a `.local` host that doesn't
resolve), then restored the settings through a `trap`.

- **PAC file** (`PROXY 127.0.0.1:18444` for every non-local host): `CFNetworkAgent` fetched the PAC, the proxy logged
  `CONNECT getcmdr.com:443` (200), and the `.local` request reached the proxy too, as the PAC said. Loopback went
  direct.
- **Unreachable PAC** (`http://127.0.0.1:18445/proxy.pac`, nothing listening): every request went direct and succeeded,
  without a noticeable wait.
- **Manual proxy, default bypass list** (`*.local`, `169.254/16`): `getcmdr.com` went through the proxy; the `.local`
  host went direct (a local DNS failure, and nothing at the proxy); loopback went direct.
- **Manual proxy, bypass list plus `getcmdr.com` and `127.0.0.1`**: nothing reached the proxy.
- **WPAD**: not run end to end (it needs a `wpad` DNS name on the network). With discovery switched on, macOS's answer
  for a URL was the PAC entry `http://wpad/wpad.dat` followed by DIRECT, which is the path the PAC test exercises.

After the runs the Wi-Fi service was back to no manual proxy, no PAC, no discovery, and the default bypass list
(`scutil --proxy`, `networksetup -get…`). One leftover: `networksetup` can't clear a stored auto-proxy URL, so a
disabled `http://127.0.0.1:18445/proxy.pac` stays in the Wi-Fi settings, inactive.

## SOCKS

A ~130-line Go SOCKS5 server (scratchpad only, `127.0.0.1:21080`, optional RFC 1929 sign-in) that logged each `CONNECT`
as a name or an IP and relayed it, plus a PAC file on `127.0.0.1:21081`. A probe built on `cmdr-http`
(`client_builder()` plus a timeout) requested `https://getcmdr.com/latest.json`, `http://example.com/`, and the PAC URL
on loopback. macOS 27.0, 2026-10-08. Every Wi-Fi change was undone by a shell `trap`.

- **`ALL_PROXY=socks5h://…`**: both requests → 200, the server logged `CONNECT NAME getcmdr.com:443` and
  `NAME example.com:80`. Loopback went direct.
- **`ALL_PROXY=socks5://…`**: → 200, logged as IPv4 addresses: the name was resolved on the Mac, as the scheme says.
- **`socks5h://ada:p%40ss%20word@…`, server requiring sign-in**: → 200, signed in as `ada`. A wrong password fails with
  `SOCKS error: credentials not accepted`.
- **macOS SOCKS setting** (`networksetup -setsocksfirewallproxy Wi-Fi 127.0.0.1 21080`): → 200, both as names. A
  `URLSession` request from a Swift script reached the same server as `NAME example.com:80`, so `socks5h` matches what
  macOS's own stack does. Loopback went direct.
- **PAC** answering `SOCKS 127.0.0.1:21080`: `CFNetworkAgent` fetched the PAC, then both requests reached the server as
  names (200). Loopback went direct.
- **PAC keywords** (unit test against CFNetwork, `pac_test.rs`): `SOCKS` in any case is kept; `SOCKS5`, `SOCKS4`, and
  `HTTPS` are dropped from the parsed list, so a PAC written for Chrome with `SOCKS5` is ignored by Cmdr as by Safari.

Not verified: SOCKS credentials stored with the macOS setting (needs a keychain item), and a SOCKS4-only server from the
macOS setting (Cmdr speaks SOCKS5 there). Afterwards Wi-Fi was back to SOCKS off with no server, PAC off with its
previous stored URL, and no server process left (`networksetup -get…`, `scutil --proxy`, `pgrep`).
