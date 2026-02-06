//! Error types for MTP connection operations.

use mtp_rs::ptp::ResponseCode;

/// Error types for MTP connection operations.
#[derive(Debug, Clone, serde::Serialize)]
#[serde(rename_all = "camelCase", tag = "type")]
pub enum MtpConnectionError {
    /// Device not found (may have been unplugged).
    DeviceNotFound { device_id: String },
    /// Device is not connected.
    NotConnected { device_id: String },
    /// Another process has exclusive access to the device.
    ExclusiveAccess {
        device_id: String,
        blocking_process: Option<String>,
    },
    /// Connection timed out.
    Timeout { device_id: String },
    /// Device was disconnected unexpectedly.
    Disconnected { device_id: String },
    /// Protocol error from device.
    Protocol { device_id: String, message: String },
    /// Device is busy (retryable).
    DeviceBusy { device_id: String },
    /// Storage is full.
    StorageFull { device_id: String },
    /// Object not found on device.
    ObjectNotFound { device_id: String, path: String },
    /// Other connection error.
    Other { device_id: String, message: String },
}

impl MtpConnectionError {
    /// Returns true if the operation may succeed if retried.
    #[allow(dead_code, reason = "Will be used by frontend for retry logic")]
    pub fn is_retryable(&self) -> bool {
        matches!(self, Self::Timeout { .. } | Self::DeviceBusy { .. })
    }

    /// Returns a user-friendly message for this error.
    #[allow(dead_code, reason = "Will be exposed via Tauri commands for UI error display")]
    pub fn user_message(&self) -> String {
        match self {
            Self::DeviceNotFound { .. } => "Device not found. It may have been unplugged.".to_string(),
            Self::NotConnected { .. } => {
                "Device is not connected. Select it from the volume picker to connect.".to_string()
            }
            Self::ExclusiveAccess { blocking_process, .. } => {
                if let Some(proc) = blocking_process {
                    format!(
                        "Another app ({}) is using this device. Close it or use the Terminal workaround.",
                        proc
                    )
                } else {
                    "Another app is using this device. Close other apps that might be accessing it.".to_string()
                }
            }
            Self::Timeout { .. } => {
                "The operation timed out. The device may be slow or unresponsive. Try again.".to_string()
            }
            Self::Disconnected { .. } => "Device was disconnected. Reconnect it to continue.".to_string(),
            Self::Protocol { message, .. } => {
                format!("Device reported an error: {}. Try reconnecting.", message)
            }
            Self::DeviceBusy { .. } => "Device is busy. Wait a moment and try again.".to_string(),
            Self::StorageFull { .. } => "Device storage is full. Free up some space.".to_string(),
            Self::ObjectNotFound { path, .. } => {
                format!("File or folder not found: {}. It may have been deleted.", path)
            }
            Self::Other { message, .. } => message.clone(),
        }
    }
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
            Self::ObjectNotFound { device_id, path } => {
                write!(f, "Object not found on {device_id}: {path}")
            }
            Self::Other { device_id, message } => {
                write!(f, "Error for {device_id}: {message}")
            }
        }
    }
}

impl std::error::Error for MtpConnectionError {}

