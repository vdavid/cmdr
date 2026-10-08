//! Where a place's endpoint lands, and the store key every bucket under one key
//! shares.

use url::Url;

use super::{InvalidProvider, S3ConnectionParams, S3Provider};

fn aws(region: &str) -> S3Provider {
    S3Provider::Aws {
        region: region.to_string(),
    }
}

fn other(endpoint: &str) -> S3Provider {
    S3Provider::Other {
        endpoint: Url::parse(endpoint).expect("a valid test URL"),
        region: None,
        path_style: true,
    }
}

#[test]
fn a_preset_lands_on_its_https_endpoint() {
    let params = S3ConnectionParams::new(aws("eu-west-1"), "AKIAEXAMPLE", None).expect("a valid preset");
    assert_eq!(params.host(), "s3.eu-west-1.amazonaws.com");
    assert_eq!(params.port(), 443);
    assert_eq!(params.credential_service(), "s3+https://s3.eu-west-1.amazonaws.com:443");
}

#[test]
fn an_other_endpoint_keeps_its_scheme_and_port() {
    let params = S3ConnectionParams::new(other("http://127.0.0.1:14480"), "GK1", Some("cmdr-test")).expect("valid");
    assert_eq!(params.host(), "127.0.0.1");
    assert_eq!(params.port(), 14480);
    assert_eq!(params.credential_service(), "s3+http://127.0.0.1:14480");
    assert_eq!(params.remote_root(), "/cmdr-test");
}

#[test]
fn an_other_endpoint_without_a_port_takes_the_schemes() {
    let params = S3ConnectionParams::new(other("https://minio.example.com"), "K", None).expect("valid");
    assert_eq!(params.port(), 443);
}

#[test]
fn every_bucket_under_one_key_shares_one_secret() {
    // ❗ The secret belongs to the key, not to the place: a second bucket under
    // the same key must find the secret the first one stored.
    let photos = S3ConnectionParams::new(aws("eu-west-1"), "AKIAEXAMPLE", Some("photos")).expect("valid");
    let root = S3ConnectionParams::new(aws("eu-west-1"), "AKIAEXAMPLE", None).expect("valid");
    assert_eq!(photos.credential_service(), root.credential_service());
}

#[test]
fn an_empty_bucket_is_the_account_root() {
    let params = S3ConnectionParams::new(aws("eu-west-1"), "AKIAEXAMPLE", Some("")).expect("valid");
    assert_eq!(params.bucket(), None);
    assert_eq!(params.remote_root(), "/");
}

#[test]
fn a_region_that_could_redirect_the_host_is_refused() {
    // A typed `x.evil.com/` would otherwise send the request, and its
    // signature, somewhere else.
    assert_eq!(
        S3ConnectionParams::new(aws("x.evil.com/"), "AKIAEXAMPLE", None).err(),
        Some(InvalidProvider)
    );
}

#[test]
fn an_endpoint_with_a_path_is_refused() {
    assert_eq!(
        S3ConnectionParams::new(other("https://minio.example.com/bucket"), "K", None).err(),
        Some(InvalidProvider)
    );
}
