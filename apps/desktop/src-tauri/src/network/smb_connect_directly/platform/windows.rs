//! Windows: no system keychain Cmdr reads SMB passwords from yet (Credential Manager is the one
//! to add).

pub(crate) use super::unsupported::{connect_directly_with_system_saved_password, system_has_saved_password};
