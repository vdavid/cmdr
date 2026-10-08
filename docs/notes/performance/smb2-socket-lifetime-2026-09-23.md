# Two `smb2` socket-lifetime bugs, fixed in `smb2` 0.24.1 (2026-09-23)

**What this settles:** why a long-running Cmdr held SMB sockets it no longer used (two in `CLOSE_WAIT`, and 78
ESTABLISHED to the Docker test fixtures), and that both were `smb2` defects Cmdr needed only a version bump for. Cmdr
ships `smb2` 0.27.0 now, which carries the fix. The memory cost was negligible; the real costs were file descriptors,
server-side sessions, and about 86 timer wakeups a second from the tasks that stayed alive.

Found while checking the MCP "leak" (`mcp-connection-leak-2026-09-22.md`, which is not one). The ownership rules that
now prevent both bugs live in the `smb2` repo's client module doc, § "Socket lifetime".

## Bug A: a server hang-up left the socket in `CLOSE_WAIT` (bounded)

- A `Connection` shares one `TcpTransport` between its receiver and writer tasks. When the server sent FIN, the receiver
  returned and marked the connection disconnected, but nothing told the writer, which stayed parked in `rx.recv()`
  holding the transport, and with it the fd, for as long as anyone held the `Connection`.
- In Cmdr that holder is a share's main session (`SmbVolumeInner.client`, built with `auto_reconnect: false`). Nothing
  notices the dead session until the next operation fails and the reconnect path swaps in a new client.
- Seen in prod: the NAS dropped an idle main session after ~10.7 min (Samba's `deadtime` of about 10 min), while the
  watcher's session, which keeps a handle open and a CHANGE_NOTIFY pending, stayed ESTABLISHED.
- Bound: at most one per registered SMB volume, cleared by the next operation on it.

## Bug B: dropping a `Connection` never closed its socket (unbounded)

- The receiver task held a STRONG `Arc<Inner>`, the other three tasks a `Weak`, and only `Inner::drop` aborted the
  receiver: a reference cycle through a task. When the consumer dropped every clone, the socket, `Inner`, and all four
  tasks lived on until the SERVER hung up, with the keepalive waking once a second and the sweeper every 10 s.
- Every place Cmdr discards a session hit it: the watcher restarting on reconnect, `on_unmount`, retiring scan-pool
  members, share listing, and the predecessors dropped on reconnect.
- A server with idle reaping bounds it; one without (Samba's default `deadtime = 0`, which the Docker fixtures use)
  never does. The prod process held **78 ESTABLISHED sockets to the fixtures** from 126 connects over 60 E2E
  mount/unmount cycles.
- Memory: 7,753 B of live heap per leaked connection (measured over 200), so about 0.8 MB for 78. Not the heap mystery.

## The fix (`smb2` 0.24.1)

- **B**: the receiver takes a `Weak<Inner>` too, upgraded once per frame and dropped before waiting again. When the last
  `Connection` drops, `Inner::drop` aborts every task and the fd closes.
- **A**: a per-generation `SocketLife` flag shared by the writer and receiver; any exit of either ends it, and each task
  exits once it ends. A disconnect of any kind (hang-up, bad frame, failed write, `mark_dead`) now closes the fd at
  once, while the `Connection` stays usable as a disconnected one, so Cmdr's lazy reconnect works unchanged.
- No public API change; released as a patch.

Verified against the published crate with a standalone repro (an in-process `TcpListener`, no containers), 2026-09-23:
dropping 200 connections, the server saw EOF on 200 of 200 (0 before), no client sockets left (200 before), and 296 B of
residual heap per connection (7,753 before); a server hang-up left no `CLOSE_WAIT`, immediately or 5 s later. `smb2`'s
own gates and Docker suite passed, and so did Cmdr's SMB fixture lane. Not run: `smb2`'s consumer suite for 0.24.1.

## The fix holds in Cmdr (acceptance check, now a lasting test)

`smb_integration_mount_unmount_cycles_leave_no_sockets_behind`
(`crates/cmdr-smb/src/volume/session_integration_test.rs`, in the `desktop-rust-integration-tests` lane) runs 10
mount/unmount cycles against the Docker `smb-consumer-guest` fixture. Each cycle connects the share the way a mount
does, lists its root, opens a scan session, then runs `on_unmount` and drops the volume. The cell reads the process's
own sockets with `getpeername` over every fd, keeping only peers on the fixture's port, so every socket it counts is one
Cmdr opened to that server.

- Result (`smb2` 0.27.0, macOS 27.0, nextest, 2026-10-01): each mount held 6 sockets while scanning (the main session,
  the watcher's, and four scan-pool members), never more, so no cycle inherited a socket from the one before. After the
  last unmount, 0 were left. 60 opened, 60 closed, and the whole run took about 1.3 s.
- The cell catches the regression: leaking the volume instead of unmounting it (`mem::forget`) left the sockets open,
  and the cell timed out waiting for them to close.
- Not a dev-instance run with `lsof`. The cell drives the same `connect_smb_volume` / `on_unmount` pair the app's mount
  and unmount paths call, minus the app's `VolumeManager` registry around them.

## Left out on purpose

`SmbClient::close()` (LOGOFF, then drop). With B fixed, dropping the client is the close in the common case, and
`mark_dead` force-closes even while clones live elsewhere. A real `close()` needs a "closed for good" state that stops
an armed reviver (a `Watcher` clone calling `reconnect_if_needed`) from bringing it back, and a decision on durable
opens, which a TCP drop keeps reclaimable and LOGOFF throws away. Those are design calls, not a patch-release rider.
