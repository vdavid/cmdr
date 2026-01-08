# Known shares store

How Cmdr persists network connection history.

## Overview

We store metadata about network shares the user has connected to. This enables:

- Pre-filling usernames in login forms
- Detecting when auth options change
- Showing contextual messages about connection history
- Quick reconnection to known shares

## Data structure

Stored in `settings.json` alongside other user preferences:

```json
{
    "knownNetworkShares": [
        {
            "serverName": "Alpha",
            "shareName": "Documents",
            "protocol": "smb",
            "lastConnectedAt": "2026-01-03T21:00:00Z",
            "lastConnectionMode": "credentials",
            "lastKnownAuthOptions": "guest_or_credentials",
            "username": "david"
        },
        {
            "serverName": "Bravo",
            "shareName": "media",
            "protocol": "smb",
            "lastConnectedAt": "2026-01-02T15:30:00Z",
            "lastConnectionMode": "guest",
            "lastKnownAuthOptions": "guest_only",
            "username": null
        }
    ]
}
```

### Field definitions

| Field                  | Type     | Description                                                       |
| ---------------------- | -------- | ----------------------------------------------------------------- |
| `serverName`           | string   | Hostname or IP of the server                                      |
| `shareName`            | string   | Name of the specific share                                        |
| `protocol`             | `"smb"`  | Protocol type (future: `"afp"`, `"nfs"`)                          |
| `lastConnectedAt`      | ISO 8601 | When we last successfully connected                               |
| `lastConnectionMode`   | enum     | `"guest"` or `"credentials"`                                      |
| `lastKnownAuthOptions` | enum     | `"guest_only"`, `"credentials_only"`, or `"guest_or_credentials"` |
| `username`             | string?  | Username used (null for guest)                                    |

### Storage location

Same as other settings: `~/Library/Application Support/com.veszelovszki.cmdr/settings.json`

## UX scenarios

### Scenario 1: Auth options changed (guest → now can auth)

```
Previous: { lastKnownAuthOptions: "guest_only" }
Current:  Server now offers credentials

→ Show login form with message:
  "You connected as guest before. You can now sign in for more access."
→ Guest option pre-selected
```

### Scenario 2: Auth options changed (had creds → now guest only)

```
Previous: { lastKnownAuthOptions: "guest_or_credentials", lastConnectionMode: "credentials" }
Current:  Server now only allows guest

→ Show message:
  "This share now only allows guest access. Your previous credentials won't be used."
→ Connect as guest automatically
```

### Scenario 3: Username pre-fill

```
Previous: { username: "david" }
Now:      User connects again, credentials required

→ Pre-fill username field with "david"
→ User only needs to enter password
```

### Scenario 4: Quick reconnect

In the future, we could show "Recent connections" in the volume selector:

```
📶 Network
   └── TestServer (discovered)
   └── PI (discovered)
📜 Recent
   └── TestServer / Documents ← From known shares
   └── PI / media
```

## TypeScript types (frontend)

```typescript
type ConnectionMode = 'guest' | 'credentials'

type AuthOptions = 'guest_only' | 'credentials_only' | 'guest_or_credentials'

interface KnownNetworkShare {
    serverName: string
    shareName: string
    protocol: 'smb'
    lastConnectedAt: string // ISO 8601
    lastConnectionMode: ConnectionMode
    lastKnownAuthOptions: AuthOptions
    username: string | null
}

interface SettingsStore {
    // ... other settings ...
    knownNetworkShares: KnownNetworkShare[]
}
```

## Rust types (backend)

```rust
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum ConnectionMode {
    Guest,
    Credentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
enum AuthOptions {
    GuestOnly,
    CredentialsOnly,
    GuestOrCredentials,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct KnownNetworkShare {
    server_name: String,
    share_name: String,
    protocol: String, // "smb"
    last_connected_at: String, // ISO 8601
    last_connection_mode: ConnectionMode,
    last_known_auth_options: AuthOptions,
    username: Option<String>,
}
```

## Operations

### Add/update share

Called after successful connection:

```rust
fn update_known_share(settings: &mut Settings, share: KnownNetworkShare) {
    let existing = settings.known_network_shares
        .iter_mut()
        .find(|s| s.server_name == share.server_name && s.share_name == share.share_name);

    match existing {
        Some(s) => *s = share, // Update
        None => settings.known_network_shares.push(share), // Add
    }
}
```

### Lookup share

```rust
fn get_known_share(settings: &Settings, server: &str, share: &str) -> Option<&KnownNetworkShare> {
    settings.known_network_shares
        .iter()
        .find(|s| s.server_name == server && s.share_name == share)
}
```

## Security considerations

- **No passwords stored here** — passwords go in Keychain only
- **Username is not sensitive** — okay to store in plain JSON
- **Clear on logout** — if we add multi-user support, clear on user switch
