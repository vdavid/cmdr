//! An AWS account root reaching a bucket in another region, against a fake
//! AWS: a local server that every `*.amazonaws.com` host the client dials
//! resolves to, answering the way S3 does (Docker fixtures have one region,
//! so they can't produce the 301).
//!
//! The fake knows one bucket per region. A request whose host names another
//! region gets `301 PermanentRedirect` with `x-amz-bucket-region` (unless the
//! cell turned the header off for everything but `HeadBucket`, which always
//! carries it); one signed for the wrong region on the right host gets `400
//! AuthorizationHeaderMalformed`.

use std::net::SocketAddr;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use http::{HeaderValue, Method, StatusCode};
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpListener;

use super::S3Client;
use crate::ops;
use crate::profile::{Preset, ProviderProfile};
use crate::request::{Body, S3Request};
use crate::sigv4::Credentials;

const HOME: &str = "us-east-1";
const FAR: &str = "eu-west-1";
const BUDGET: Duration = Duration::from_secs(5);

/// One request the fake saw.
#[derive(Debug, Clone)]
struct Seen {
    method: String,
    host: String,
    /// The region the `Authorization` header's scope names.
    signed_for: String,
    body_len: usize,
}

#[derive(Default)]
struct World {
    seen: Vec<Seen>,
    /// Whether a misrouted answer other than `HeadBucket`'s names the region.
    redirect_names_region: bool,
    /// Answers on any host, judging only the signature's region, the way a
    /// request that reached the right endpoint signed for the wrong region is.
    any_host: bool,
}

struct FakeAws {
    addr: SocketAddr,
    world: Arc<Mutex<World>>,
}

impl FakeAws {
    async fn start(redirect_names_region: bool) -> Self {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();
        let world = Arc::new(Mutex::new(World {
            seen: Vec::new(),
            redirect_names_region,
            any_host: false,
        }));
        let serving = Arc::clone(&world);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let world = Arc::clone(&serving);
                tokio::spawn(async move {
                    let Some((head, body_len)) = read_request(&mut socket).await else {
                        return;
                    };
                    let response = answer(&world, &head, body_len);
                    let _ = socket.write_all(response.as_bytes()).await;
                    let _ = socket.shutdown().await;
                });
            }
        });
        Self { addr, world }
    }

    /// A client for the account root, its region `HOME`, dialing this fake.
    fn client(&self, route_each_bucket: bool) -> S3Client {
        let mut profile = ProviderProfile::from_preset(&Preset::Aws { region: HOME.into() }).unwrap();
        profile.scheme = "http".into();
        let hosts = [
            format!("s3.{HOME}.amazonaws.com"),
            format!("s3.{FAR}.amazonaws.com"),
            format!("near.s3.{HOME}.amazonaws.com"),
            format!("near.s3.{FAR}.amazonaws.com"),
            format!("far.s3.{HOME}.amazonaws.com"),
            format!("far.s3.{FAR}.amazonaws.com"),
        ];
        let mut client = S3Client::resolving(profile, Credentials::new("AKIDEXAMPLE", "secret"), &hosts, self.addr);
        if route_each_bucket {
            client.route_each_bucket();
        }
        client
    }

    fn seen(&self) -> Vec<Seen> {
        self.world.lock().unwrap().seen.clone()
    }

    /// The hosts the requests went to, in order.
    fn hosts(&self) -> Vec<String> {
        self.seen().into_iter().map(|seen| seen.host).collect()
    }
}

/// The request head (lowercased header names kept as sent) and how many body
/// bytes followed it.
async fn read_request(socket: &mut tokio::net::TcpStream) -> Option<(String, usize)> {
    let mut buffer = Vec::new();
    let mut chunk = [0u8; 4096];
    let head_end = loop {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            return None;
        }
        buffer.extend_from_slice(&chunk[..read]);
        if let Some(at) = buffer.windows(4).position(|w| w == b"\r\n\r\n") {
            break at + 4;
        }
    };
    let head = String::from_utf8_lossy(&buffer[..head_end]).into_owned();
    let length: usize = header_of(&head, "content-length")
        .and_then(|v| v.parse().ok())
        .unwrap_or(0);
    let mut body_len = buffer.len() - head_end;
    while body_len < length {
        let read = socket.read(&mut chunk).await.ok()?;
        if read == 0 {
            break;
        }
        body_len += read;
    }
    Some((head, body_len))
}

fn header_of<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().skip(1).find_map(|line| {
        let (key, value) = line.split_once(':')?;
        key.trim().eq_ignore_ascii_case(name).then(|| value.trim())
    })
}

