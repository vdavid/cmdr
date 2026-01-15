# Host discovery

How Cmdr discovers SMB hosts on the local network.

## Overview

Network hosts are discovered automatically using **Bonjour** (Apple's implementation of mDNS/DNS-SD). When an
SMB-capable device advertises itself on the local network, it appears in the Network browser view.

## How Bonjour works

Bonjour uses multicast DNS (mDNS) to discover services on the local network without requiring a central directory:

1. **Advertising**: Devices with SMB file sharing broadcast: _"I'm `TestServer` and I have SMB on port 445"_
2. **Listening**: Our app listens for these broadcasts on multicast address `224.0.0.251`
3. **Resolution**: When a service is found, we resolve its hostname/IP to connect

### Service type

SMB shares advertise as `_smb._tcp.local`:

- `_smb` — the service (SMB file sharing)
- `_tcp` — the protocol (TCP)
- `.local` — the local network domain

## Implementation

### macOS APIs

We use Apple's `NSNetServiceBrowser` and `NSNetService` classes from Foundation framework, accessed via
`objc2-foundation`:

```
NSNetServiceBrowser (created at app startup)
    │
    ▼
searchForServicesOfType("_smb._tcp.", inDomain: "local.")
    │
    ▼
Delegate receives callbacks as services appear/disappear
    │
    ▼
NSNetService objects (one per discovered host)
    │
    ▼
hostname/IP derived from service name (lazy resolution)
```

### Challenges

| Challenge              | Solution                                                                                                 |
| ---------------------- | -------------------------------------------------------------------------------------------------------- |
| **Delegate pattern**   | Create a Rust struct that implements `NSNetServiceBrowserDelegate` using `objc2`'s `define_class!` macro |
| **Async/streaming**    | Services appear over time; use Tauri events to propagate updates                                         |
| **Service resolution** | Hostname derived from service name, IP resolved via DNS on hover/demand                                  |
| **Run loop**           | Bonjour callbacks require a run loop; use Tauri's main thread                                            |

### Lifecycle

1. **App startup**: Start `NSNetServiceBrowser` listening for `_smb._tcp.local`
2. **Continuous**: Receive callbacks as hosts appear/disappear on network
3. **Cache results**: Keep discovered hosts in memory for instant display
4. **Network view opened**: Show cached hosts immediately; continue updating as new hosts found

### Lazy resolution

Resolution (getting IP/hostname) is deferred until needed to keep discovery fast:

- **On discovery**: Store host name only (from mDNS announcement)
- **On hover**: Resolve that specific host's hostname and IP
- **On navigate**: Resolve if not already cached
- **Cache**: Keep resolved addresses in memory

This way the network browser populates instantly with host names, and resolution happens just-in-time.

### Host disappearance

Bonjour proactively notifies when hosts stop advertising (via `netServiceBrowser:didRemoveService:`).

**UI behavior when a host disappears:**

- **In Network browser**: Remove host from list, adjust cursor position
- **During connection**: Show appropriate error if mid-operation

## UX behavior

### Volume selector

The volume selector shows a single "Network" item under the Network section:

```
📁 Favorites
   └── Documents
   └── Downloads
📁 Macintosh HD
📁 External Drive
🌐 Network ← Click to open Network browser
```

### Network browser view

When user clicks "Network" in the volume selector, the pane switches to the Network browser view:

```
┌────────────────────────────────────────────────────────────┐
│ Name              │ IP address     │ Hostname      │ Status │
├────────────────────────────────────────────────────────────┤
│ 🖥️ David's MacBook │ 192.168.1.10  │ macbook.local │   —    │
│ 🖥️ NAS-Server      │ 192.168.1.50  │ nas.local     │   —    │
│ 🖥️ Office-PC       │ —             │ —             │   —    │
│                                                            │
│ Searching... (if discovery in progress)                    │
└────────────────────────────────────────────────────────────┘
```

**Columns:**

- **Name**: Host's advertised name from Bonjour
- **IP address**: Resolved IP (on hover/demand), or "—" if not yet resolved
- **Hostname**: Derived `.local` hostname
- **Shares**: Share count (future, currently "—")
- **Status**: Connection status (future, currently "—")

**Keyboard navigation:**

- Arrow Up/Down: Navigate between hosts
- Enter: Select host (future: show shares)
- Backspace: No-op (can't go "up" from network root)

**Back/Forward navigation:**

- Works as expected: going Back returns to previous file view
- Coming Forward again returns to Network browser

## Network change detection

Bonjour automatically handles network changes:

| Event                   | Detection        | Action           |
| ----------------------- | ---------------- | ---------------- |
| Host starts advertising | Bonjour callback | Add to list      |
| Host stops advertising  | Bonjour callback | Remove from list |

## What Bonjour doesn't cover

Not all devices advertise via Bonjour:

| Scenario                      | Bonjour finds it?        |
| ----------------------------- | ------------------------ |
| Mac with File Sharing enabled | ✅ Yes                   |
| NAS devices (Synology, QNAP)  | ✅ Yes (usually)         |
| Windows with mDNS enabled     | ✅ Yes                   |
| Old Windows / no mDNS         | ❌ No                    |
| Enterprise servers            | ❌ Often no              |
| VPN-connected servers         | ❌ No (different subnet) |

**Future work**: Add "Connect to server..." option for manual server entry (see [index.md](./index.md#future-work)).

## Performance

| Metric          | Value                                    |
| --------------- | ---------------------------------------- |
| CPU usage       | Negligible (passive multicast listening) |
| Network traffic | Minimal (small UDP packets)              |
| Side effects    | None (read-only discovery)               |
| Startup impact  | Near-zero (async, non-blocking)          |

Discovery is lightweight enough to start immediately at app launch.

## Testing

### Backend unit tests

- `test_service_name_to_hostname`: Tests hostname derivation from service names
- `test_service_name_to_id`: Tests ID generation from service names
- `test_network_host_serialization`: Tests JSON serialization of NetworkHost

### Frontend tests

- `network-hosts.test.ts`: Tests network host type interfaces and event handling logic
- Integration tests verify NetworkBrowser renders correctly and handles events
