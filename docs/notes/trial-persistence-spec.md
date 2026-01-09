# Trial persistence spec

This document specifies how to persist the trial start date across app reinstalls using hardware fingerprinting and macOS Keychain.

## Goals

1. **Survive reinstalls**: Trial countdown continues even if user deletes and reinstalls the app
2. **Work offline**: No network required to check trial status
3. **Simple implementation**: Use existing macOS Keychain infrastructure
4. **Good UX**: New Mac = fresh trial (acceptable, they bought new hardware)

## Relation to other specs

This spec shares the **machine ID generation code** with the [activation system spec](./license-activation-spec.md). Implement this spec first since:
1. It's simpler (no server-side component)
2. The machine ID code can be reused by the activation system
3. Trial is the first thing users experience

## Current implementation

Currently, trial data is stored via `tauri-plugin-store` in:
```
~/Library/Application Support/com.veszelovszki.cmdr/license.json
```

**Problem**: This is deleted when user removes app data or reinstalls.

## New implementation

Store trial start date in **macOS Keychain**, keyed by a hardware fingerprint.

```
┌────────────────────────────────────────────────────────────────────────┐
│                         Trial check flow                                │
├────────────────────────────────────────────────────────────────────────┤
│                                                                         │
│  1. App starts                                                         │
│           ↓                                                             │
│  2. Check for valid license (existing code) → if valid, skip trial     │
│           ↓                                                             │
│  3. Generate machine ID (hash of IOPlatformUUID)                       │
│           ↓                                                             │
│  4. Look up Keychain: "cmdr-trial-{machine_id_prefix}"                 │
│           ↓                                                             │
│  5a. Found → Parse timestamp, calculate days remaining                 │
│  5b. Not found → Store current timestamp, start 14-day trial           │
│           ↓                                                             │
│  6. Return trial status to frontend                                    │
│                                                                         │
└────────────────────────────────────────────────────────────────────────┘
```

## Machine ID generation

