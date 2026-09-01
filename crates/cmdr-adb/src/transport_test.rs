use super::*;
use crate::testing::{FakeAdbServer, FakeTree};

#[test]
fn frame_is_four_hex_digits_then_the_service() {
    assert_eq!(frame("host:version"), b"000chost:version".to_vec());
    assert_eq!(frame(""), b"0000".to_vec());
    let long = "x".repeat(0x1234);
    assert_eq!(&frame(&long)[..4], b"1234");
}

#[test]
fn hex_length_parses_either_case_and_rejects_junk() {
    assert_eq!(parse_hex_len(b"000c").unwrap(), 12);
    assert_eq!(parse_hex_len(b"00FF").unwrap(), 255);
    assert!(matches!(parse_hex_len(b"zzzz"), Err(AdbError::Protocol(_))));
}

#[test]
fn hex_message_round_trips_through_frame_parsing() {
    let msg = hex_message(b"hello");
    assert_eq!(&msg[..4], b"0005");
    assert_eq!(parse_hex_len(&[msg[0], msg[1], msg[2], msg[3]]).unwrap(), 5);
}

#[tokio::test(flavor = "multi_thread")]
async fn request_reads_okay_then_a_payload() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let mut conn = server.endpoint().connect().await.unwrap();
    conn.request("host:version").await.unwrap();
    assert_eq!(conn.read_hex_message().await.unwrap(), b"0029".to_vec());
    conn.shutdown().await;
}

#[tokio::test(flavor = "multi_thread")]
async fn fail_becomes_refused_with_the_message() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let mut conn = server.endpoint().connect().await.unwrap();
    let err = conn.request("host:no-such-service").await.unwrap_err();
    assert!(matches!(err, AdbError::Refused(msg) if msg.contains("no-such-service")));
}

#[tokio::test(flavor = "multi_thread")]
async fn binding_an_unknown_serial_is_refused_and_a_closed_socket_is_device_gone() {
    let server = FakeAdbServer::start(FakeTree::new()).await;
    let mut conn = server.endpoint().connect().await.unwrap();
    assert!(matches!(conn.bind_device("nope").await, Err(AdbError::Refused(_))));
    // The fake closes after a FAIL on transport; the next read sees EOF.
    let mut byte = [0u8; 1];
    assert!(matches!(conn.read_exact(&mut byte).await, Err(AdbError::DeviceGone)));
}