/// What S3 would say to this request.
fn answer(world: &Mutex<World>, head: &str, body_len: usize) -> String {
    let mut parts = head.lines().next().unwrap_or_default().split(' ');
    let method = parts.next().unwrap_or_default().to_string();
    let path = parts.next().unwrap_or_default().to_string();
    let host = header_of(head, "host").unwrap_or_default().to_string();
    let signed_for = header_of(head, "authorization")
        .and_then(|auth| auth.split("Credential=").nth(1))
        .and_then(|credential| credential.split('/').nth(2))
        .unwrap_or_default()
        .to_string();
    let mut world = world.lock().unwrap();
    world.seen.push(Seen {
        method: method.clone(),
        host: host.clone(),
        signed_for: signed_for.clone(),
        body_len,
    });
    let host_region = host
        .split(".s3.")
        .nth(1)
        .and_then(|rest| rest.strip_suffix(".amazonaws.com"));
    let bucket = host
        .split(".s3.")
        .next()
        .filter(|_| host.contains(".s3."))
        .unwrap_or_default();
    let lives_in = match bucket {
        "far" => FAR,
        _ => HOME,
    };
    let head_bucket = method == "HEAD" && path == "/";
    let names_region = head_bucket || world.redirect_names_region;
    if !world.any_host && host_region != Some(lives_in) {
        let header = if names_region {
            format!("x-amz-bucket-region: {lives_in}\r\n")
        } else {
            String::new()
        };
        return respond(
            301,
            &header,
            "<Error><Code>PermanentRedirect</Code><Message>m</Message></Error>",
            &method,
        );
    }
    if signed_for != lives_in {
        let body = format!(
            "<Error><Code>AuthorizationHeaderMalformed</Code><Message>m</Message><Region>{lives_in}</Region></Error>"
        );
        return respond(400, "", &body, &method);
    }
    let header = format!("x-amz-bucket-region: {lives_in}\r\netag: \"e\"\r\n");
    respond(200, &header, if method == "GET" { "hello" } else { "" }, &method)
}

fn respond(status: u16, headers: &str, body: &str, method: &str) -> String {
    let body = if method == "HEAD" { "" } else { body };
    format!(
        "HTTP/1.1 {status} X\r\ncontent-length: {}\r\nconnection: close\r\n{headers}\r\n{body}",
        body.len()
    )
}

fn get(client: &S3Client, bucket: &str) -> S3Request {
    ops::get_object(client.profile(), bucket, "k", None).unwrap()
}

#[tokio::test]
async fn a_bucket_in_another_region_is_reached_after_one_redirect_and_directly_after_that() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(true);

    let first = client.exchange(get(&client, "far"), BUDGET).await.unwrap();
    assert_eq!(first.status, StatusCode::OK);
    assert_eq!(first.body, b"hello");
    let second = client.exchange(get(&client, "far"), BUDGET).await.unwrap();
    assert_eq!(second.status, StatusCode::OK);

    assert_eq!(
        aws.hosts(),
        [
            format!("far.s3.{HOME}.amazonaws.com"),
            format!("far.s3.{FAR}.amazonaws.com"),
            format!("far.s3.{FAR}.amazonaws.com"),
        ]
    );
    let signed: Vec<String> = aws.seen().into_iter().map(|seen| seen.signed_for).collect();
    assert_eq!(signed, [HOME, FAR, FAR]);
}

#[tokio::test]
async fn a_bucket_in_the_home_region_costs_no_extra_request() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(true);

    let answer = client.exchange(get(&client, "near"), BUDGET).await.unwrap();

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(aws.hosts(), [format!("near.s3.{HOME}.amazonaws.com")]);
}

#[tokio::test]
async fn a_redirect_that_names_no_region_asks_head_bucket_which_always_does() {
    let aws = FakeAws::start(false).await;
    let client = aws.client(true);

    let answer = client.exchange(get(&client, "far"), BUDGET).await.unwrap();

    assert_eq!(answer.status, StatusCode::OK);
    let methods: Vec<(String, String)> = aws.seen().into_iter().map(|s| (s.method, s.host)).collect();
    assert_eq!(
        methods,
        [
            ("GET".to_string(), format!("far.s3.{HOME}.amazonaws.com")),
            ("HEAD".to_string(), format!("far.s3.{HOME}.amazonaws.com")),
            ("GET".to_string(), format!("far.s3.{FAR}.amazonaws.com")),
        ]
    );
}

#[tokio::test]
async fn a_region_listed_upfront_is_used_from_the_first_request() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(true);

    client.learn_bucket_region("far", FAR);
    let answer = client.exchange(get(&client, "far"), BUDGET).await.unwrap();

    assert_eq!(answer.status, StatusCode::OK);
    assert_eq!(aws.hosts(), [format!("far.s3.{FAR}.amazonaws.com")]);
}

