//! Error types for MTP connection operations.

/// Error types for MTP connection operations.
#[derive(Debug, Clone, serde::Serialize, specta::Type)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MtpConnectionError {
    DeviceNotFound {
        device_id: String,
    },
    NotConnected {
        device_id: String,
    },
    ExclusiveAccess {
        device_id: String,
        blocking_process: Option<String>,
    },
    Timeout {
        device_id: String,
    },
    Disconnected {
        device_id: String,
    },
    Protocol {
        device_id: String,
        message: String,
    },
    /// Retryable.
    DeviceBusy {
        device_id: String,
    },
    StorageFull {
        device_id: String,
    },
    StoreReadOnly {
        device_id: String,
    },
    /// USB device file not accessible (Linux: missing udev rules; `EACCES`).
    PermissionDenied {
        device_id: String,
    },
    Cancelled {
        device_id: String,
        message: String,
    },
    ObjectNotFound {
        device_id: String,
        path: String,
    },
    /// The cached parent-folder handle was rejected by the device during an
    /// upload's `SendObjectInfo` (the device re-keyed its object handles since
    /// the folder was last listed). The cache has been refreshed; the caller
    /// should re-resolve and retry the upload once with a fresh source stream.
    /// Carries the destination folder path so the volume layer can surface a
    /// destination-correct message if the retry also fails.
    StaleParentHandle {
        device_id: String,
        dest_folder: String,
    },
    /// mtp-rs reset the device in software to recover from a wedged transfer
    /// cancel. The PTP session is gone, but the device is STILL PLUGGED IN and
    /// reopenable with no replug — ❌ never treat this as a disconnect.
    ///
    /// Reachable in Cmdr through `PtpSession::recover_if_needed()`: a data-phase
    /// op whose future is dropped mid-flight leaves an armed `TransactionScope`,
    /// and the NEXT op drains it via `cancel_transfer` and propagates that
    /// drain's outcome verbatim. ❌ Cmdr never drops one deliberately (see the
    /// module `CLAUDE.md`), so this is the seatbelt for a genuine disconnect
    /// mid-transfer, not a routine path.
    SessionReset {
        device_id: String,
    },
    Other {
        device_id: String,
        message: String,
    },
}

impl std::fmt::Display for MtpConnectionError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::DeviceNotFound { device_id } => {
                write!(f, "Device not found: {device_id}")
            }
            Self::NotConnected { device_id } => {
                write!(f, "Device not connected: {device_id}")
            }
            Self::ExclusiveAccess {
                device_id,
                blocking_process,
            } => {
                if let Some(proc) = blocking_process {
                    write!(f, "Device {device_id} is in use by {proc}")
                } else {
                    write!(f, "Device {device_id} is in use by another process")
                }
            }
            Self::Timeout { device_id } => {
                write!(f, "Connection timed out for device: {device_id}")
            }
            Self::Disconnected { device_id } => {
                write!(f, "Device disconnected: {device_id}")
            }
            Self::Protocol { device_id, message } => {
                write!(f, "Protocol error for {device_id}: {message}")
            }
            Self::DeviceBusy { device_id } => {
                write!(f, "Device busy: {device_id}")
            }
            Self::StorageFull { device_id } => {
                write!(f, "Storage full on device: {device_id}")
            }
            Self::StoreReadOnly { device_id } => {
                write!(f, "Device is read-only: {device_id}")
            }
            Self::PermissionDenied { device_id } => {
                write!(f, "Permission denied for device: {device_id}")
            }
            Self::Cancelled { device_id, message } => {
                write!(f, "Cancelled on {device_id}: {message}")
            }
            Self::ObjectNotFound { device_id, path } => {
                write!(f, "Object not found on {device_id}: {path}")
            }
            Self::StaleParentHandle { device_id, dest_folder } => {
                write!(f, "Stale destination folder handle on {device_id}: {dest_folder}")
            }
            Self::SessionReset { device_id } => {
                write!(
                    f,
                    "The connection to {device_id} restarted; it's still plugged in and can be reopened"
                )
            }
            Self::Other { device_id, message } => {
                write!(f, "Error for {device_id}: {message}")
            }
        }
    }
}

impl std::error::Error for MtpConnectionError {}

/// `true` when the device rejected an operation because the object/parent handle
/// we sent is no longer valid — the device re-keyed its handles since we last
/// listed. On Android this happens when MediaProvider rescans between a folder
/// listing and a later upload into it. The upload path treats this as a
/// recoverable stale-cache condition (refresh handles + retry once) rather than
/// a real not-found.
pub(super) fn is_stale_handle_rejection(e: &mtp_rs::Error) -> bool {
    e.is_stale_handle()
}

