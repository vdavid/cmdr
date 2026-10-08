# Gating the mDNS browse on need: what it saves

The SMB discovery browse used to run for the app's whole life once a user had answered the macOS Local Network prompt.
It now runs only while something needs it: the Servers view on screen, an SMB upgrade resolving a mount's server, and a
10 s warm-up at launch. The mechanism and the staleness policy live in `apps/desktop/src-tauri/src/network/DETAILS.md` §
"Discovery runs only while something needs it"; this note holds the measurement.

## Method

- **Binaries**: release builds (`pnpm tauri build --no-bundle`) of `e9fd713ad` (base) and of the branch (after), run as
  bare binaries with their own `CMDR_INSTANCE_ID` and `CMDR_DATA_DIR`.
- **Data**: one fresh data dir shared by both, with `network.firstTriggerDone` on (so base browses from launch, the prod
  case), indexing, media indexing, the SMB upgrade pass, update checks, and the global shortcut off. No pane on the
  Servers view.
- **Protocol**: three interleaved rounds (base, after, base, after, base, after), each a fresh launch; 60 s to settle
  (past the after build's warm-up and linger), then a 240 s idle window.
- **Instrument**: per-thread CPU with names, read with `proc_pidinfo(PROC_PIDTHREADINFO)` at both ends of the window
  (`pth_user_time + pth_system_time` per `pth_name`), plus `ps -o time` for the process and `ps -M` for the thread
  count. `ps -M` alone has no thread names, which is why the per-thread read went through `libproc`.
- **Machine**: David's M3 MacBook Pro, macOS 27.0, load average 10–37 from sibling agents' builds. Seven multicast
  interfaces (`en0`, `lo0`, and five VM bridges), and two SMB servers answering on the LAN.

## Results

Verified on release builds, `proc_pidinfo` per-thread CPU deltas over 240 s, 2026-09-27:

- **`mDNS_daemon` + `mdns-event-loop`, base**: 119.5, 132.8, and 223.2 ms per 240 s. Median **0.055% of a core**
  (0.050–0.093%), about **2 CPU-seconds an hour** (1.8–3.3). The event loop is under 1 ms of that; the cost is the
  daemon's timers and the multicast traffic it answers.
- **After**: both threads are absent in every round, so that cost is **zero** at idle.
- **Threads**: 38 in every base round; 34, 37, and 36 after. By name the difference is exactly the two mDNS threads; the
  rest is `tokio-rt-worker` churn between samples.
- **Whole process**: 1.60, 1.83, and 3.95 s per 240 s base against 2.33, 1.81, and 1.95 s after (medians 1.83 and 1.95).
  The saving is well inside the noise of a process-level read on a loaded machine, which is why the per-thread read is
  the number to quote.

In the hub's terms (about 30 threads at 0.05–0.22% each), this was one of the cheaper threads: a clear win, but two
quiet threads, not a hotspot. The daemon queries and listens on every multicast interface, so a Mac with a VM runtime
likely pays more than one with Wi-Fi alone (unmeasured).

## The behavior it keeps

Checked on the after build over MCP, with a probe service registered by `dns-sd -R` (2026-09-27):

- At launch the browse runs for the 10 s warm-up plus the 10 s linger, then its threads are gone.
- Opening the Servers view starts a browse at once; the two cached servers are listed immediately and re-resolved within
  about 140 ms.
- A service registered while the view is open appears, disappears when unregistered, and reappears when registered
  again.
- Leaving the view stops the browse about 10 s later.
- A server that left while nothing browsed is still listed from the cache when the view reopens, and is gone within the
  settle window.

## Not measured

- **Wakeups**: `top`'s `IDLEW` is unreliable on macOS 27 (hub rule), and the per-process wakeup counters are too noisy
  to isolate two threads on a loaded machine. The daemon's periodic interface check and query schedule are gone with it,
  so the wakeups go the same way as the CPU.
- **The launch cost**: each launch now browses for about 20 s (warm-up plus linger) instead of forever. The hosts it
  finds get no SMB connection from that: share lists are read only once a Servers view opens (saved servers) or a host
  is opened (found ones), per #324. The rule: `apps/desktop/src/lib/file-explorer/network/DETAILS.md`.