#[tokio::test]
async fn a_wrong_signing_region_is_corrected_from_the_error_body() {
    let aws = FakeAws::start(true).await;
    aws.world.lock().unwrap().any_host = true;
    let client = aws.client(true);

    let answer = client.exchange(get(&client, "far"), BUDGET).await.unwrap();

    assert_eq!(answer.status, StatusCode::OK);
    let signed: Vec<String> = aws.seen().into_iter().map(|seen| seen.signed_for).collect();
    assert_eq!(signed, [HOME, FAR]);
}

#[tokio::test]
async fn a_bucket_place_leaves_the_redirect_for_the_connect_refusal() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(false);

    let answer = client.exchange(get(&client, "far"), BUDGET).await.unwrap();

    assert_eq!(answer.status, StatusCode::MOVED_PERMANENTLY);
    assert_eq!(aws.hosts(), [format!("far.s3.{HOME}.amazonaws.com")]);
}

#[tokio::test]
async fn a_streamed_get_follows_the_bucket_too() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(true);

    let mut opened = client.open(get(&client, "far"), "v", "/far/k").await.unwrap();

    assert_eq!(opened.status, StatusCode::OK);
    let mut body = Vec::new();
    while let Some(chunk) = opened.chunk().await {
        body.extend(chunk.unwrap());
    }
    assert_eq!(body, b"hello");
    assert_eq!(aws.hosts().last().unwrap(), &format!("far.s3.{FAR}.amazonaws.com"));
}

#[tokio::test]
async fn an_upload_to_a_bucket_of_unknown_region_asks_first_so_its_body_goes_once() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(true);
    let mut request = S3Request::new(
        Method::PUT,
        "http",
        &format!("far.s3.{HOME}.amazonaws.com"),
        "/k".into(),
    );
    request.bucket = Some("far".into());
    request.body = Body::Streamed { length: 5 };
    request = request.header(
        http::header::CONTENT_TYPE,
        HeaderValue::from_static("application/octet-stream"),
    );
    let body: super::UploadBody = Box::pin(futures_util::stream::iter([Ok(bytes::Bytes::from_static(b"hello"))]));

    let answer = client.upload(request, body).await.unwrap();

    assert_eq!(answer.status, StatusCode::OK);
    let seen = aws.seen();
    let trail: Vec<(&str, &str, usize)> = seen
        .iter()
        .map(|s| (s.method.as_str(), s.host.as_str(), s.body_len))
        .collect();
    let far_host = format!("far.s3.{FAR}.amazonaws.com");
    let home_host = format!("far.s3.{HOME}.amazonaws.com");
    assert_eq!(trail, [("HEAD", home_host.as_str(), 0), ("PUT", far_host.as_str(), 5)]);
}

#[tokio::test]
async fn a_share_link_is_signed_for_the_buckets_own_region_and_host() {
    let aws = FakeAws::start(true).await;
    let client = aws.client(true);
    client.learn_bucket_region("far", FAR);

    let url = client.share_link("far", "k", Duration::from_secs(60)).await.unwrap();

    assert_eq!(url.host_str(), Some(format!("far.s3.{FAR}.amazonaws.com").as_str()));
    let credential = url
        .query_pairs()
        .find(|(name, _)| name == "X-Amz-Credential")
        .map(|(_, value)| value.into_owned())
        .unwrap();
    assert!(credential.contains(&format!("/{FAR}/s3/")), "{credential}");
}

/// ❗ Only a provider on the profile's routing allowlist routes per bucket;
/// everyone else sends every request to the endpoint the user named.
#[test]
fn routing_is_for_the_routing_allowlist_only() {
    let client = |preset: Preset| {
        let mut client = S3Client::new(
            ProviderProfile::from_preset(&preset).unwrap(),
            Credentials::new("AKIDEXAMPLE", "secret"),
        )
        .unwrap();
        client.route_each_bucket();
        client
    };
    let wasabi = client(Preset::Wasabi {
        region: "eu-central-1".into(),
    });
    assert!(wasabi.routes_each_bucket());
    for preset in [
        Preset::Hetzner {
            location: "nbg1".into(),
        },
        Preset::B2 {
            region: "eu-central-003".into(),
        },
        Preset::DigitalOcean { region: "fra1".into() },
        Preset::Other {
            endpoint: url::Url::parse("http://127.0.0.1:9000").unwrap(),
            region: None,
            path_style: true,
        },
    ] {
        assert!(!client(preset.clone()).routes_each_bucket(), "{preset:?}");
    }
}
