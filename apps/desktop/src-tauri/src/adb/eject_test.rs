//! Ejecting a phone, end to end: `eject.rs` routes the id to this provider, the
//! provider retires the volume, the row stays listed with nothing registered
//! behind it, and the next dial opens the phone as a NEW volume.

use std::sync::Arc;

use cmdr_adb::testing::FakeTree;

use super::device_provider;
use super::test_support::{a_listed_phone, dial, dials_seen, registry_and_provider_agree, retire_phone};
use super::volume_wiring::install_device_provider;
use crate::device_volumes::provider_for_volume_id;
use crate::file_system::volume::eject::{EjectAction, EjectContext, decide_eject_action, eject};
use crate::file_system::volume::manager::get_volume_manager;

/// Whether the volume list has a row for `volume_id` (`None` when it doesn't),
/// and whether that row carries `capabilities`, which enrichment fills only for
/// a registered volume.
async fn row_has_capabilities(volume_id: &str) -> Option<bool> {
    crate::volume_listing::complete(Vec::new())
        .await
        .into_iter()
        .find(|row| row.id == volume_id)
        .map(|row| row.capabilities.is_some())
}

/// ❗ Eject on a phone detaches nothing (`adb` has no per-client detach), so
/// the whole round trip is app state: the registry lets the volume go, the
/// provider forgets it, and the row stays behind WITHOUT capabilities, which is
/// what tells a pane standing on it to dial again. A provider that forgot
/// nothing would hand the next dial the ejected volume back without touching
/// the wire.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn ejecting_a_phone_retires_its_volume_keeps_its_row_and_the_next_dial_opens_a_new_one() {
    const SERIAL: &str = "R58M-Eject-Round-Trip";
    install_device_provider();
    let fake = a_listed_phone(SERIAL, FakeTree::new()).await;
    let (volume_id, ejected) = dial(&fake, SERIAL, "adb-eject-first-dial").await;
    assert_eq!(
        row_has_capabilities(&volume_id).await,
        Some(true),
        "precondition: a dialed phone's row carries its capabilities"
    );

    let provider = provider_for_volume_id(&volume_id)
        .await
        .expect("the ADB provider owns a listed phone's id");
    let action = decide_eject_action(&EjectContext {
        volume_id: &volume_id,
        is_ejectable: false,
        is_smb: false,
        device_provider: Some(provider.id()),
    })
    .expect("a device volume always has an eject action");
    assert_eq!(
        action,
        EjectAction::DeviceDisconnect {
            provider: "adb",
            volume_id: volume_id.clone()
        }
    );

    eject(&volume_id).await.expect("the phone ejects");

    assert!(
        get_volume_manager().get(&volume_id).is_none(),
        "the registry lets the ejected volume go"
    );
    assert!(
        device_provider::connected_volume(SERIAL).is_none(),
        "the provider forgets it"
    );
    assert_eq!(
        row_has_capabilities(&volume_id).await,
        Some(false),
        "the phone stays listed, now without capabilities"
    );

    let (reopened_id, reopened) = dial(&fake, SERIAL, "adb-eject-second-dial").await;
    assert_eq!(reopened_id, volume_id, "the same phone, the same id");
    assert_eq!(
        dials_seen(&fake),
        2,
        "the second open goes over the wire; requests: {:?}",
        fake.requests()
    );
    assert!(
        !std::ptr::addr_eq(Arc::as_ptr(&ejected), Arc::as_ptr(&reopened)),
        "the reopened phone is a new volume, never the ejected one handed back"
    );
    assert!(
        registry_and_provider_agree(SERIAL),
        "and the provider remembers the new one"
    );

    retire_phone(SERIAL);
    device_provider::apply_device_list(Vec::new());
}