Shared code with [activation spec](./license-activation-spec.md#machine-id-generation).

```rust
// In src/licensing/machine_id.rs

use std::process::Command;
use sha2::{Sha256, Digest};

/// Generate a stable machine ID from hardware identifiers.
/// Uses IOPlatformUUID which is unique per Mac and survives reinstalls.
pub fn get_machine_id() -> String {
    let uuid = get_io_platform_uuid().unwrap_or_else(|| "unknown".to_string());
    
    // Hash so we don't expose raw hardware ID
    let mut hasher = Sha256::new();
    hasher.update(uuid.as_bytes());
    hasher.update(b"cmdr-machine-id-salt-v1");  // Versioned salt
    let hash = hasher.finalize();
    
    format!("sha256:{}", hex::encode(hash))
}

/// Get a short prefix for Keychain key names.
/// Full hash is 64 chars, we only need ~16 for uniqueness.
pub fn get_machine_id_prefix() -> String {
    let full_id = get_machine_id();
    // Take first 16 chars after "sha256:"
    full_id.chars().skip(7).take(16).collect()
}

/// Get the raw IOPlatformUUID from macOS.
fn get_io_platform_uuid() -> Option<String> {
    let output = Command::new("ioreg")
        .args(["-rd1", "-c", "IOPlatformExpertDevice"])
        .output()
        .ok()?;
    
    let stdout = String::from_utf8_lossy(&output.stdout);
    
    // Parse: "IOPlatformUUID" = "XXXXXXXX-XXXX-XXXX-XXXX-XXXXXXXXXXXX"
    stdout
        .lines()
        .find(|line| line.contains("IOPlatformUUID"))
        .and_then(|line| line.split('"').nth(3))
        .map(|s| s.to_string())
}

/// Get a human-readable machine name for display.
pub fn get_machine_name() -> String {
    Command::new("scutil")
        .args(["--get", "ComputerName"])
        .output()
        .ok()
        .and_then(|o| String::from_utf8(o.stdout).ok())
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|| "Unknown Mac".to_string())
}
```

### Crates needed

Add to `apps/desktop/src-tauri/Cargo.toml`:
```toml
sha2 = "0.10"
hex = "0.4"
```

## Keychain storage

Use the existing `security-framework` crate (already in dependencies for SMB credentials).

```rust
// In src/licensing/trial.rs

use security_framework::passwords::{get_generic_password, set_generic_password, delete_generic_password};
use std::time::{SystemTime, UNIX_EPOCH};

const TRIAL_DAYS: u64 = 14;
const KEYCHAIN_SERVICE: &str = "com.veszelovszki.cmdr";

/// Get the Keychain account name for trial data.
fn trial_account_name() -> String {
    let prefix = super::machine_id::get_machine_id_prefix();
    format!("trial-{}", prefix)
}

/// Get trial status, initializing if needed.
pub fn get_trial_status() -> TrialStatus {
    let account = trial_account_name();
    
    match get_generic_password(KEYCHAIN_SERVICE, &account) {
        Ok(data) => {
            // Parse stored timestamp
            let timestamp_str = String::from_utf8_lossy(&data);
            let start_timestamp: u64 = timestamp_str.parse().unwrap_or(0);
            
            if start_timestamp == 0 {
                // Corrupted data, reset trial
                return initialize_trial();
            }
            
            calculate_trial_status(start_timestamp)
        }
        Err(_) => {
            // No trial record found, initialize
            initialize_trial()
        }
    }
}

/// Initialize a new trial period.
fn initialize_trial() -> TrialStatus {
    let now = current_timestamp();
    let account = trial_account_name();
    
    // Store in Keychain
    let _ = set_generic_password(
        KEYCHAIN_SERVICE,
        &account,
        now.to_string().as_bytes(),
    );
    
    TrialStatus::Active {
        days_remaining: TRIAL_DAYS,
        days_used: 0,
        total_days: TRIAL_DAYS,
    }
}

/// Calculate trial status from start timestamp.
fn calculate_trial_status(start_timestamp: u64) -> TrialStatus {
    let now = current_timestamp();
    let elapsed_secs = now.saturating_sub(start_timestamp);
    let days_used = elapsed_secs / 86400;
    
    if days_used < TRIAL_DAYS {
        TrialStatus::Active {
            days_remaining: TRIAL_DAYS - days_used,
            days_used,
            total_days: TRIAL_DAYS,
        }
    } else {
        TrialStatus::Expired
    }
}

fn current_timestamp() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}

#[derive(Debug, Clone, serde::Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TrialStatus {
    Active {
        days_remaining: u64,
        days_used: u64,
        total_days: u64,
    },
    Expired,
}
```

## Migration from current implementation

Users who already started a trial with the old system should have their trial honored.

```rust
/// Migrate trial data from old store to Keychain if needed.
pub fn migrate_trial_if_needed(app: &tauri::AppHandle) {
    use tauri_plugin_store::StoreExt;
    
    let account = trial_account_name();
    
    // Check if already migrated (Keychain has data)
    if get_generic_password(KEYCHAIN_SERVICE, &account).is_ok() {
        return; // Already migrated
    }
    
    // Check old store for trial data
    if let Ok(store) = app.store("license.json") {
        if let Some(value) = store.get("first_run_timestamp") {
            if let Some(old_timestamp) = value.as_u64() {
                // Migrate to Keychain
                let _ = set_generic_password(
                    KEYCHAIN_SERVICE,
                    &account,
                    old_timestamp.to_string().as_bytes(),
                );
                // Optionally delete from old store
                store.delete("first_run_timestamp");
            }
        }
    }
}
```

## Updated app status check

```rust
// In src/licensing/mod.rs

pub fn get_app_status(app: &tauri::AppHandle) -> AppStatus {
    // 1. Check for valid license first
    if let Some(license) = verification::get_license_info(app) {
        return AppStatus::Licensed { email: license.email };
    }
    
    // 2. Migrate old trial data if needed
    trial::migrate_trial_if_needed(app);
    
    // 3. Get trial status from Keychain
    match trial::get_trial_status() {
        trial::TrialStatus::Active { days_remaining, days_used, total_days } => {
            AppStatus::Trial(TrialInfo { days_remaining, days_used, total_days })
        }
        trial::TrialStatus::Expired => {
            AppStatus::TrialExpired
        }
    }
}
```

## File structure

```
apps/desktop/src-tauri/src/licensing/
├── mod.rs           # Module entry, AppStatus enum, get_app_status()
├── machine_id.rs    # NEW: Machine ID generation (shared with activation)
├── trial.rs         # UPDATED: Keychain-based trial tracking
└── verification.rs  # License signature verification (unchanged)
```

## Reset trial (debug only)

For testing, allow resetting trial in debug builds:

```rust
#[cfg(debug_assertions)]
pub fn reset_trial() {
    let account = trial_account_name();
    let _ = delete_generic_password(KEYCHAIN_SERVICE, &account);
}
```

## Security considerations

1. **Keychain access**: Only this app can read its Keychain entries
2. **Machine ID salt**: Versioned salt prevents rainbow table attacks
3. **Not foolproof**: Determined users can clear Keychain, but that's acceptable friction

## Implementation checklist

- [ ] Add `sha2` and `hex` crates to Cargo.toml
- [ ] Create `src/licensing/machine_id.rs` with ID generation
- [ ] Update `src/licensing/trial.rs` to use Keychain
- [ ] Add migration logic for existing users
- [ ] Update `src/licensing/mod.rs` to use new trial module
- [ ] Add `reset_trial` command (debug only)
- [ ] Test: Fresh install → trial starts
- [ ] Test: Reinstall app → trial continues
- [ ] Test: Trial expires → shows expired state
- [ ] Test: Migration from old store works

## Testing

```bash
# Check current Keychain entries (for debugging)
security find-generic-password -s "com.veszelovszki.cmdr" -a "trial-*"

# Delete trial entry (to reset for testing)
security delete-generic-password -s "com.veszelovszki.cmdr" -a "trial-abc123..."
```
