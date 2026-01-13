# License activation system spec

This document specifies the license activation system that enables machine limits (max N machines per license key).

## Goals

1. **Machine limits**: Allow users to activate license on max 2 machines
2. **Activation tracking**: Know which licenses are active on which machines
3. **Self-service deactivation**: Users can deactivate old machines to free up slots
4. **Minimal infrastructure**: Use existing Cloudflare Worker + D1 database

## Relation to other specs

This spec shares the **machine ID generation code** with the [trial persistence spec](trial-persistence-spec.md). 

**Recommended development order:**
1. **Trial persistence first** — simpler, no server component, machine ID code is reused
2. **Activation system second** — builds on machine ID, adds server-side tracking

The machine ID implementation in both specs is identical and should live in `src/licensing/machine_id.rs`.

## Architecture overview

```
┌────────────────────────────────────────────────────────────────────────┐
│                         Activation flow                                 │
├────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. User enters license key in Cmdr app                                │
│           ↓                                                             │
│  2. App validates signature locally (existing, fast, offline)          │
│           ↓                                                             │
│  3. App generates machine ID (hash of hardware identifiers)            │
│           ↓                                                             │
│  4. App calls: POST /activate { licenseKey, machineId, machineName }   │
│           ↓                                                             │
│  5. Server checks: How many activations for this license?              │
│           ↓                                                             │
│  6a. If < maxActivations: Record activation, return success            │
│  6b. If >= maxActivations: Return error with list of active machines   │
│           ↓                                                             │
│  7. App stores activation token locally                                │
│                                                                         │
└────────────────────────────────────────────────────────────────────────┘
```

## Database schema (Cloudflare D1)

### Table: `activations`

| Column | Type | Description |
|--------|------|-------------|
| `id` | TEXT PRIMARY KEY | UUID |
| `license_key_hash` | TEXT NOT NULL | SHA-256 hash of the license key (not the key itself) |
| `machine_id` | TEXT NOT NULL | Hardware fingerprint hash |
| `machine_name` | TEXT | Human-readable name (e.g., "David's MacBook Pro") |
| `activated_at` | TEXT NOT NULL | ISO 8601 timestamp |
| `last_seen_at` | TEXT NOT NULL | Updated on each validation |
| `deactivated_at` | TEXT NULL | Set when user deactivates |

**Indexes:**
- `idx_license_key_hash` on `license_key_hash`
- `idx_machine_id` on `machine_id`
- `UNIQUE(license_key_hash, machine_id)` - prevent duplicate activations

### Table: `licenses` (optional, for analytics)

| Column | Type | Description |
|--------|------|-------------|
| `key_hash` | TEXT PRIMARY KEY | SHA-256 hash |
| `email` | TEXT | From license data |
| `created_at` | TEXT | When license was generated |
| `max_activations` | INTEGER DEFAULT 2 | Activation limit |

## API endpoints

### POST `/activate`

Activate a license on a machine.

**Request:**
```json
{
    "licenseKey": "base64payload.base64signature",
    "machineId": "sha256:abc123...",
    "machineName": "David's MacBook Pro"
}
```

**Response (success):**
```json
{
    "status": "activated",
    "activationId": "uuid",
    "activationsUsed": 1,
    "maxActivations": 2
}
```

**Response (already activated on this machine):**
```json
{
    "status": "already_activated",
    "activationId": "uuid",
    "activationsUsed": 1,
    "maxActivations": 2
}
```

**Response (limit reached):**
```json
{
    "status": "limit_reached",
    "activationsUsed": 2,
    "maxActivations": 2,
    "activeDevices": [
        { "machineName": "David's MacBook Pro", "activatedAt": "2026-01-01T..." },
        { "machineName": "David's Mac Mini", "activatedAt": "2026-01-02T..." }
    ]
}
```

### POST `/deactivate`

Deactivate a license on the current machine.

**Request:**
```json
{
    "licenseKey": "base64payload.base64signature",
    "machineId": "sha256:abc123..."
}
```

**Response:**
```json
{
    "status": "deactivated",
    "activationsUsed": 1,
    "maxActivations": 2
}
```

### GET `/activations`

List all activations for a license (for user to see their devices).

**Request headers:**
```
Authorization: Bearer <license_key>
```

**Response:**
```json
{
    "activations": [
        {
            "machineId": "sha256:abc...",
            "machineName": "David's MacBook Pro",
            "activatedAt": "2026-01-01T...",
            "lastSeenAt": "2026-01-09T...",
            "isCurrent": true
        }
    ],
    "maxActivations": 2
}
```

