//! Known network shares store.
//!
//! Persists metadata about network shares the user has connected to.
//! Enables username pre-fill, auth change detection, and quick reconnect.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::Mutex;

/// Connection mode used for the last successful connection.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionMode {
    Guest,
    Credentials,
}

/// Authentication options available for a share.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AuthOptions {
    GuestOnly,
    CredentialsOnly,
    GuestOrCredentials,
}

/// Information about a known network share.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownNetworkShare {
    pub server_name: String,
    pub share_name: String,
    /// Currently only "smb".
    pub protocol: String,
    /// ISO 8601.
    pub last_connected_at: String,
    pub last_connection_mode: ConnectionMode,
    pub last_known_auth_options: AuthOptions,
    /// None for guest.
    pub username: Option<String>,
}

/// The known shares store, persisted to disk.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownSharesStore {
    #[serde(default)]
    pub known_network_shares: Vec<KnownNetworkShare>,
}

/// In-memory cache of known shares, synchronized with disk.
static KNOWN_SHARES: std::sync::OnceLock<Mutex<KnownSharesStore>> = std::sync::OnceLock::new();

fn get_known_shares_mutex() -> &'static Mutex<KnownSharesStore> {
    KNOWN_SHARES.get_or_init(|| Mutex::new(KnownSharesStore::default()))
}

/// Atomically writes content to a file using write-to-temp + rename.
/// On failure, the original file (if any) remains intact.
fn atomic_write_json(path: &Path, content: &str) -> std::io::Result<()> {
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, content)?;
    fs::rename(&tmp, path)?;
    Ok(())
}

/// Removes a stale `.tmp` file left over from a crash during atomic write.
fn cleanup_tmp_file(path: &Path) {
    let tmp = path.with_extension("json.tmp");
    if tmp.exists() {
        let _ = fs::remove_file(&tmp);
    }
}

/// Returns the path to the known shares store file.
fn get_store_path<R: tauri::Runtime>(app: &tauri::AppHandle<R>) -> Option<PathBuf> {
    crate::config::resolved_app_data_dir(app)
        .ok()
        .map(|dir| dir.join("known-shares.json"))
}

/// Loads known shares from disk into memory.
pub fn load_known_shares<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(path) = get_store_path(app) else {
        return;
    };

    cleanup_tmp_file(&path);

    let store = if let Ok(contents) = fs::read_to_string(&path) {
        serde_json::from_str(&contents).unwrap_or_default()
    } else {
        KnownSharesStore::default()
    };

    if let Ok(mut cache) = get_known_shares_mutex().lock() {
        *cache = store;
    }
}

/// Saves known shares from memory to disk.
fn save_known_shares<R: tauri::Runtime>(app: &tauri::AppHandle<R>) {
    let Some(path) = get_store_path(app) else {
        return;
    };

    let store = match get_known_shares_mutex().lock() {
        Ok(cache) => cache.clone(),
        Err(_) => return,
    };

    // Ensure parent directory exists
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }

    if let Ok(json) = serde_json::to_string_pretty(&store)
        && let Err(e) = atomic_write_json(&path, &json)
    {
        log::warn!("Couldn't write known shares store: {}", e);
    }
}

/// Creates a unique key for a share.
fn share_key(server_name: &str, share_name: &str) -> String {
    format!("{}/{}", server_name.to_lowercase(), share_name.to_lowercase())
}

/// Gets all known network shares.
pub fn get_all_known_shares() -> Vec<KnownNetworkShare> {
    get_known_shares_mutex()
        .lock()
        .map(|cache| cache.known_network_shares.clone())
        .unwrap_or_default()
}

/// Gets a specific known share by server and share name.
pub fn get_known_share(server_name: &str, share_name: &str) -> Option<KnownNetworkShare> {
    let key = share_key(server_name, share_name);
    get_known_shares_mutex()
        .lock()
        .ok()?
        .known_network_shares
        .iter()
        .find(|s| share_key(&s.server_name, &s.share_name) == key)
        .cloned()
}

