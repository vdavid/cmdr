//! Tauri commands for the SMB diagnostics dashboard (debug-window only).
//!
//! Two commands:
//! - [`list_smb_volumes`] — picker entries for the dashboard's volume selector
//! - [`get_smb_diagnostics`] — snapshot of one volume's `smb2::SmbClient`
//!
//! The snapshot types are cmdr-side mirrors of `smb2::Diagnostics` &
//! friends, with `specta::Type` derives so the typed bindings are
//! end-to-end. Conversion is one `impl From<smb2::X> for XDto` per type.
//! Two reasons we mirror instead of re-exporting:
//!
//! 1. `smb2` doesn't (and shouldn't) depend on `specta`.
//! 2. We can pick a TS-friendly shape (e.g. `Duration` → milliseconds as
//!    `u64`, enums as `String`) without leaking Rust's `std::time::Duration`
//!    JSON shape (`{secs, nanos}`) to the frontend.

use crate::file_system::SmbVolume;
use crate::file_system::get_volume_manager;

// ── DTO mirror types ──────────────────────────────────────────────────

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SmbVolumeRef {
    pub volume_id: String,
    pub name: String,
    pub server: String,
    pub disconnected: bool,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SmbDiagnosticsDto {
    pub client: ClientInfoDto,
    pub primary: ConnectionDiagnosticsDto,
    pub extra_connections: Vec<ConnectionDiagnosticsDto>,
    pub dfs_cache: Vec<DfsCacheEntryDto>,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ClientInfoDto {
    pub primary_server: String,
    pub timeout_ms: u64,
    pub auto_reconnect: bool,
    pub dfs_enabled: bool,
    pub metrics: ClientMetricsDto,
}

#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type)]
pub struct ClientMetricsDto {
    pub reconnects: u64,
    pub dfs_referrals_resolved: u64,
    pub dfs_cache_hits: u64,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct ConnectionDiagnosticsDto {
    pub server: String,
    pub negotiated: Option<NegotiatedSummaryDto>,
    pub credits: CreditInfoDto,
    pub signing: SigningInfoDto,
    pub encryption: EncryptionInfoDto,
    pub compression: CompressionInfoDto,
    pub rtt_estimate_ms: Option<f64>,
    pub disconnected: bool,
    pub dfs_trees: Vec<u32>,
    pub session: Option<SessionDiagnosticsDto>,
    pub metrics: MetricsSnapshotDto,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct NegotiatedSummaryDto {
    pub dialect: String,
    pub max_read_size: u32,
    pub max_write_size: u32,
    pub max_transact_size: u32,
    pub server_guid_hex: String,
    pub signing_required: bool,
    pub capabilities_bits: u32,
    pub gmac_negotiated: bool,
    pub cipher: Option<String>,
    pub compression_supported: bool,
}

#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type)]
pub struct CreditInfoDto {
    pub available: u16,
    pub in_flight: u32,
    pub next_message_id: u64,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SigningInfoDto {
    pub active: bool,
    pub algorithm: Option<String>,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct EncryptionInfoDto {
    pub active: bool,
    pub cipher: Option<String>,
}

#[derive(Debug, Clone, Copy, serde::Serialize, specta::Type)]
pub struct CompressionInfoDto {
    pub requested: bool,
    pub negotiated: bool,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct SessionDiagnosticsDto {
    pub session_id_hex: String,
    pub should_sign: bool,
    pub should_encrypt: bool,
    pub signing_algorithm: String,
}

#[derive(Debug, Clone, serde::Serialize, specta::Type)]
pub struct DfsCacheEntryDto {
    pub path_prefix: String,
    pub target_count: u32,
    pub expires_in_ms: Option<u64>,
}

#[derive(Debug, Clone, Copy, Default, serde::Serialize, specta::Type)]
pub struct MetricsSnapshotDto {
    pub requests_sent: u64,
    pub compound_requests_sent: u64,
    pub wire_bytes_sent: u64,
    pub explicit_cancels_sent: u64,
    pub responses_routed_ok: u64,
    pub responses_routed_err: u64,
    pub responses_late_after_drop: u64,
    pub responses_stray: u64,
    pub wire_bytes_received: u64,
    pub status_pending_loops: u64,
    pub unsolicited_notifications_received: u64,
    pub signature_failures: u64,
    pub decrypt_failures: u64,
    pub decompress_failures: u64,
    pub malformed_frames: u64,
    pub session_expired_events: u64,
    pub requests_returned_err: u64,
}

// ── Conversions ───────────────────────────────────────────────────────

impl From<smb2::Diagnostics> for SmbDiagnosticsDto {
    fn from(d: smb2::Diagnostics) -> Self {
        Self {
            client: d.client.into(),
            primary: d.primary.into(),
            extra_connections: d.extra_connections.into_iter().map(Into::into).collect(),
            dfs_cache: d.dfs_cache.into_iter().map(Into::into).collect(),
        }
    }
}

impl From<smb2::ClientInfo> for ClientInfoDto {
    fn from(c: smb2::ClientInfo) -> Self {
        Self {
            primary_server: c.primary_server,
            timeout_ms: c.timeout.as_millis() as u64,
            auto_reconnect: c.auto_reconnect,
            dfs_enabled: c.dfs_enabled,
            metrics: ClientMetricsDto {
                reconnects: c.metrics.reconnects,
                dfs_referrals_resolved: c.metrics.dfs_referrals_resolved,
                dfs_cache_hits: c.metrics.dfs_cache_hits,
            },
        }
    }
}

impl From<smb2::ConnectionDiagnostics> for ConnectionDiagnosticsDto {
    fn from(c: smb2::ConnectionDiagnostics) -> Self {
        Self {
            server: c.server,
            negotiated: c.negotiated.map(Into::into),
            credits: CreditInfoDto {
                available: c.credits.available,
                in_flight: c.credits.in_flight as u32,
                next_message_id: c.credits.next_message_id,
            },
            signing: SigningInfoDto {
                active: c.signing.active,
                algorithm: c.signing.algorithm.map(|a| format!("{:?}", a)),
            },
            encryption: EncryptionInfoDto {
                active: c.encryption.active,
                cipher: c.encryption.cipher.map(|cph| format!("{:?}", cph)),
            },
            compression: CompressionInfoDto {
                requested: c.compression.requested,
                negotiated: c.compression.negotiated,
            },
            rtt_estimate_ms: c.rtt_estimate.map(|d| d.as_secs_f64() * 1000.0),
            disconnected: c.disconnected,
            dfs_trees: c.dfs_trees.into_iter().map(|t| t.0).collect(),
            session: c.session.map(Into::into),
            metrics: MetricsSnapshotDto {
                requests_sent: c.metrics.requests_sent,
                compound_requests_sent: c.metrics.compound_requests_sent,
                wire_bytes_sent: c.metrics.wire_bytes_sent,
                explicit_cancels_sent: c.metrics.explicit_cancels_sent,
                responses_routed_ok: c.metrics.responses_routed_ok,
                responses_routed_err: c.metrics.responses_routed_err,
                responses_late_after_drop: c.metrics.responses_late_after_drop,
                responses_stray: c.metrics.responses_stray,
                wire_bytes_received: c.metrics.wire_bytes_received,
                status_pending_loops: c.metrics.status_pending_loops,
                unsolicited_notifications_received: c
                    .metrics
                    .unsolicited_notifications_received,
                signature_failures: c.metrics.signature_failures,
                decrypt_failures: c.metrics.decrypt_failures,
                decompress_failures: c.metrics.decompress_failures,
                malformed_frames: c.metrics.malformed_frames,
                session_expired_events: c.metrics.session_expired_events,
                requests_returned_err: c.metrics.requests_returned_err,
            },
        }
    }
}

impl From<smb2::NegotiatedSummary> for NegotiatedSummaryDto {
    fn from(n: smb2::NegotiatedSummary) -> Self {
        Self {
            dialect: format!("{:?}", n.dialect),
            max_read_size: n.max_read_size,
            max_write_size: n.max_write_size,
            max_transact_size: n.max_transact_size,
            server_guid_hex: format!(
                "{:08x}-{:04x}-{:04x}-{}",
                n.server_guid.data1,
                n.server_guid.data2,
                n.server_guid.data3,
                n.server_guid
                    .data4
                    .iter()
                    .map(|b| format!("{:02x}", b))
                    .collect::<String>(),
            ),
            signing_required: n.signing_required,
            capabilities_bits: n.capabilities.0,
            gmac_negotiated: n.gmac_negotiated,
            cipher: n.cipher.map(|c| format!("{:?}", c)),
            compression_supported: n.compression_supported,
        }
    }
}

impl From<smb2::SessionDiagnostics> for SessionDiagnosticsDto {
    fn from(s: smb2::SessionDiagnostics) -> Self {
        Self {
            session_id_hex: format!("{:016x}", s.session_id.0),
            should_sign: s.should_sign,
            should_encrypt: s.should_encrypt,
            signing_algorithm: format!("{:?}", s.signing_algorithm),
        }
    }
}

impl From<smb2::DfsCacheEntry> for DfsCacheEntryDto {
    fn from(e: smb2::DfsCacheEntry) -> Self {
        Self {
            path_prefix: e.path_prefix,
            target_count: e.target_count as u32,
            expires_in_ms: e.expires_in.map(|d| d.as_millis() as u64),
        }
    }
}

// ── Commands ──────────────────────────────────────────────────────────

/// List every currently-registered SMB volume, with a one-line summary for
/// the dashboard's volume picker. Returns an empty vec if no SMB volumes
/// are mounted. A volume that's currently disconnected still shows up —
/// `disconnected: true` indicates that, and the dashboard renders it
/// distinctly so the user can see why diagnostics are stale.
#[tauri::command]
#[specta::specta]
pub async fn list_smb_volumes() -> Vec<SmbVolumeRef> {
    let manager = get_volume_manager();
    let ids: Vec<(String, String)> = manager.list_volumes();
    let mut out = Vec::new();
    for (id, name) in ids {
        let Some(vol) = manager.get(&id) else { continue };
        // Hold the Arc<dyn Volume> across the await — downcast is sync,
        // returns a borrow tied to `vol`, so the await stays inside the
        // borrow's scope.
        if vol.as_any().downcast_ref::<SmbVolume>().is_none() {
            continue;
        }
        // Re-downcast to call the async diagnostics() method. Cheap.
        let server: String;
        let disconnected: bool;
        if let Some(smb) = vol.as_any().downcast_ref::<SmbVolume>() {
            match smb.diagnostics().await {
                Some(diag) => {
                    server = diag.primary.server.clone();
                    disconnected = diag.primary.disconnected;
                }
                None => {
                    server = String::new();
                    disconnected = true;
                }
            }
        } else {
            continue;
        }
        out.push(SmbVolumeRef {
            volume_id: id,
            name,
            server,
            disconnected,
        });
    }
    out
}

/// Snapshot of the SMB client backing the given volume.
///
/// Returns `Err(message)` if the volume id is unknown, the volume isn't
/// an SMB volume, or the volume is currently disconnected (no client to
/// snapshot). Always cheap — internally a handful of atomic loads and
/// short-critical-section mutex copies; no network I/O.
#[tauri::command]
#[specta::specta]
pub async fn get_smb_diagnostics(volume_id: String) -> Result<SmbDiagnosticsDto, String> {
    let manager = get_volume_manager();
    let vol = manager
        .get(&volume_id)
        .ok_or_else(|| format!("no such volume: {volume_id}"))?;
    let smb_diag = {
        let smb = vol
            .as_any()
            .downcast_ref::<SmbVolume>()
            .ok_or_else(|| format!("volume {volume_id} is not an SMB volume"))?;
        smb.diagnostics().await
    };
    let diag = smb_diag.ok_or_else(|| format!("volume {volume_id} is disconnected"))?;
    Ok(diag.into())
}