## Machine ID generation

The machine ID must be:
- **Stable**: Same ID after reboots, app updates
- **Unique**: Different between machines
- **Unpredictable**: Can't be easily guessed/spoofed

### macOS implementation (Rust)

```rust
use std::process::Command;

/// Generate a stable machine ID from hardware identifiers.
/// Uses IOPlatformUUID which is stable and unique per Mac.
pub fn get_machine_id() -> String {
    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .expect("Failed to execute ioreg");
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse IOPlatformUUID from output
    // Format: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
    let uuid = stdout
        .lines()
        .find(|line| line.contains("IOPlatformUUID"))
        .and_then(|line| {
            line.split('"')
                .nth(3)  // Get the value after the second quote
        })
        .unwrap_or("unknown");
    
    // Hash it so we don't send raw hardware ID
    use sha2::{Sha256, Digest};
    let mut hasher = Sha256::new();
    hasher.update(uuid.as_bytes());
    hasher.update(b"cmdr-machine-id-salt-v1");  // Versioned salt
    let hash = hasher.finalize();
    
    format!("sha256:{}", hex::encode(hash))
}

/// Get a human-readable machine name.
pub fn get_machine_name() -> String {
    let output = Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok();
    
    output
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Unknown Mac".to_string())
}
```

### Crates needed

Add to `Cargo.toml`:
```toml
sha2 = "0.10"
hex = "0.4"
```

## Implementation in license server (TypeScript)

### Database setup

Create `apps/license-server/src/db.ts`:

```typescript
import { D1Database } from '@cloudflare/workers-types'

export interface Activation {
    id: string
    license_key_hash: string
    machine_id: string
    machine_name: string | null
    activated_at: string
    last_seen_at: string
    deactivated_at: string | null
}

export async function initializeDb(db: D1Database) {
    await db.exec(`
        CREATE TABLE IF NOT EXISTS activations (
            id TEXT PRIMARY KEY,
            license_key_hash TEXT NOT NULL,
            machine_id TEXT NOT NULL,
            machine_name TEXT,
            activated_at TEXT NOT NULL,
            last_seen_at TEXT NOT NULL,
            deactivated_at TEXT,
            UNIQUE(license_key_hash, machine_id)
        );
        CREATE INDEX IF NOT EXISTS idx_license_key_hash ON activations(license_key_hash);
    `)
}

export async function getActiveActivations(db: D1Database, licenseKeyHash: string): Promise<Activation[]> {
    const result = await db.prepare(
        `SELECT * FROM activations 
         WHERE license_key_hash = ? AND deactivated_at IS NULL`
    ).bind(licenseKeyHash).all<Activation>()
    
    return result.results
}

export async function createActivation(
    db: D1Database,
    licenseKeyHash: string,
    machineId: string,
    machineName: string | null
): Promise<Activation> {
    const id = crypto.randomUUID()
    const now = new Date().toISOString()
    
    await db.prepare(
        `INSERT INTO activations (id, license_key_hash, machine_id, machine_name, activated_at, last_seen_at)
         VALUES (?, ?, ?, ?, ?, ?)
         ON CONFLICT(license_key_hash, machine_id) 
         DO UPDATE SET last_seen_at = ?, deactivated_at = NULL`
    ).bind(id, licenseKeyHash, machineId, machineName, now, now, now).run()
    
    return { id, license_key_hash: licenseKeyHash, machine_id: machineId, machine_name: machineName, activated_at: now, last_seen_at: now, deactivated_at: null }
}

export async function deactivate(db: D1Database, licenseKeyHash: string, machineId: string): Promise<boolean> {
    const result = await db.prepare(
        `UPDATE activations SET deactivated_at = ? 
         WHERE license_key_hash = ? AND machine_id = ? AND deactivated_at IS NULL`
    ).bind(new Date().toISOString(), licenseKeyHash, machineId).run()
    
    return result.meta.changes > 0
}
```

### Wrangler config update

Add D1 binding to `wrangler.toml`:

```toml
[[d1_databases]]
binding = "DB"
database_name = "cmdr-licenses"
database_id = "<your-d1-database-id>"  # From `wrangler d1 create cmdr-licenses`
```

### Setting up D1

```bash
# Create the database
wrangler d1 create cmdr-licenses

# Copy the database_id to wrangler.toml

# Run migrations (create a migrations folder)
wrangler d1 migrations create cmdr-licenses init
# Edit the generated SQL file with the CREATE TABLE statement
wrangler d1 migrations apply cmdr-licenses
```

