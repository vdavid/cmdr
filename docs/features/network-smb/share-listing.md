# Share listing

How Cmdr lists available shares on a network host.

**Related ADR**: [ADR 013: Use smb-rs for SMB share enumeration](../../artifacts/adr/013-smb-rs-share-listing.md)

## Overview

When a user selects a network host (like "Office"), the app enumerates what shares that host offers:

- `Documents`
- `Media`
- `TimeMachine`
- etc.

## Implementation: `smb-rs` crate

We use the [`smb` crate](https://crates.io/crates/smb) (smb-rs on GitHub) - a pure Rust SMB2/3 client.

### Why smb-rs?

| Criteria             | smb-rs                                                  |
| -------------------- | ------------------------------------------------------- |
| **License**          | MIT ✅ - compatible with AGPL and allows dual-licensing |
| **Dependencies**     | Pure Rust - no C libraries                              |
| **Protocol support** | SMB 2.X & 3.X dialects                                  |
| **Auth**             | NTLM & Kerberos via `sspi` crate                        |
| **Async**            | Native tokio support                                    |
| **Distribution**     | No bundling issues - just Rust code                     |

### API usage

```rust
use smb::{Client, ClientConfig, UncPath};
use std::str::FromStr;

async fn list_shares(server: &str, credentials: Option<Credentials>) -> Result<Vec<ShareInfo>, Error> {
    let client = Client::new(ClientConfig::default());

    // Connect to the server (not a specific share yet)
    let target = UncPath::from_str(&format!(r"\\{}", server))?;

    match credentials {
        Some(creds) => {
            client.connect(&target, &creds.username, creds.password).await?;
        }
        None => {
            // Guest access
            client.connect_guest(&target).await?;
        }
    }

    // Enumerate shares (implementation depends on smb-rs API)
    let shares = client.list_shares().await?;

    Ok(shares.into_iter().map(|s| ShareInfo {
        name: s.name,
        share_type: s.share_type, // disk, printer, etc.
    }).collect())
}
```

> **Note**: The exact API may differ—verify against actual `smb` crate documentation during implementation.

### Share types

SMB shares have different types:

| Type                      | Description                 | Show in UI?                           |
| ------------------------- | --------------------------- | ------------------------------------- |
| Disk                      | File share                  | ✅ Yes                                |
| Printer                   | Printer share               | ❌ No (not relevant for file manager) |
| IPC$                      | Inter-process communication | ❌ No (system internal)               |
| Admin shares (C$, ADMIN$) | Hidden admin shares         | ❌ No (hidden by convention)          |

Filter to show only disk shares by default.

## Authentication flow

Share enumeration may require authentication:

1. **Try guest first**: Attempt anonymous enumeration
2. **If auth required**: Check Keychain for stored credentials
3. **If found**: Use stored credentials
4. **If not found or failed**: Prompt user (see [authentication.md](./authentication.md))

### Auth mode detection

When probing a share, we detect what authentication is available:

```rust
enum AuthMode {
    GuestAllowed,   // Guest access works; credentials might also work
    CredsRequired,  // Guest failed; must have credentials
}
```

**How detection works:**

1. Attempt guest access to the share
2. If succeeds → `GuestAllowed`
3. If fails with auth error → `CredsRequired`

**Note**: We can't distinguish "guest only" from "guest or credentials" without trying credentials. When guest works, we
assume credentials might also work and offer a "Sign in for more access" option in the UI.

## Connection pooling

Maintain a pool of smb-rs `Client` instances to avoid reconnection overhead:

```rust
struct ConnectionPool {
    clients: HashMap<String, PooledClient>,
}

struct PooledClient {
    client: Client,
    last_used: Instant,
}
```

**Pool behavior:**

- **On connect**: Check pool for existing connection to server
- **Cache hit**: Reuse existing client
- **Cache miss**: Create new client, add to pool
- **TTL**: Remove clients after 60 seconds of inactivity
- **Max size**: Limit to 20 pooled connections

This makes subsequent operations (list shares, probe auth) much faster.

## Prefetching

To improve perceived performance, prefetch share information before user navigates:

### On hover (500ms debounce)

When user hovers over a network host in volume selector:

1. Start probing that host
2. List shares
3. For each share, detect auth mode (try guest access)
4. Cache results

### For known hosts

After discovery settles, prefetch shares for hosts in `knownNetworkShares`:

- Run in parallel (cap at 10 concurrent)
- Use stored credentials if available
- Cache results for instant display

### Unknown hosts

Don't probe unknown hosts automatically—only on user hover or navigation. This avoids creating scanning-like behavior
across the network.

## Encryption

smb-rs supports SMB 3.0+ encryption. During connection negotiation:

- Client and server agree on highest supported protocol
- If both support SMB 3.0+, encryption is available
- We use whatever encryption the server supports (don't force it, to maintain compatibility)

## Error handling

| Error                | User message                       | Action                       |
| -------------------- | ---------------------------------- | ---------------------------- |
| Host unreachable     | "Can't connect to Office"          | Check network connection     |
| Connection timeout   | "Connection timed out"             | Retry or check server        |
| Auth required        | (Show login form)                  | See auth flow                |
| Access denied        | "Access denied"                    | Check permissions with admin |
| SMB version mismatch | "Server uses unsupported protocol" | May need fallback            |

## Fallback for edge cases

If `smb-rs` fails for specific servers (old SMB1-only servers, quirky implementations), we can offer a fallback to
`smbutil view -g //server`. This should be rare but provides a safety net.

```rust
async fn list_shares_with_fallback(server: &str) -> Result<Vec<ShareInfo>, Error> {
    match list_shares_smb_rs(server).await {
        Ok(shares) => Ok(shares),
        Err(e) if e.is_protocol_error() => {
            log::warn!("smb-rs failed, trying smbutil fallback: {}", e);
            list_shares_smbutil(server).await
        }
        Err(e) => Err(e),
    }
}
```

## UX considerations

### Loading state

Share enumeration can take 1–10 seconds depending on network and server. Show appropriate feedback:

```
┌─────────────────────────────────────────────────────────────────┐
│ 📁 TestServer                                                   │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ⏳ Connecting to server...                                    │
│                                                                 │
│   [Cancel]                                                      │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Timeout

Set a reasonable timeout (10–15 seconds) with user option to cancel. Don't let the UI hang indefinitely.

### Caching

Cache share lists briefly (~30 seconds) to avoid re-querying when navigating back and forth. Invalidate on:

- User explicitly refreshes
- Authentication changes
- Enough time passes

## Testing

### Unit tests

- Test share type filtering (only show disk shares)
- Test error mapping from SMB errors to user messages
- Test guest vs authenticated enumeration logic

### Integration tests

- Mock SMB server responses for consistent testing
- Test timeout handling
- Test fallback trigger conditions

### Validation spike

Before full implementation, validate `smb-rs` works with actual servers:

- [ ] Test against macOS file sharing
- [ ] Test against Synology/QNAP NAS
- [ ] Test against Windows share
- [ ] Test against Linux Samba
- [ ] Test guest access
- [ ] Test authenticated access
- [ ] Measure latency
