# cmdr-http details

## Why a crate

Every HTTP client in Cmdr (the app's api-server calls, the updater, AI, model downloads, and the WebDAV and S3 backends)
needs the same proxy behavior, and the backend crates can't depend on the app. One crate both sides depend on keeps the
routing in one place, and `clippy.toml`'s `disallowed-methods` makes it the only way in. Evidence that motivated it:
`docs/notes/proxy-and-tls-inspection-2026-10.md`.

## The decision, per destination

reqwest calls the `Proxy::custom` closure with `scheme://host[:port]` once per new connection and, for a plain-HTTP
request, once per request (to decide on proxy auth). `route::decide` answers:

1. **No host** → direct.
2. **Local host** → direct. `localhost` and `*.localhost`, `127.0.0.0/8`, `::1`, `169.254.0.0/16`, `fe80::/10`,
   `0.0.0.0` / `::`, and the IPv4-mapped forms of those. Not `*.local` and not private ranges: those are LAN hosts a
   corporate proxy may legitimately be asked to skip through the bypass list, which macOS lists by default (`*.local`,
   `169.254/16`).
3. **`NO_PROXY` matches** → direct, even when the system would proxy. Same as reqwest (hyper-util's matcher applies `no`
   to every proxy). Entries: `*`, domains (`corp.example`, `.corp.example`, `*.corp.example` all cover the domain and
   its subdomains), IPs, CIDR blocks; a `:port` suffix is ignored.
4. **`HTTPS_PROXY` / `HTTP_PROXY`** for the URL's scheme, else `ALL_PROXY` → that proxy. Uppercase wins over lowercase.
   A value without a scheme gets `http://`. Credentials in the URL are kept; reqwest's matcher turns them into a
   `Proxy-Authorization` header, or for a SOCKS URL into the SOCKS5 username/password sign-in (RFC 1929). A `socks4` /
   `socks4a` / `socks5` / `socks5h` value is passed through as typed, so `socks5://` resolves names on this Mac and
   `socks5h://` at the proxy (curl's convention, and what someone writing the variable expects).
5. **The OS** (`system::OsSystem`, per OS under `system/platform/`) → on macOS CFNetwork's verdict. Linux and Windows
   read no system layer yet, so this step is direct there.

The environment is snapshotted on the first `client_builder()` call (`OnceLock`): a running process's environment
doesn't change.

## The macOS answer

`CFNetworkCopySystemProxySettings()` returns the live settings (cheap: CFNetwork caches them, 76 ns a call) and
`CFNetworkCopyProxiesForURL(url, settings)` returns an ordered list of dictionaries, each with a `kCFProxyTypeKey`.
`first_route` walks it:

- `kCFProxyTypeNone` → direct.
- `kCFProxyTypeHTTP` / `kCFProxyTypeHTTPS` → `http://host:port` (the HTTPS type is an HTTP proxy that tunnels with
  `CONNECT`, so the proxy URL's scheme stays `http`). Username and password are embedded when CFNetwork supplies both.
- `kCFProxyTypeSOCKS` → `socks5h://host:port` (port 1080 when missing), credentials embedded the same way. See § "SOCKS"
  for the version and DNS choice.
- `kCFProxyTypeAutoConfigurationURL` / `…JavaScript` → the PAC's own list for this URL, walked the same way (a PAC
  answer never names another PAC). A PAC that can't run moves on to the next entry.
- FTP, or an entry missing its host → skipped, logged at debug; the next entry decides.

Credentials are percent-encoded byte by byte, never `form_urlencoded`: hyper-util percent-decodes a proxy URL's
userinfo, so a `+` standing for a space would reach the proxy as a literal `+` (`entries_test.rs`).

## SOCKS

reqwest's `socks` feature (no extra crate: the SOCKS client is hyper-util's, already in the graph) lets the closure
answer with a `socks…://` URL. Sources:

- **Env var**: any of the four SOCKS schemes, as typed (above).
- **macOS "SOCKS proxy" setting**: CFNetwork lists it after an HTTP(S) proxy for the same scheme, so with both set,
  `https` goes to the HTTPS proxy and plain `http` (with no HTTP proxy) to SOCKS (`system_test.rs`).
- **PAC**: CFNetwork's PAC parser knows `SOCKS host:port` (any case) and drops `SOCKS5` and `SOCKS4` from the list
  entirely, as it does `HTTPS` (verified on macOS 27.0, `pac_test.rs::cfnetwork_drops_the_versioned_socks_keywords`,
  2026-10-08). Cmdr only sees the parsed list, so it ignores those words exactly as Safari does.

**Decision: SOCKS5 with remote DNS (`socks5h`) for anything macOS hands over.** CFNetwork's SOCKS entry names no
version, and macOS's own stack speaks SOCKS5 to it. Remote resolution because a network that forces traffic through
SOCKS often can't resolve outside names locally, and a local lookup would show the network's DNS every host Cmdr talks
to. It's also what macOS does: with the SOCKS setting on, both Cmdr and a `URLSession` request reached the test proxy as
a name, not an IP (macOS 27.0, a logging SOCKS5 server, 2026-10-08). A SOCKS4-only server won't work from the macOS
setting; env `socks4://` does.

**Credentials**: an env URL's `user:password@` signs in (verified end to end, wrong credentials fail with
`SOCKS error: credentials not accepted`). From the macOS setting, they're used when CFNetwork's entry carries both
username and password; not verified, since that needs a keychain item.