/// Maps mtp_rs errors to our error types.
pub(super) fn map_mtp_error(e: mtp_rs::Error, device_id: &str) -> MtpConnectionError {
    match e {
        mtp_rs::Error::NoDevice => MtpConnectionError::DeviceNotFound {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Disconnected => MtpConnectionError::Disconnected {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Timeout => MtpConnectionError::Timeout {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Cancelled => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: "Operation cancelled".to_string(),
        },
        mtp_rs::Error::SessionNotOpen => MtpConnectionError::NotConnected {
            device_id: device_id.to_string(),
        },
        mtp_rs::Error::Protocol { code, operation } => {
            // Map specific response codes to user-friendly errors
            match code {
                ResponseCode::DeviceBusy => MtpConnectionError::DeviceBusy {
                    device_id: device_id.to_string(),
                },
                ResponseCode::StoreFull => MtpConnectionError::StorageFull {
                    device_id: device_id.to_string(),
                },
                ResponseCode::StoreReadOnly => MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "This device is read-only. You can copy files from it, but not to it.".to_string(),
                },
                ResponseCode::InvalidObjectHandle | ResponseCode::InvalidParentObject => {
                    MtpConnectionError::ObjectNotFound {
                        device_id: device_id.to_string(),
                        path: format!("(operation: {:?})", operation),
                    }
                }
                ResponseCode::AccessDenied => MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: "Access denied. The device rejected the operation.".to_string(),
                },
                _ => MtpConnectionError::Protocol {
                    device_id: device_id.to_string(),
                    message: format!("{:?}", code),
                },
            }
        }
        mtp_rs::Error::InvalidData { message } => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("Invalid data from device: {}", message),
        },
        mtp_rs::Error::Io(io_err) => MtpConnectionError::Other {
            device_id: device_id.to_string(),
            message: format!("I/O error: {}", io_err),
        },
        mtp_rs::Error::Usb(usb_err) => {
            // Check for exclusive access errors
            let msg = usb_err.to_string().to_lowercase();
            if msg.contains("exclusive access") || msg.contains("device or resource busy") {
                MtpConnectionError::ExclusiveAccess {
                    device_id: device_id.to_string(),
                    blocking_process: None,
                }
            } else {
                MtpConnectionError::Other {
                    device_id: device_id.to_string(),
                    message: format!("USB error: {}", usb_err),
                }
            }
        }
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
    fn test_connection_error_is_retryable() {
        // Retryable errors
        assert!(
            MtpConnectionError::Timeout {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            MtpConnectionError::DeviceBusy {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );

        // Non-retryable errors
        assert!(
            !MtpConnectionError::DeviceNotFound {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            !MtpConnectionError::Disconnected {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
        assert!(
            !MtpConnectionError::StorageFull {
                device_id: "mtp-1-5".to_string()
            }
            .is_retryable()
        );
    }

    #[test]
    fn test_connection_error_user_message() {
        let err = MtpConnectionError::DeviceNotFound {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("not found"));
        assert!(err.user_message().contains("unplugged"));

        let err = MtpConnectionError::StorageFull {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("full"));

        let err = MtpConnectionError::DeviceBusy {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.user_message().contains("busy"));
        assert!(err.user_message().contains("try again"));
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
        assert!(err.to_string().contains("Object not found"));
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

        // Test user message for this case
        assert!(err.user_message().contains("Another app"));
        assert!(!err.user_message().contains("ptpcamerad"));
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
            // Each should have non-empty user message
            assert!(!err.user_message().is_empty());
        }
    }

    #[test]
    fn test_not_connected_error() {
        let err = MtpConnectionError::NotConnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert_eq!(err.to_string(), "Device not connected: mtp-1-5");
        assert!(err.user_message().contains("not connected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_timeout_error() {
        let err = MtpConnectionError::Timeout {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("timed out"));
        assert!(err.user_message().contains("timed out"));
        assert!(err.is_retryable());
    }

    #[test]
    fn test_disconnected_error() {
        let err = MtpConnectionError::Disconnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("disconnected"));
        assert!(err.user_message().contains("disconnected"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_protocol_error_user_message() {
        let err = MtpConnectionError::Protocol {
            device_id: "mtp-1-5".to_string(),
            message: "InvalidObjectHandle".to_string(),
        };
        assert!(err.user_message().contains("InvalidObjectHandle"));
        assert!(err.user_message().contains("reconnecting"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_object_not_found_user_message() {
        let err = MtpConnectionError::ObjectNotFound {
            device_id: "mtp-1-5".to_string(),
            path: "/DCIM/photo.jpg".to_string(),
        };
        assert!(err.user_message().contains("/DCIM/photo.jpg"));
        assert!(err.user_message().contains("deleted"));
        assert!(!err.is_retryable());
    }

    #[test]
    fn test_other_error() {
        let err = MtpConnectionError::Other {
            device_id: "mtp-1-5".to_string(),
            message: "Custom error message".to_string(),
        };
        assert!(err.to_string().contains("Custom error message"));
        assert_eq!(err.user_message(), "Custom error message");
        assert!(!err.is_retryable());
    }
}