## Tauri app integration

### Activation flow in Rust

```rust
// In licensing/activation.rs

const MAX_ACTIVATIONS: usize = 2;
const LICENSE_SERVER_URL: &str = "https://cmdr-license-server.veszelovszki.workers.dev";

#[derive(Serialize)]
struct ActivationRequest {
    license_key: String,
    machine_id: String,
    machine_name: String,
}

#[derive(Deserialize)]
struct ActivationResponse {
    status: String,
    activation_id: Option<String>,
    activations_used: usize,
    max_activations: usize,
    active_devices: Option<Vec<ActiveDevice>>,
}

pub async fn activate_license(license_key: &str) -> Result<ActivationResponse, String> {
    // 1. Validate signature locally first (fast, offline)
    validate_license_key(license_key)?;
    
    // 2. Get machine identifiers
    let machine_id = get_machine_id();
    let machine_name = get_machine_name();
    
    // 3. Call activation server
    let client = reqwest::Client::new();
    let response = client
        .post(format!("{}/activate", LICENSE_SERVER_URL))
        .json(&ActivationRequest {
            license_key: license_key.to_string(),
            machine_id,
            machine_name,
        })
        .send()
        .await
        .map_err(|e| format!("Network error: {}", e))?;
    
    let result: ActivationResponse = response
        .json()
        .await
        .map_err(|e| format!("Invalid response: {}", e))?;
    
    match result.status.as_str() {
        "activated" | "already_activated" => {
            // Store activation locally
            save_activation_token(&result.activation_id.unwrap())?;
            Ok(result)
        }
        "limit_reached" => {
            Err(format!(
                "License already active on {} devices. Deactivate one to continue.",
                result.activations_used
            ))
        }
        _ => Err("Unknown activation status".to_string())
    }
}
```

### Crates needed

Add to `Cargo.toml`:
```toml
reqwest = { version = "0.11", features = ["json"] }
```

## Offline handling

The app should work offline for users who have already activated:

1. **First activation**: Requires network
2. **Subsequent launches**: Check local activation token, no network needed
3. **Periodic re-validation**: Every 30 days, try to phone home to update `last_seen_at`
4. **Grace period**: If re-validation fails, allow 14 days before requiring network

```rust
pub fn check_activation_status() -> ActivationStatus {
    let local_token = load_activation_token();
    
    match local_token {
        Some(token) if token.validated_at > (now() - 30.days()) => {
            ActivationStatus::Valid
        }
        Some(token) if token.validated_at > (now() - 37.days()) => {
            // Try to re-validate in background, but allow usage
            spawn_revalidation_task();
            ActivationStatus::ValidNeedsRevalidation
        }
        Some(_) => {
            ActivationStatus::NeedsRevalidation
        }
        None => {
            ActivationStatus::NotActivated
        }
    }
}
```

## Security considerations

1. **Hash license keys**: Never store raw license keys in the database
2. **Rate limiting**: Add rate limiting to prevent brute-force attacks
3. **Machine ID spoofing**: Accept that determined users can spoof; this is about friction, not DRM
4. **HTTPS only**: All API calls over HTTPS

## References

- [Cloudflare D1 docs](https://developers.cloudflare.com/d1/)
- [Cloudflare D1 with Hono](https://hono.dev/getting-started/cloudflare-workers#bindings)
- [macOS IOPlatformUUID](https://developer.apple.com/documentation/iokit)
- [Ed25519 dalek (Rust)](https://docs.rs/ed25519-dalek/latest/ed25519_dalek/)

## Implementation checklist

- [ ] Create D1 database: `wrangler d1 create cmdr-licenses`
- [ ] Add database binding to `wrangler.toml`
- [ ] Create and apply migrations
- [ ] Add `/activate` endpoint to license server
- [ ] Add `/deactivate` endpoint to license server
- [ ] Add `/activations` endpoint to license server
- [ ] Add `sha2` and `hex` crates to desktop app
- [ ] Implement `get_machine_id()` in Rust
- [ ] Implement `get_machine_name()` in Rust
- [ ] Add `reqwest` crate for HTTP calls
- [ ] Implement `activate_license()` command in Tauri
- [ ] Add activation UI to frontend (show devices, deactivation option)
- [ ] Handle offline scenarios
- [ ] Add rate limiting to API endpoints