- An exhausted list → direct.

When a PAC file is configured, the list is `[AutoConfigurationURL, None]`: CFNetwork appends its own DIRECT fallback
(verified on macOS 27.0, `system_test.rs::a_pac_setting_surfaces_as_its_url`, 2026-10-06). So a PAC that can't be
fetched, throws, or times out ends in direct, which is what macOS does too.

**WPAD needs nothing of its own.** With "Auto proxy discovery" on, the live settings carry
`ProxyAutoConfigURLString = http://wpad/wpad.dat` (DNS discovery) and the list is the same PAC entry (verified on macOS
27.0 by turning discovery on and printing `entries_for` over `CFNetworkCopySystemProxySettings()`, 2026-10-06).

Cost: about 15 µs per lookup in a release build, almost all of it `CFNetworkCopyProxiesForURL` (macOS 27.0, M3 MacBook
Pro, 1,000 lookups of `https://api.getcmdr.com`, 2026-10-06). That's noise next to a connection, so the static path has
no cache.

## PAC evaluation

`CFNetworkExecuteProxyAutoConfigurationURL` (or `…Script` for an inline one) fetches the file itself and answers through
a callback on a run loop. reqwest's closure is synchronous, so `MacPac::evaluate`:

1. Checks the cache, keyed by the PAC source and `scheme://host[:port]` (reqwest never passes a path, so a rule on the
   path sees `/`).
