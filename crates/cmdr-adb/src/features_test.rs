use super::*;
use crate::testing::{FAKE_SERIAL, FakeAdbServer, FakeTree};

#[test]
fn parses_the_four_flags_and_ignores_the_rest() {
    let f = DeviceFeatures::parse("shell_v2,cmd,stat_v2, ls_v2 ,fixed_push_mkdir,sendrecv_v2,openscreen_mdns");
    assert_eq!(f, DeviceFeatures::all());
    assert_eq!(DeviceFeatures::parse(""), DeviceFeatures::default());
    assert_eq!(
        DeviceFeatures::parse("cmd,stat_v2"),
        DeviceFeatures {
            stat_v2: true,
            ..DeviceFeatures::default()
        }
    );
}

#[tokio::test(flavor = "multi_thread")]
async fn fetch_reads_the_device_list() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    assert_eq!(
        DeviceFeatures::fetch(&server.endpoint(), FAKE_SERIAL).await.unwrap(),
        DeviceFeatures::all()
    );
    server.set_features("cmd,shell_v2");
    let f = DeviceFeatures::fetch(&server.endpoint(), FAKE_SERIAL).await.unwrap();
    assert!(f.shell_v2 && !f.stat_v2 && !f.ls_v2 && !f.sendrecv_v2);
    assert!(matches!(
        DeviceFeatures::fetch(&server.endpoint(), "ghost").await,
        Err(AdbError::Refused(_))
    ));
}
