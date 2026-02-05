//! SMB share listing cache.
//!
//! Provides caching functionality for share listing results to reduce
//! network round-trips and improve responsiveness.

use crate::network::smb_types::{AuthMode, ShareListResult};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// Cached share list with expiration.
pub(super) struct CachedShares {
    result: ShareListResult,
    expires_at: Instant,
}

/// Share cache with configurable TTL.
static SHARE_CACHE: std::sync::OnceLock<Mutex<HashMap<String, CachedShares>>> = std::sync::OnceLock::new();

/// Default cache TTL (30 seconds) - used when no setting is provided.
pub const DEFAULT_CACHE_TTL_MS: u64 = 30_000;
/// Default list shares timeout (15 seconds) - used when no setting is provided.
pub const DEFAULT_LIST_SHARES_TIMEOUT_MS: u64 = 15_000;

fn get_share_cache() -> &'static Mutex<HashMap<String, CachedShares>> {
    SHARE_CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Gets cached shares for a host if still valid.
pub fn get_cached_shares(host_id: &str) -> Option<ShareListResult> {
    let cache = get_share_cache().lock().ok()?;
    let entry = cache.get(host_id)?;

    if Instant::now() < entry.expires_at {
        let mut result = entry.result.clone();
        result.from_cache = true;
        Some(result)
    } else {
        None
    }
}

/// Caches share list for a host with a configurable TTL.
pub fn cache_shares(host_id: &str, result: &ShareListResult, cache_ttl_ms: u64) {
    if let Ok(mut cache) = get_share_cache().lock() {
        // Clean up expired entries while we're here
        let now = Instant::now();
        cache.retain(|_, v| v.expires_at > now);

        let ttl = Duration::from_millis(cache_ttl_ms);
        cache.insert(
            host_id.to_string(),
            CachedShares {
                result: result.clone(),
                expires_at: now + ttl,
            },
        );
    }
}

/// Invalidates cache for a host.
#[allow(
    dead_code,
    reason = "Will be used when implementing cache invalidation on host disconnect"
)]
pub fn invalidate_cache(host_id: &str) {
    if let Ok(mut cache) = get_share_cache().lock() {
        cache.remove(host_id);
    }
}

/// Gets the cached auth mode for a host, if available.
pub fn get_cached_shares_auth_mode(host_id: &str) -> Option<AuthMode> {
    let cache = get_share_cache().lock().ok()?;
    let entry = cache.get(host_id)?;

    if Instant::now() < entry.expires_at {
        Some(entry.result.auth_mode)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::network::smb_types::ShareInfo;

    #[test]
    fn test_cache_operations() {
        let host_id = "test-host-cache";

        // Initially no cache
        assert!(get_cached_shares(host_id).is_none());

        // Cache something
        let result = ShareListResult {
            shares: vec![ShareInfo {
                name: "TestShare".to_string(),
                is_disk: true,
                comment: None,
            }],
            auth_mode: AuthMode::GuestAllowed,
            from_cache: false,
        };
        cache_shares(host_id, &result, DEFAULT_CACHE_TTL_MS);

        // Should be cached now
        let cached = get_cached_shares(host_id);
        assert!(cached.is_some());
        let cached = cached.unwrap();
        assert!(cached.from_cache);
        assert_eq!(cached.shares.len(), 1);
        assert_eq!(cached.shares[0].name, "TestShare");

        // Invalidate
        invalidate_cache(host_id);
        assert!(get_cached_shares(host_id).is_none());
    }
}