2. Takes the one evaluation turn, then checks again: a burst of first connections to one host (an S3 upload's parts)
   waits for one answer instead of running the PAC N times.
3. Spawns `cmdr-http-pac`, a short-lived thread that adds the source to its own run loop in a private mode and runs that
   mode until the callback fills a stack slot or `TIMEOUT` (3 s) passes, then invalidates the source so the callback
   can't fire into a dead frame. The asking thread waits on a channel for at most `TIMEOUT` + 1 s.
4. Caches the answer for 5 minutes, or a failure for 30 seconds (a dead PAC host costs one timeout per host per half
   minute), capped at 256 entries.

A thread per miss, not a long-lived run-loop thread: misses are rare (one per host per five minutes), and a fresh thread
needs no cross-thread wakeup source and holds nothing while idle.

One evaluation can make CFNetwork fetch the PAC more than once (three connections per evaluation on macOS 27.0), a late
one can land after the answer is back, and CFNetwork keeps the fetched file, so a changed PAC doesn't show in its next
answer either (verified on macOS 27.0, by switching the served script mid-test, 2026-10-25). So neither the server's
hits nor the answers tell a cache hit from a miss: `pac_test.rs::an_answer_is_cached_per_host` reads `MacPac`'s
test-only `evaluations` count.

Two of those connections are TLS: for an `http://` PAC URL, CFNetwork sends a TLS ClientHello on two connections first,
and only the third asks `GET /proxy.pac` in plain HTTP (verified on macOS 27.0, a loopback server logging each
connection's first bytes, 2026-10-29). A process's FIRST evaluation is the slow one: 0.2–2 s on a busy Mac, then about 1
ms for the next host (same probe). A nextest process pays that cold start every time, which is why answer-expecting
cells take the test-only `ANSWER_BUDGET` (#754).

### Launch warm-up

Left alone, a PAC on a loaded Mac could miss `TIMEOUT` on that cold first evaluation and send the launch's first
requests direct for `FAILURE_TTL`, past a proxy that should have covered them (#810). So the app calls
`warm_up_pac("https://api.getcmdr.com")` right after the logger starts:

- A `cmdr-http-pac-warm-up` thread runs `route::decide` for that URL with `WarmingSystem` (macOS's arm), which is the
  macOS route with the PAC miss given `WARM_UP_TIMEOUT` (10 s) through `MacPac::with_budget`. Same cache, same turn:
  only the budget differs. No PAC, a local host, or an env var deciding first → nothing runs, at the cost of one 15 µs
  lookup.
- **A request arriving meanwhile joins it**: it blocks on the turn, then reads the cached answer, so it can wait up to
  `WARM_UP_TIMEOUT` + 1 s instead of `TIMEOUT` + 1 s, once, and only while the PAC is genuinely slow
  (`pac_test.rs::a_request_during_the_warm_up_joins_it`).
- **The URL is reduced to `scheme://host[:port]`** (`as_asked`), the exact form reqwest's `Proxy::custom` asks with
  (reqwest 0.13.5 `Custom::call`), or the warm-up would fill a cache key no request reads
  (`lib_test.rs::the_pac_warm_up_asks_about_a_url_exactly_as_reqwest_does`).
- A warm-up that fails is cached as any failure (`FAILURE_TTL`): a PAC that can't answer in 10 s wouldn't in 3 either.
- Other hosts gain too: the cold cost is per process, so their first evaluation is the warm ~1 ms one.

**Unverified: whether sleep/wake or a network change makes the next evaluation cold again.** Plausible for a network
change (CFNetwork likely drops the fetched PAC file and refetches, TLS attempts included), less so for wake. Not probed:
it needs a real Wi-Fi switch or sleep cycle with the loopback probe above, timing the first evaluation after. If it
turns out cold, warm up again on `NSWorkspaceDidWakeNotification` and on a `SCDynamicStore` network-change key. Until
then, the worst case after such an event is one host going direct for `FAILURE_TTL`.

❗ The two `Execute…` functions are declared in `pac.rs` with raw pointers rather than taken from `objc2-cf-network`
0.3.2: its `CFProxyAutoConfigurationResultCallback` types the proxy list as `NonNull`, and CFNetwork passes null when
the PAC fails, which would be undefined behavior in a `NonNull` parameter.

## Testing

- `route_test.rs` / `env_test.rs`: the pure decision, with the environment injected (`EnvProxies::from_vars`) and a fake
  system layer. ❌ Never `std::env::set_var` in a test: the runner is parallel.
- `system/platform/macos_test.rs`: the real CFNetwork call on macOS against a settings dictionary the test builds
  (`settings`), so it never reads or changes this Mac's own settings.
- `pac_test.rs`: real PAC evaluation against a PAC file served from loopback, an inline script, the cache, an
  unreachable PAC host, one that never answers, and a broken script.
- `lib_test.rs`: the built client end to end, with a fake proxy and origin on loopback, and a fake SOCKS5 server that
  records the sign-in and whether it got a name or an IP.
- Against the real system settings: `docs/notes/proxy-and-tls-inspection-2026-10.md` § "After the fixes" (a probe on
  this crate, the Wi-Fi proxy set to a manual proxy, a bypass list, and a PAC file in turn).
