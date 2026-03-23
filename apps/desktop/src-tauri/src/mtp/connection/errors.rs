//! Error types for MTP connection operations.

use mtp_rs::ptp::ResponseCode;

/// Error types for MTP connection operations.
#[derive(Debug, Clone, serde::Serialize)]
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
    /// Linux: USB device file not accessible (missing udev rules).
    PermissionDenied {
        device_id: String,
    },
    ObjectNotFound {
        device_id: String,
        path: String,
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
            Self::PermissionDenied { device_id } => {
                write!(f, "Permission denied for device: {device_id}")
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
            // nusb::Error wraps OS-level USB errors. The message is typically the OS error
            // string (e.g. "Permission denied (os error 13)" on Linux). The two checks
            // below are mutually exclusive: "exclusive access"/"busy" vs "permission denied".
            let msg = usb_err.to_string().to_lowercase();
            if msg.contains("exclusive access") || msg.contains("device or resource busy") {
                MtpConnectionError::ExclusiveAccess {
                    device_id: device_id.to_string(),
                    blocking_process: None,
                }
            } else if msg.contains("permission denied") || msg.contains("access denied") {
                MtpConnectionError::PermissionDenied {
                    device_id: device_id.to_string(),
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
            MtpConnectionError::PermissionDenied {
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
        }
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
        assert!(err.to_string().contains("timed out"));
    }

    #[test]
    fn test_disconnected_error() {
        let err = MtpConnectionError::Disconnected {
            device_id: "mtp-1-5".to_string(),
        };
        assert!(err.to_string().contains("disconnected"));
    }

    #[test]
    fn test_permission_denied_error() {
        let err = MtpConnectionError::PermissionDenied {
            device_id: "mtp-1-5".to_string(),
        };
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
        assert!(err.to_string().contains("Custom error message"));
    }
}