/// Gets all known shares for a specific server.
#[allow(dead_code, reason = "Will be used when implementing quick reconnect UI")]
pub fn get_known_shares_for_server(server_name: &str) -> Vec<KnownNetworkShare> {
    let server_lower = server_name.to_lowercase();
    get_known_shares_mutex()
        .lock()
        .map(|cache| {
            cache
                .known_network_shares
                .iter()
                .filter(|s| s.server_name.to_lowercase() == server_lower)
                .cloned()
                .collect()
        })
        .unwrap_or_default()
}

/// Updates or adds a known network share.
/// Called after a successful connection.
pub fn update_known_share<R: tauri::Runtime>(app: &tauri::AppHandle<R>, share: KnownNetworkShare) {
    let key = share_key(&share.server_name, &share.share_name);

    if let Ok(mut cache) = get_known_shares_mutex().lock() {
        // Find and update, or add new
        if let Some(existing) = cache
            .known_network_shares
            .iter_mut()
            .find(|s| share_key(&s.server_name, &s.share_name) == key)
        {
            *existing = share;
        } else {
            cache.known_network_shares.push(share);
        }
    }

    save_known_shares(app);
}

/// Removes a known network share.
#[allow(dead_code, reason = "Will be used when implementing share removal UI")]
pub fn remove_known_share<R: tauri::Runtime>(app: &tauri::AppHandle<R>, server_name: &str, share_name: &str) {
    let key = share_key(server_name, share_name);

    if let Ok(mut cache) = get_known_shares_mutex().lock() {
        cache
            .known_network_shares
            .retain(|s| share_key(&s.server_name, &s.share_name) != key);
    }

    save_known_shares(app);
}

