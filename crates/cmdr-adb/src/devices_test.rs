use std::sync::Arc;
use std::time::Duration;

use tokio::sync::mpsc;

use super::*;
use crate::testing::{FAKE_SERIAL, FakeAdbServer, FakeTree, fake_device};

#[test]
fn parses_the_long_format() {
    let list = parse_device_list(
        "R5CT10ABCDE            device usb:1-1 product:beyond2lteeea model:SM_G975F device:beyond2 transport_id:3\n\
         emulator-5554          unauthorized transport_id:4\n",
    );
    assert_eq!(list.len(), 2);
    assert_eq!(list[0].serial, "R5CT10ABCDE");
    assert_eq!(list[0].state, AdbDeviceState::Ready);
    assert_eq!(list[0].product.as_deref(), Some("beyond2lteeea"));
    assert_eq!(list[0].model.as_deref(), Some("SM_G975F"));
    assert_eq!(list[0].device.as_deref(), Some("beyond2"));
    assert_eq!(list[0].transport_id, Some(3));
    assert_eq!(list[0].display_name(), "SM G975F");
    assert!(list[0].is_ready());
    assert_eq!(list[1].state, AdbDeviceState::Unauthorized);
    assert_eq!(list[1].model, None);
    assert_eq!(list[1].display_name(), "emulator-5554");
    assert!(!list[1].is_ready());
}

#[test]
fn parses_the_short_format_and_every_state_word() {
    let cases = [
        ("device", AdbDeviceState::Ready),
        ("unauthorized", AdbDeviceState::Unauthorized),
        ("offline", AdbDeviceState::Offline),
        ("no permissions", AdbDeviceState::NoPermissions),
        (
            "no permissions (user in plugdev group?); see [url]",
            AdbDeviceState::NoPermissions,
        ),
        ("connecting", AdbDeviceState::Connecting),
        ("authorizing", AdbDeviceState::Authorizing),
        ("recovery", AdbDeviceState::Recovery),
        ("bootloader", AdbDeviceState::Bootloader),
        ("sideload", AdbDeviceState::Sideload),
        ("weird", AdbDeviceState::Unknown),
    ];
    for (word, state) in cases {
        let list = parse_device_list(&format!("SER123\t{word}\n"));
        assert_eq!(list.len(), 1, "{word}");
        assert_eq!(list[0].state, state, "{word}");
        assert_eq!(list[0].serial, "SER123");
    }
    assert!(parse_device_list("").is_empty());
    assert!(parse_device_list("\n\n").is_empty());
}

#[test]
fn display_name_falls_back_to_product_then_serial() {
    let mut d = fake_device();
    assert_eq!(d.display_name(), "Fake Phone");
    d.model = None;
    assert_eq!(d.display_name(), "sdk_gphone64_arm64");
    d.product = None;
    assert_eq!(d.display_name(), FAKE_SERIAL);
}

#[tokio::test(flavor = "multi_thread")]
async fn list_devices_reads_the_long_fields() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let list = list_devices(&server.endpoint()).await.unwrap();
    assert_eq!(list, vec![fake_device()]);
}

fn device(serial: &str, state: AdbDeviceState) -> AdbDevice {
    AdbDevice {
        serial: serial.to_string(),
        state,
        product: None,
        model: Some(format!("Model_{serial}")),
        device: None,
        transport_id: None,
    }
}

async fn next(rx: &mut mpsc::UnboundedReceiver<Vec<AdbDevice>>) -> Vec<AdbDevice> {
    tokio::time::timeout(Duration::from_secs(10), rx.recv())
        .await
        .expect("a device list within 10 s")
        .expect("channel open")
}

#[tokio::test(flavor = "multi_thread")]
async fn tracker_delivers_pushes_and_reconnects_after_a_drop() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let (tx, mut rx) = mpsc::unbounded_channel();
    let on_change: Arc<dyn Fn(Vec<AdbDevice>) + Send + Sync> = Arc::new(move |list| {
        let _ = tx.send(list);
    });
    let tracker = track_devices_with(
        server.endpoint(),
        tokio::runtime::Handle::current(),
        on_change,
        TrackerBackoff {
            initial: Duration::from_millis(20),
            cap: Duration::from_millis(100),
        },
    );

    // The initial list, refetched with the long fields.
    assert_eq!(next(&mut rx).await, vec![fake_device()]);

    // A push reaches the listener with the full fields.
    let two = vec![fake_device(), device("SECOND", AdbDeviceState::Unauthorized)];
    server.push_devices(two.clone());
    assert_eq!(next(&mut rx).await, two);

    // The server drops every socket; the tracker reconnects and redelivers.
    server.drop_connections();
    assert_eq!(next(&mut rx).await, two);

    // And pushes keep flowing on the new socket.
    let three = vec![device("THIRD", AdbDeviceState::Ready)];
    server.push_devices(three.clone());
    assert_eq!(next(&mut rx).await, three);

    tracker.stop();
    drop(tracker);
    // Drain anything that was already in flight when the stop landed. ❗ Only a
    // real message continues the drain: once the stopped task drops the last
    // sender, `recv()` answers `None` with no wait, so an `is_ok()` loop here
    // spins forever instead of ending.
    while let Ok(Some(_)) = tokio::time::timeout(Duration::from_millis(100), rx.recv()).await {}

    // A push after the stop reaches nobody, either because nothing arrives or
    // because the task already dropped the only sender.
    server.push_devices(Vec::new());
    let stray = tokio::time::timeout(Duration::from_millis(200), rx.recv()).await;
    assert!(
        matches!(stray, Err(_) | Ok(None)),
        "stopped tracker stays silent, got {stray:?}"
    );
}
