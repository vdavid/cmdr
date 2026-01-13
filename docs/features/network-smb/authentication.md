# Authentication

How Cmdr handles SMB authentication and Keychain integration.

**Related ADR**: [ADR 012: Custom auth UI with Keychain integration](../../artifacts/adr/012-custom-auth-ui-keychain.md)

## Overview

SMB shares may require authentication. We implement a custom login UI that integrates with macOS Keychain for credential
storage.

## Authentication modes

We detect two authentication states (see [share-listing.md](./share-listing.md#auth-mode-detection)):

| Mode              | Description          | How detected                          |
| ----------------- | -------------------- | ------------------------------------- |
| **GuestAllowed**  | Guest access works   | Probe succeeded with anonymous access |
| **CredsRequired** | Credentials required | Probe failed with auth error          |

**Note**: We can't distinguish "guest only" from "guest or credentials" at probe time. When guest works, we assume
credentials might also work and offer a "Sign in for more access" option.

## Flow by scenario

### Guest allowed (happy path)

```
User selects share
    → Auth mode is GuestAllowed
    → Mount as guest immediately
    → Browse files with "Sign in for more access" option visible
```

### Guest allowed with prior credentials

```
User selects share
    → Auth mode is GuestAllowed
    → Check if user previously used credentials (from knownNetworkShares)
    → If yes: Try credentials first (may have more permissions)
    → If credentials fail or not stored: Fall back to guest
    → Browse files
```

### Credentials required (with stored creds)

```
User selects share
    → Auth mode is CredsRequired
    → Check Keychain for stored credentials
    → Found! Fetch and try mounting
    → Success → Browse files
```

### Credentials required (no stored creds or failed)

```
User selects share
    → Auth mode is CredsRequired
    → Check Keychain: nothing found (or stored creds failed)
    → Show login form IN THE PANE (replacing file list)
    → User enters credentials, optionally checks "Remember"
    → Try mounting
    → Success → Save to Keychain if requested → Browse files
```

## "Sign in for more access" UX

When connected as guest, show a subtle prompt in the file list header:

```
┌─────────────────────────────────────────────────────────────────┐
│ 📁 TestServer / Documents                  [Sign in for more →] │
├─────────────────────────────────────────────────────────────────┤
│ 📁 Folder1                                                      │
│ 📁 Folder2                                                      │
│ 📄 readme.txt                                                   │
└─────────────────────────────────────────────────────────────────┘
```

Clicking "Sign in for more access" shows the login form. This allows users who want more permissions to authenticate
without forcing everyone through a login flow.

## Login form UI

The login form appears **in the file pane**, replacing the directory listing:

```
┌─────────────────────────────────────────────────────────────────┐
│ 🔒 Sign in to "TestServer"                                      │
├─────────────────────────────────────────────────────────────────┤
│                                                                 │
│   ○ Connect as guest                    ← Only if guest allowed │
│   ● Sign in with credentials                                    │
│                                                                 │
│   Username: [david                                           ]  │
│   Password: [••••••••••                                      ]  │
│                                                                 │
│   ☑ Remember in Keychain                                        │
│                                                                 │
│                              [Cancel]  [Connect]                │
│                                                                 │
└─────────────────────────────────────────────────────────────────┘
```

### Contextual messaging

If the user previously connected and auth options changed, show a message:

- **Was guest, now can use credentials**: _"You connected as guest before. You can now sign in for more access."_

- **Was credentials, now guest-only**: _"This share now only allows guest access. Your previous credentials won't be
  used."_

## Keychain integration

We use macOS **Security.framework** to store/retrieve credentials:

### Storing credentials

```rust
use security_framework::passwords::set_generic_password;

fn save_credentials(server: &str, username: &str, password: &str) -> Result<(), Error> {
    set_generic_password(
        "Cmdr",           // Service name
        &format!("smb://{}", server), // Account (we use server URL)
        password.as_bytes(),
    )?;
    // Username stored separately or in account name
    Ok(())
}
```

### Retrieving credentials

```rust
use security_framework::passwords::get_generic_password;

fn get_credentials(server: &str) -> Option<Credentials> {
    let password = get_generic_password(
        "Cmdr",
        &format!("smb://{}", server),
    ).ok()?;

    // Retrieve username from our known shares store
    let username = get_username_from_store(server)?;

    Some(Credentials { username, password })
}
```

### Keychain item visibility

Credentials appear in Keychain Access.app under "Cmdr" entries, allowing users to view/edit/delete them independently of
our app.

## Known shares store

We track connection history in settings (see [known-shares-store.md](./known-shares-store.md)):

```json
{
    "knownNetworkShares": [
        {
            "name": "Office",
            "type": "smb",
            "lastConnected": "2026-01-03T21:00:00Z",
            "lastConnectionMode": "credentials",
            "lastKnownAuthOptions": "guest_or_credentials",
            "username": "david"
        }
    ]
}
```

This enables:

- Pre-filling username in login form
- Detecting auth option changes
- Showing "last connected" info

## Error handling

| Error                  | Message                        | Action                  |
| ---------------------- | ------------------------------ | ----------------------- |
| Invalid credentials    | "Invalid username or password" | Re-show form with error |
| Keychain access denied | (Fall back to prompting)       | Show login form         |
| Account locked         | "Account is locked"            | Show message, no retry  |

## Testing

### Unit tests

- Test credential validation logic
- Test auth option detection parsing
- Mock Keychain for credential storage/retrieval tests

### Integration tests

- Test login form appears when credentials required
- Test guest option visibility based on auth mode
- Test "Remember in Keychain" actually persists