/// Builds a map of server names to their last known usernames.
/// Useful for pre-filling login forms.
pub fn get_username_hints() -> HashMap<String, String> {
    get_known_shares_mutex()
        .lock()
        .map(|cache| {
            let mut hints = HashMap::new();
            // Group by server, use most recently connected share's username
            for share in cache.known_network_shares.iter() {
                if let Some(ref username) = share.username {
                    // Keep the newest entry per server (shares are in order of addition/update)
                    hints.insert(share.server_name.to_lowercase(), username.clone());
                }
            }
            hints
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_share_key() {
        assert_eq!(share_key("MyNAS", "Documents"), "mynas/documents");
        assert_eq!(share_key("server.local", "Media"), "server.local/media");
    }

    #[test]
    fn test_connection_mode_serialization() {
        let guest = ConnectionMode::Guest;
        let creds = ConnectionMode::Credentials;

        assert_eq!(serde_json::to_string(&guest).unwrap(), r#""guest""#);
        assert_eq!(serde_json::to_string(&creds).unwrap(), r#""credentials""#);

        let guest_back: ConnectionMode = serde_json::from_str(r#""guest""#).unwrap();
        assert_eq!(guest_back, ConnectionMode::Guest);
    }

    #[test]
    fn test_auth_options_serialization() {
        let guest_only = AuthOptions::GuestOnly;
        let creds_only = AuthOptions::CredentialsOnly;
        let both = AuthOptions::GuestOrCredentials;

        assert_eq!(serde_json::to_string(&guest_only).unwrap(), r#""guest_only""#);
        assert_eq!(serde_json::to_string(&creds_only).unwrap(), r#""credentials_only""#);
        assert_eq!(serde_json::to_string(&both).unwrap(), r#""guest_or_credentials""#);
    }

    #[test]
    fn test_known_share_serialization() {
        let share = KnownNetworkShare {
            server_name: "Alpha".to_string(),
            share_name: "Documents".to_string(),
            protocol: "smb".to_string(),
            last_connected_at: "2026-01-03T21:00:00Z".to_string(),
            last_connection_mode: ConnectionMode::Credentials,
            last_known_auth_options: AuthOptions::GuestOrCredentials,
            username: Some("david".to_string()),
        };

        let json = serde_json::to_string_pretty(&share).unwrap();
        assert!(json.contains(r#""serverName": "Alpha""#));
        assert!(json.contains(r#""shareName": "Documents""#));
        assert!(json.contains(r#""lastConnectionMode": "credentials""#));
        assert!(json.contains(r#""lastKnownAuthOptions": "guest_or_credentials""#));
        assert!(json.contains(r#""username": "david""#));

        // Round-trip
        let parsed: KnownNetworkShare = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.server_name, "Alpha");
        assert_eq!(parsed.share_name, "Documents");
        assert_eq!(parsed.last_connection_mode, ConnectionMode::Credentials);
    }

    #[test]
    fn test_store_serialization() {
        let store = KnownSharesStore {
            known_network_shares: vec![
                KnownNetworkShare {
                    server_name: "Alpha".to_string(),
                    share_name: "Documents".to_string(),
                    protocol: "smb".to_string(),
                    last_connected_at: "2026-01-03T21:00:00Z".to_string(),
                    last_connection_mode: ConnectionMode::Credentials,
                    last_known_auth_options: AuthOptions::GuestOrCredentials,
                    username: Some("david".to_string()),
                },
                KnownNetworkShare {
                    server_name: "Bravo".to_string(),
                    share_name: "media".to_string(),
                    protocol: "smb".to_string(),
                    last_connected_at: "2026-01-02T15:30:00Z".to_string(),
                    last_connection_mode: ConnectionMode::Guest,
                    last_known_auth_options: AuthOptions::GuestOnly,
                    username: None,
                },
            ],
        };

        let json = serde_json::to_string_pretty(&store).unwrap();
        assert!(json.contains("knownNetworkShares"));

        // Round-trip
        let parsed: KnownSharesStore = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed.known_network_shares.len(), 2);
    }

    #[test]
    fn test_in_memory_operations() {
        // Test the in-memory cache operations directly
        let cache = get_known_shares_mutex();

        // Clear any previous state
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }

        // Get all should return empty
        let all = get_all_known_shares();
        assert!(all.is_empty());

        // Add a share directly to cache (simulating update without app handle)
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.push(KnownNetworkShare {
                server_name: "TestServer".to_string(),
                share_name: "TestShare".to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-06T12:00:00Z".to_string(),
                last_connection_mode: ConnectionMode::Guest,
                last_known_auth_options: AuthOptions::GuestOnly,
                username: None,
            });
        }

        // Should find it now
        let found = get_known_share("TestServer", "TestShare");
        assert!(found.is_some());
        assert_eq!(found.unwrap().share_name, "TestShare");

        // Case-insensitive lookup
        let found_lower = get_known_share("testserver", "testshare");
        assert!(found_lower.is_some());

        // Get for server
        let server_shares = get_known_shares_for_server("TestServer");
        assert_eq!(server_shares.len(), 1);

        // Clean up
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }
    }

    #[test]
    fn test_username_hints() {
        let cache = get_known_shares_mutex();

        // Clear and set up test data
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
            c.known_network_shares.push(KnownNetworkShare {
                server_name: "Server1".to_string(),
                share_name: "Share1".to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-06T12:00:00Z".to_string(),
                last_connection_mode: ConnectionMode::Credentials,
                last_known_auth_options: AuthOptions::CredentialsOnly,
                username: Some("alice".to_string()),
            });
            c.known_network_shares.push(KnownNetworkShare {
                server_name: "Server2".to_string(),
                share_name: "Share2".to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-06T12:00:00Z".to_string(),
                last_connection_mode: ConnectionMode::Guest,
                last_known_auth_options: AuthOptions::GuestOnly,
                username: None,
            });
        }

        let hints = get_username_hints();
        assert_eq!(hints.get("server1"), Some(&"alice".to_string()));
        assert!(!hints.contains_key("server2")); // No username for guest-only

        // Clean up
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }
    }

    /// Concurrent threads adding distinct shares must not lose any writes.
    #[test]
    fn concurrent_in_memory_updates_no_lost_writes() {
        let cache = get_known_shares_mutex();

        // Clear previous state
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }

        let thread_count = 20;
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(thread_count));
        let mut handles = Vec::new();

        for i in 0..thread_count {
            let barrier = barrier.clone();
            handles.push(std::thread::spawn(move || {
                barrier.wait(); // All threads start at the same time
                let key = format!("server-{}", i);
                if let Ok(mut c) = get_known_shares_mutex().lock() {
                    c.known_network_shares.push(KnownNetworkShare {
                        server_name: key.clone(),
                        share_name: "share".to_string(),
                        protocol: "smb".to_string(),
                        last_connected_at: "2026-01-01T00:00:00Z".to_string(),
                        last_connection_mode: ConnectionMode::Guest,
                        last_known_auth_options: AuthOptions::GuestOnly,
                        username: None,
                    });
                }
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }

        let all = get_all_known_shares();
        assert_eq!(
            all.len(),
            thread_count,
            "Expected {} shares but got {} — a concurrent write was lost",
            thread_count,
            all.len()
        );

        // Clean up
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }
    }

    /// Concurrent reads while another thread writes should not panic or return corrupt data.
    #[test]
    fn concurrent_read_during_write() {
        let cache = get_known_shares_mutex();

        // Seed with initial data
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
            c.known_network_shares.push(KnownNetworkShare {
                server_name: "seed".to_string(),
                share_name: "share".to_string(),
                protocol: "smb".to_string(),
                last_connected_at: "2026-01-01T00:00:00Z".to_string(),
                last_connection_mode: ConnectionMode::Guest,
                last_known_auth_options: AuthOptions::GuestOnly,
                username: None,
            });
        }

        let barrier = std::sync::Arc::new(std::sync::Barrier::new(3));
        let mut handles = Vec::new();

        // Writer thread: adds shares
        let b = barrier.clone();
        handles.push(std::thread::spawn(move || {
            b.wait();
            for i in 0..50 {
                if let Ok(mut c) = get_known_shares_mutex().lock() {
                    c.known_network_shares.push(KnownNetworkShare {
                        server_name: format!("writer-{}", i),
                        share_name: "share".to_string(),
                        protocol: "smb".to_string(),
                        last_connected_at: "2026-01-01T00:00:00Z".to_string(),
                        last_connection_mode: ConnectionMode::Guest,
                        last_known_auth_options: AuthOptions::GuestOnly,
                        username: None,
                    });
                }
            }
        }));

        // Two reader threads: read all shares repeatedly
        for _ in 0..2 {
            let b = barrier.clone();
            handles.push(std::thread::spawn(move || {
                b.wait();
                for _ in 0..100 {
                    let shares = get_all_known_shares();
                    // Must always have at least the seed share
                    assert!(!shares.is_empty(), "Read returned empty during concurrent write");
                }
            }));
        }

        for h in handles {
            h.join().expect("thread panicked");
        }

        // Clean up
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }
    }

    /// Rapid sequential updates to the same share should keep the last value.
    #[test]
    fn rapid_sequential_updates_same_share() {
        let cache = get_known_shares_mutex();

        // Clear previous state
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }

        let iterations = 100;
        for i in 0..iterations {
            if let Ok(mut c) = cache.lock() {
                let key = share_key("rapid-server", "rapid-share");
                if let Some(existing) = c
                    .known_network_shares
                    .iter_mut()
                    .find(|s| share_key(&s.server_name, &s.share_name) == key)
                {
                    existing.last_connected_at = format!("2026-01-01T00:00:{:02}Z", i);
                    existing.username = Some(format!("user-{}", i));
                } else {
                    c.known_network_shares.push(KnownNetworkShare {
                        server_name: "rapid-server".to_string(),
                        share_name: "rapid-share".to_string(),
                        protocol: "smb".to_string(),
                        last_connected_at: format!("2026-01-01T00:00:{:02}Z", i),
                        last_connection_mode: ConnectionMode::Credentials,
                        last_known_auth_options: AuthOptions::GuestOrCredentials,
                        username: Some(format!("user-{}", i)),
                    });
                }
            }
        }

        let share = get_known_share("rapid-server", "rapid-share").expect("share should exist");
        assert_eq!(share.username, Some(format!("user-{}", iterations - 1)));

        // Only one entry should exist (upsert, not duplicate)
        let all = get_all_known_shares();
        let rapid_count = all.iter().filter(|s| s.server_name == "rapid-server").count();
        assert_eq!(rapid_count, 1, "Rapid updates should not create duplicate entries");

        // Clean up
        if let Ok(mut c) = cache.lock() {
            c.known_network_shares.clear();
        }
    }
}