/// Maps mtp_rs errors to our error types.
///
/// `mtp_rs::Error` is backend-neutral and `#[non_exhaustive]`, so this matches
/// the neutral variants and keeps a catch-all for future ones.
pub(super) fn map_mtp_error(e: mtp_rs::Error, device_id: &str) -> MtpConnectionError {
    use mtp_rs::Error as E;
    let device_id = device_id.to_string();
    match e {
        E::NoDevice => MtpConnectionError::DeviceNotFound { device_id },
        E::Disconnected => MtpConnectionError::Disconnected { device_id },
        E::Timeout => MtpConnectionError::Timeout { device_id },
        E::Cancelled => MtpConnectionError::Cancelled {
            device_id,
            message: "Operation cancelled".to_string(),
        },
        E::ExclusiveAccess => MtpConnectionError::ExclusiveAccess {
            device_id,
            blocking_process: None,
        },
        // Missing udev permission (Linux `EACCES`), distinct from exclusive access.
        E::PermissionDenied => MtpConnectionError::PermissionDenied { device_id },
        E::Busy => MtpConnectionError::DeviceBusy { device_id },
        E::StorageFull => MtpConnectionError::StorageFull { device_id },
        // Read-only storage and write-protected/denied objects all surface to the
        // user as "the device refused this write".
        E::AccessDenied => MtpConnectionError::StoreReadOnly { device_id },
        // A re-keyed handle is the recoverable stale-cache case; the upload path
        // intercepts it via `is_stale_handle_rejection` before mapping, so reaching
        // here means a non-recoverable context — surface it as not-found.
        E::StaleHandle => MtpConnectionError::ObjectNotFound {
            device_id,
            path: "(stale object handle)".to_string(),
        },
        // A software device reset, NOT a disconnect: the device is still present
        // (`Error::is_disconnected()` is deliberately false for it) and only the
        // PTP session died. Its own log line so a reset is diagnosable in a log
        // instead of hiding inside the generic `Other` bucket.
        //
        // This is also where recovery starts. Every device op funnels through
        // this mapper, so it's the one choke point that sees a reset no matter
        // which operation tripped it; `schedule_recovery` is fire-and-forget and
        // deduplicates per device, so the failing op still returns right away.
        E::DeviceReset => {
            log::warn!(
                target: "mtp_connection",
                "Device {device_id} was reset in software to recover a wedged transfer cancel: the PTP session is gone, the device is still attached"
            );
            super::session_reset::schedule_recovery(&device_id);
            MtpConnectionError::SessionReset { device_id }
        }
        E::NotFound => MtpConnectionError::ObjectNotFound {
            device_id,
            path: "(not found)".to_string(),
        },
        E::Unsupported => MtpConnectionError::Other {
            device_id,
            message: "Operation not supported by this device.".to_string(),
        },
        E::InvalidData { message } => MtpConnectionError::Other {
            device_id,
            message: format!("Invalid data from device: {message}"),
        },
        E::Io { message } => MtpConnectionError::Other {
            device_id,
            message: format!("I/O error: {message}"),
        },
        E::Other { detail } => MtpConnectionError::Protocol {
            device_id,
            message: detail,
        },
        // `mtp_rs::Error` is `#[non_exhaustive]`; treat any future variant as a
        // generic protocol error rather than failing to compile downstream.
        other => MtpConnectionError::Other {
            device_id,
            message: other.to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_connection_error_display() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not found: mtp-1-5");

        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: Some("ptpcamerad".to_string()),
        };
        assert_eq!(err.to_string(), "Device mtp-1-5 is in use by ptpcamerad");
    }

    #[test]
    fn test_new_error_types_display() {
        let err = MtpConnectionError::DeviceBusy {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device busy: mtp-1-5");

        let err = MtpConnectionError::StorageFull {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Storage full on device: mtp-1-5");

        let err = MtpConnectionError::ObjectNotFound {
            device_id: "mtp-1-5".to_string(),
            path: "/DCIM/photo.jpg".to_string(),
        };
        // allowed-error-string-match: testing Display impl of MtpConnectionError::ObjectNotFound
        assert!(err.to_string().contains("Object not found"));
        // allowed-error-string-match: testing Display impl of MtpConnectionError::ObjectNotFound
        assert!(err.to_string().contains("/DCIM/photo.jpg"));
    }

    #[test]
    fn test_connection_error_serialization() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Note: With tag = "type" and rename_all = "camelCase", device_id becomes deviceId
        assert!(json.contains("\"type\":\"deviceNotFound\""), "JSON: {}", json);
        assert!(json.contains("\"device_id\":\"mtp-1-5\""), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_exclusive_access_serialization() {
        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: Some("ptpcamerad".to_string()),
        };
        let json = serde_json::to_string(&err).unwrap();
        // Note: tag type is camelCase, but inner field names stay snake_case
        assert!(json.contains("\"type\":\"exclusiveAccess\""), "JSON: {}", json);
        assert!(json.contains("\"blocking_process\":\"ptpcamerad\""), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_exclusive_access_no_process() {
        let err = MtpConnectionError::ExclusiveAccess {
            device_id: "mtp-1-5".to_string(),
            blocking_process: None,
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"blocking_process\":null"), "JSON: {}", json);
    }

    #[test]
    fn test_connection_error_protocol_serialization() {
        let err = MtpConnectionError::Protocol {
            device_id: "mtp-1-5".to_string(),
            message: "InvalidObjectHandle".to_string(),
        };
        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"type\":\"protocol\""), "JSON: {}", json);
        assert!(json.contains("\"message\":\"InvalidObjectHandle\""), "JSON: {}", json);
    }

    #[test]
    fn test_all_error_variants_display() {
        // Test all variants have Display impl
        let errors = vec![
            MtpConnectionError::DeviceNotFound {
                device_id: "test".to_string(),
            },
            MtpConnectionError::NotConnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::ExclusiveAccess {
                device_id: "test".to_string(),
                blocking_process: None,
            },
            MtpConnectionError::Timeout {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Disconnected {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Protocol {
                device_id: "test".to_string(),
                message: "error".to_string(),
            },
            MtpConnectionError::DeviceBusy {
                device_id: "test".to_string(),
            },
            MtpConnectionError::StorageFull {
                device_id: "test".to_string(),
            },
            MtpConnectionError::StoreReadOnly {
                device_id: "test".to_string(),
            },
            MtpConnectionError::PermissionDenied {
                device_id: "test".to_string(),
            },
            MtpConnectionError::Cancelled {
                device_id: "test".to_string(),
                message: "cancelled".to_string(),
            },
            MtpConnectionError::ObjectNotFound {
                device_id: "test".to_string(),
                path: "/path".to_string(),
            },
            MtpConnectionError::Other {
                device_id: "test".to_string(),
                message: "other".to_string(),
            },
        ];

        for err in errors {
            // Each should have non-empty display
            assert!(!err.to_string().is_empty());
        }
    }

    #[test]
    fn device_reset_maps_to_a_typed_variant_not_the_catch_all() {
        // A software device reset is a distinct, diagnosable condition: the PTP
        // session is gone but the device is still plugged in. Landing in the
        // `#[non_exhaustive]` catch-all as `Other` erases that.
        let mapped = map_mtp_error(mtp_rs::Error::DeviceReset, "mtp-1-5");
        assert!(
            matches!(mapped, MtpConnectionError::SessionReset { .. }),
            "DeviceReset must map to SessionReset, got {mapped:?}"
        );
    }

    #[test]
    fn device_reset_is_not_a_disconnect() {
        // mtp-rs is explicit that the device is still present and reopenable
        // after a reset, and `Error::is_disconnected()` is deliberately false for
        // it. Mapping it to `Disconnected` would tear down a live device: the
        // volume leaves the sidebar and the index flips Stale for nothing.
        let mapped = map_mtp_error(mtp_rs::Error::DeviceReset, "mtp-1-5");
        assert!(
            !matches!(mapped, MtpConnectionError::Disconnected { .. }),
            "DeviceReset must not be conflated with a disconnect, got {mapped:?}"
        );
    }

    #[test]
    fn test_not_connected_error() {
        let err = MtpConnectionError::NotConnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not connected: mtp-1-5");
    }

    #[test]
    fn test_timeout_error() {
        let err = MtpConnectionError::Timeout {
            device_id: "mtp-1-5".to_string(),
        };
        // allowed-error-string-match: testing Display impl of MtpConnectionError::Timeout
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_disconnected_error() {
        let err = MtpConnectionError::Disconnected {
            device_id: "mtp-1-5".to_string(),
        };
        // allowed-error-string-match: testing Display impl of MtpConnectionError::Disconnected
        assert!(err.to_string().contains("disconnected"));
    }

    #[test]
    fn test_permission_denied_error() {
        let err = MtpConnectionError::PermissionDenied {
            device_id: "mtp-1-5".to_string(),
        };
        // allowed-error-string-match: testing Display impl of MtpConnectionError::PermissionDenied
        assert!(err.to_string().contains("Permission denied"));

        let json = serde_json::to_string(&err).unwrap();
        assert!(json.contains("\"type\":\"permissionDenied\""), "JSON: {}", json);
    }

    #[test]
    fn test_other_error() {
        let err = MtpConnectionError::Other {
            device_id: "mtp-1-5".to_string(),
            message: "Custom error message".to_string(),
        };
        // allowed-error-string-match: testing Display impl of MtpConnectionError::Other
        assert!(err.to_string().contains("Custom error message"));
    }
}
