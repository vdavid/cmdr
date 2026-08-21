//! The share's binary connection-state machine, and how it widens.

use super::*;
use crate::volume::test_support::*;

#[test]
fn connection_state_round_trip() {
    for state in [ConnectionState::Direct, ConnectionState::Disconnected] {
        assert_eq!(ConnectionState::from_u8(state as u8), state);
    }
}

/// The backend widens its binary state machine into the BACKEND-FACING enum
/// (`cmdr_fs::volume::host::events::VolumeConnection`), never straight into the
/// frontend's wire enum: `events::volume_mapping` owns that second hop for every
/// backend, and a `From` into a `network` type here welds the SMB backend and
/// `network/` into one module cycle (`network/DETAILS.md` § "The one edge that must
/// not come back").
#[test]
fn connection_state_widens_into_the_backend_facing_enum() {
    use cmdr_fs::volume::host::events::VolumeConnection;

    assert_eq!(
        VolumeConnection::from(ConnectionState::Direct),
        VolumeConnection::Connected
    );
    assert_eq!(
        VolumeConnection::from(ConnectionState::Disconnected),
        VolumeConnection::Disconnected
    );
}

#[test]
fn connection_state_unknown_value_defaults_to_disconnected() {
    // The internal state machine is binary; `1` (the old `OsMount`
    // discriminant) and any other unknown byte must decode as
    // `Disconnected`, the safe / "stop using smb2" state.
    assert_eq!(ConnectionState::from_u8(1), ConnectionState::Disconnected);
    assert_eq!(ConnectionState::from_u8(255), ConnectionState::Disconnected);
}
