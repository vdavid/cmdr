# cmdr-http

The one door every Cmdr HTTP client is built through: `client_builder()` is `reqwest::Client::builder()` plus a
`Proxy::custom` that routes each destination the way macOS would, minus the cases where that hurts.

## Module map

- `lib.rs`: `client_builder()` and the injectable `builder_with` the end-to-end tests drive.
- `route.rs`: the decision order (`decide`) and the loopback / link-local test.
- `env.rs`: the `*_PROXY` / `NO_PROXY` variables, read once.
- `system.rs`: CFNetwork's verdict per URL (`mac::entries_for`) and the walk over its list (`first_route`).
- `pac.rs`: PAC files (and so WPAD), run by macOS, with a per-host answer cache.

## Must-knows

- ❌ **Never build a bare reqwest client** (`Client::new`, `Client::builder`, `ClientBuilder::new`, `reqwest::get`).
  `clippy.toml` refuses them everywhere but here. A bare client proxies `127.0.0.1` and ignores PAC and the bypass list.
- ❗ **A third-party crate that builds its own client bypasses this.** `genai` does unless handed one: every
  `genai::Client` gets `.with_reqwest(...)` built on `client_builder()` (`ai/client.rs`).
- ❗ **Loopback and link-local always go direct**, before the environment and the system: a proxy is another machine and
  can't reach this Mac's own `127.0.0.1` (the local llama-server, Ollama, LM Studio).
- ❗ **Order**: local → `NO_PROXY` (direct, past every proxy) → `HTTP(S)_PROXY` / `ALL_PROXY` → macOS. Same variable
  names and precedence as reqwest's own reader, so an env setup that worked before still works.
- **Adding `Proxy::custom` switches reqwest's own system-proxy reader off** (`ClientBuilder::proxy`), so this closure is
  the whole decision. Calling `.proxy()` or `.no_proxy()` again on top of `client_builder()` silently changes it.
- **macOS answers via `CFNetworkCopyProxiesForURL`**, the same call Safari's stack makes, so the bypass list and
  "Exclude simple hostnames" match exactly. About 15 µs a lookup, so that layer has no cache.
- ❗ **A PAC miss blocks the asking thread for up to 3 s** (reqwest asks synchronously). It runs on a short-lived thread
  with its own run loop; a failure or timeout goes direct, as macOS does, and is cached 30 s, an answer 5 min.
- ❗ **The PAC `Execute…` functions are declared in `pac.rs`, not taken from `objc2-cf-network`**: its callback type
  says the proxy list is non-null, and CFNetwork passes null on failure.
- ❗ **A SOCKS proxy from macOS (the manual setting or a PAC) becomes `socks5h://`**: the proxy resolves names, as
  macOS's own stack does. An env var's `socks5://` (local DNS) vs `socks5h://` is kept as typed. FTP entries are
  skipped. CFNetwork drops a PAC's `SOCKS5` / `SOCKS4` answers; only `SOCKS` reaches Cmdr.

Decision order, the CFNetwork answer's shape, PAC mechanics, and measurements: `DETAILS.md`.
