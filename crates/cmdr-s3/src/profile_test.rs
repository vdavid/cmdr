//! Each preset's endpoint and capabilities, and the session downgrade.

use url::Url;

use super::{
    Addressing, ConditionalOp, LocateError, Location, NoOverwrite, Preset, ProfileError, ProviderKind, ProviderProfile,
};
use crate::encoding::KeyError;
use crate::multipart::ShortTail;

fn profile(preset: Preset) -> ProviderProfile {
    ProviderProfile::from_preset(&preset).unwrap()
}

fn other(endpoint: &str, path_style: bool) -> Preset {
    Preset::Other {
        endpoint: Url::parse(endpoint).unwrap(),
        region: None,
        path_style,
    }
}

fn loc(host: &str, path: &str) -> Location {
    Location {
        host: host.into(),
        path: path.into(),
    }
}

#[test]
fn aws_is_regional_and_virtual_hosted() {
    let aws = profile(Preset::Aws {
        region: "eu-north-1".into(),
    });
    assert_eq!(aws.kind, ProviderKind::Aws);
    assert_eq!(aws.scheme, "https");
    assert_eq!(aws.endpoint_host, "s3.eu-north-1.amazonaws.com");
    assert_eq!(aws.region, "eu-north-1");
    assert_eq!(aws.addressing, Addressing::VirtualHosted);
    assert_eq!(
        aws.locate(Some("photos"), Some("2024/a b.jpg")).unwrap(),
        loc("photos.s3.eu-north-1.amazonaws.com", "/2024/a%20b.jpg")
    );
    assert_eq!(
        aws.locate(Some("photos"), None).unwrap(),
        loc("photos.s3.eu-north-1.amazonaws.com", "/")
    );
    assert_eq!(aws.locate(None, None).unwrap(), loc("s3.eu-north-1.amazonaws.com", "/"));
}

#[test]
fn aws_falls_back_to_path_style_for_bucket_names_that_break_virtual_hosting() {
    let aws = profile(Preset::Aws {
        region: "us-east-1".into(),
    });
    // A dot breaks the `*.s3…` TLS wildcard; uppercase and underscores are
    // legacy names that aren't DNS labels.
    for bucket in ["my.photos", "Legacy_Bucket", "ab"] {
        assert_eq!(
            aws.locate(Some(bucket), Some("k")).unwrap(),
            loc("s3.us-east-1.amazonaws.com", &format!("/{bucket}/k")),
            "{bucket}"
        );
    }
}

#[test]
fn r2_signs_for_auto_composes_keys_and_copies_with_its_own_header() {
    let r2 = profile(Preset::R2 {
        account_id: "0123456789abcdef0123456789abcdef".into(),
    });
    assert_eq!(
        r2.endpoint_host,
        "0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com"
    );
    assert_eq!(r2.region, "auto");
    assert_eq!(r2.addressing, Addressing::Path);
    assert!(r2.nfc_keys);
    // Each verified live on 2026-10-02: R2 enforces `If-None-Match` on a PUT
    // and a completion, and ignores it on a copy, which takes its own header.
    assert_eq!(r2.no_overwrite(ConditionalOp::Put), NoOverwrite::IfNoneMatch);
    assert_eq!(
        r2.no_overwrite(ConditionalOp::CompleteMultipart),
        NoOverwrite::IfNoneMatch
    );
    assert_eq!(r2.no_overwrite(ConditionalOp::Copy), NoOverwrite::CloudflareCopyHeader);
    // A last part larger than the rest is `InvalidPart` there.
    assert_eq!(r2.short_tail, ShortTail::Keep);
    assert_eq!(
        r2.locate(Some("b"), Some("x")).unwrap(),
        loc("0123456789abcdef0123456789abcdef.r2.cloudflarestorage.com", "/b/x")
    );
}

#[test]
fn r2_composes_a_decomposed_key_before_it_leaves() {
    let r2 = profile(Preset::R2 {
        account_id: "abc".into(),
    });
    let nfd = "He\u{301}llo";
    assert_eq!(r2.normalize_key(nfd), "H\u{e9}llo");
    assert_eq!(r2.locate(Some("b"), Some(nfd)).unwrap().path, "/b/H%C3%A9llo");
    // Elsewhere the key goes as given.
    let aws = profile(Preset::Aws {
        region: "us-east-1".into(),
    });
    assert_eq!(aws.normalize_key(nfd), nfd);
}

#[test]
fn aws_refuses_overwrites_by_header_on_all_three_writes() {
    let aws = profile(Preset::Aws {
        region: "us-east-1".into(),
    });
    for op in [
        ConditionalOp::Put,
        ConditionalOp::CompleteMultipart,
        ConditionalOp::Copy,
    ] {
        assert_eq!(aws.no_overwrite(op), NoOverwrite::IfNoneMatch, "{op:?}");
    }
    assert!(aws.cross_bucket_copy());
    assert!(!aws.nfc_keys);
}

#[test]
fn b2_has_no_conditional_writes() {
    let b2 = profile(Preset::B2 {
        region: "us-east-005".into(),
    });
    assert_eq!(b2.endpoint_host, "s3.us-east-005.backblazeb2.com");
    assert_eq!(b2.region, "us-east-005");
    for op in [
        ConditionalOp::Put,
        ConditionalOp::CompleteMultipart,
        ConditionalOp::Copy,
    ] {
        assert_eq!(b2.no_overwrite(op), NoOverwrite::CheckThenWrite, "{op:?}");
    }
}

/// GCS ignores `If-None-Match` but refuses an occupied key on its own
/// `x-goog-if-generation-match: 0` for a PUT and a `CopyObject` (412, old
/// bytes kept; 200 on a free key; live in two runs, 2026-10-02). Its multipart
/// completion ignores it and its initiate refuses it, so Complete checks first.
#[test]
fn gcs_refuses_by_generation_precondition_on_put_and_copy_and_checks_first_on_complete() {
    let gcs = profile(Preset::Gcs);
    assert_eq!(gcs.no_overwrite(ConditionalOp::Put), NoOverwrite::GoogGenerationMatch);
    assert_eq!(gcs.no_overwrite(ConditionalOp::Copy), NoOverwrite::GoogGenerationMatch);
    assert_eq!(
        gcs.no_overwrite(ConditionalOp::CompleteMultipart),
        NoOverwrite::CheckThenWrite
    );
}

#[test]
fn every_provider_off_the_allowlist_checks_then_writes() {
    // A server can ignore `If-None-Match` and answer 200 while overwriting
    // (Garage on all three, VersityGW on Copy), so nothing is probed.
    for preset in [
        Preset::Wasabi {
            region: "eu-central-2".into(),
        },
        Preset::Other {
            endpoint: Url::parse("http://127.0.0.1:17480").unwrap(),
            region: None,
            path_style: true,
        },
    ] {
        let unlisted = profile(preset);
        for op in [
            ConditionalOp::Put,
            ConditionalOp::CompleteMultipart,
            ConditionalOp::Copy,
        ] {
            assert_eq!(
                unlisted.no_overwrite(op),
                NoOverwrite::CheckThenWrite,
                "{:?} {op:?}",
                unlisted.kind
            );
        }
    }
}

#[test]
fn wasabi_and_hetzner_endpoints_and_hetzner_copies_within_a_bucket_only() {
    let wasabi = profile(Preset::Wasabi {
        region: "eu-central-2".into(),
    });
    assert_eq!(wasabi.endpoint_host, "s3.eu-central-2.wasabisys.com");
    assert_eq!(wasabi.addressing, Addressing::Path);
    assert!(wasabi.cross_bucket_copy());

    let hetzner = profile(Preset::Hetzner {
        location: "hel1".into(),
    });
    assert_eq!(hetzner.endpoint_host, "hel1.your-objectstorage.com");
    assert_eq!(hetzner.region, "hel1");
    // Copies between two buckets of one location (live, 2026-10-02).
    assert!(hetzner.cross_bucket_copy());
}

/// Hetzner and Spaces enforce `If-None-Match` on a PUT and ignore it on a
/// completion and a copy (live, 2026-10-02).
#[test]
fn hetzner_and_spaces_refuse_by_header_on_a_put_only() {
    for preset in [
        Preset::Hetzner {
            location: "nbg1".into(),
        },
        Preset::DigitalOcean { region: "fra1".into() },
    ] {
        let listed = profile(preset);
        assert_eq!(listed.no_overwrite(ConditionalOp::Put), NoOverwrite::IfNoneMatch);
        for op in [ConditionalOp::CompleteMultipart, ConditionalOp::Copy] {
            assert_eq!(
                listed.no_overwrite(op),
                NoOverwrite::CheckThenWrite,
                "{:?} {op:?}",
                listed.kind
            );
        }
    }
}

/// GCS has no `UploadPartCopy` (400 `NotImplemented`, live 2026-10-02), so a
/// server-side copy is one `CopyObject` whatever its size.
#[test]
fn only_gcs_copies_without_parts() {
    assert!(!profile(Preset::Gcs).copies_in_parts);
    assert!(
        profile(Preset::Aws {
            region: "us-east-1".into()
        })
        .copies_in_parts
    );
    assert!(profile(other("http://127.0.0.1:9000", true)).copies_in_parts);
}

/// Garage refuses a small `UploadPartCopy` source even as the last part, and
/// "Other" may be Garage; every preset keeps a short tail as its own part.
#[test]
fn only_other_folds_a_short_tail() {
    assert_eq!(
        profile(other("http://127.0.0.1:9000", true)).short_tail,
        ShortTail::Fold
    );
    for preset in [
        Preset::Aws {
            region: "us-east-1".into(),
        },
        Preset::B2 {
            region: "us-east-005".into(),
        },
        Preset::Wasabi {
            region: "eu-central-2".into(),
        },
        Preset::Hetzner {
            location: "nbg1".into(),
        },
        Preset::Gcs,
        Preset::DigitalOcean { region: "fra1".into() },
    ] {
        let listed = profile(preset);
        assert_eq!(listed.short_tail, ShortTail::Keep, "{:?}", listed.kind);
    }
}

#[test]
fn a_not_implemented_answer_downgrades_one_allowlisted_operation_once() {
    let aws = profile(Preset::Aws {
        region: "us-east-1".into(),
    });

    assert!(aws.downgrade(ConditionalOp::Put));
    assert!(
        !aws.downgrade(ConditionalOp::Put),
        "only the first call reports (and logs)"
    );

    assert_eq!(aws.no_overwrite(ConditionalOp::Put), NoOverwrite::CheckThenWrite);
    assert_eq!(
        aws.no_overwrite(ConditionalOp::Copy),
        NoOverwrite::IfNoneMatch,
        "other ops untouched"
    );
}

#[test]
fn other_takes_its_endpoint_port_scheme_and_path_style_as_given() {
    let local = profile(other("http://127.0.0.1:17480", true));
    assert_eq!(local.kind, ProviderKind::Other);
    assert_eq!(local.scheme, "http");
    assert_eq!(local.endpoint_host, "127.0.0.1:17480");
    assert_eq!(local.region, "us-east-1");
    assert_eq!(local.addressing, Addressing::Path);
    assert_eq!(
        local.locate(Some("b"), Some("k")).unwrap(),
        loc("127.0.0.1:17480", "/b/k")
    );

    // A default port is dropped, the way `Host` spells it.
    let minio = profile(other("https://s3.example.com:443/", false));
    assert_eq!(minio.endpoint_host, "s3.example.com");
    assert_eq!(minio.addressing, Addressing::VirtualHosted);
    assert_eq!(
        minio.locate(Some("bkt"), Some("k")).unwrap(),
        loc("bkt.s3.example.com", "/k")
    );

    let regional = profile(Preset::Other {
        endpoint: Url::parse("https://s3.example.com").unwrap(),
        region: Some("garage".into()),
        path_style: true,
    });
    assert_eq!(regional.region, "garage");
}

#[test]
fn an_endpoint_with_a_path_query_or_odd_scheme_is_refused() {
    for endpoint in [
        "https://s3.example.com/prefix",
        "https://s3.example.com/?x=1",
        "ftp://s3.example.com",
        "https://user@s3.example.com",
    ] {
        assert_eq!(
            ProviderProfile::from_preset(&other(endpoint, true)).err(),
            Some(ProfileError::InvalidEndpoint),
            "{endpoint}"
        );
    }
}

#[test]
fn a_region_or_account_that_cant_be_a_hostname_part_is_refused() {
    for preset in [
        Preset::Aws {
            region: "eu north".into(),
        },
        Preset::Aws { region: String::new() },
        Preset::R2 {
            account_id: "abc.evil.com/x".into(),
        },
        Preset::Hetzner {
            location: "FSN1".into(),
        },
        Preset::B2 {
            region: "us-east-005/".into(),
        },
    ] {
        assert_eq!(
            ProviderProfile::from_preset(&preset).err(),
            Some(ProfileError::InvalidHostPart),
            "{preset:?}"
        );
    }
}

#[test]
fn a_bad_bucket_or_key_is_refused_before_any_url_exists() {
    let aws = profile(Preset::Aws {
        region: "us-east-1".into(),
    });
    assert_eq!(aws.locate(Some(""), None), Err(LocateError::InvalidBucket));
    assert_eq!(aws.locate(Some("a/b"), None), Err(LocateError::InvalidBucket));
    assert_eq!(
        aws.locate(Some("b"), Some("x/../y")),
        Err(LocateError::Key(KeyError::DotSegment))
    );
    assert_eq!(aws.locate(Some("b"), Some("")), Err(LocateError::Key(KeyError::Empty)));
}

/// ❗ An allowlist, like conditional writes: only a provider with evidence that
/// it refuses a short body keeps writing an overwrite in place. VersityGW
/// publishes a cut-off PUT, and "Other" may be VersityGW. Every named preset
/// was seen refusing one live (Wasabi in two runs, 2026-10-02).
#[test]
fn every_preset_but_other_is_trusted_to_refuse_a_short_body() {
    let trusted = [
        Preset::Wasabi {
            region: "eu-central-2".into(),
        },
        Preset::Aws {
            region: "us-east-1".into(),
        },
        Preset::R2 {
            account_id: "abc123".into(),
        },
        Preset::B2 {
            region: "us-east-005".into(),
        },
        Preset::Hetzner {
            location: "nbg1".into(),
        },
        Preset::Gcs,
        Preset::DigitalOcean { region: "fra1".into() },
    ];
    for preset in trusted {
        let listed = profile(preset);
        assert!(listed.refuses_short_body, "{:?}", listed.kind);
    }
    let other = profile(Preset::Other {
        endpoint: Url::parse("http://127.0.0.1:17480").unwrap(),
        region: None,
        path_style: true,
    });
    assert!(!other.refuses_short_body);
}

/// ❗ An allowlist too: Hetzner, Spaces, and Wasabi ignore
/// `x-amz-copy-source-if-match` on `UploadPartCopy`, so a copy there HEADs its
/// source before completing. AWS, R2, and B2 were seen refusing a stale pin
/// with 412 and taking the current one (live, B2 in two runs, 2026-10-02).
#[test]
fn only_aws_r2_and_b2_are_trusted_to_enforce_the_copy_source_pin() {
    let enforcing = [
        Preset::Aws {
            region: "us-east-1".into(),
        },
        Preset::R2 {
            account_id: "abc123".into(),
        },
        Preset::B2 {
            region: "us-east-005".into(),
        },
    ];
    for preset in enforcing {
        let listed = profile(preset);
        assert!(listed.enforces_copy_source_pin, "{:?}", listed.kind);
    }
    let unverified = [
        Preset::Wasabi {
            region: "eu-central-2".into(),
        },
        Preset::Hetzner {
            location: "nbg1".into(),
        },
        Preset::Gcs,
        Preset::DigitalOcean { region: "fra1".into() },
        Preset::Other {
            endpoint: Url::parse("http://127.0.0.1:17480").unwrap(),
            region: None,
            path_style: true,
        },
    ];
    for preset in unverified {
        let unlisted = profile(preset);
        assert!(!unlisted.enforces_copy_source_pin, "{:?}", unlisted.kind);
    }
}

fn request_to(profile: &ProviderProfile, bucket: &str, key: &str) -> crate::request::S3Request {
    crate::ops::head_object(profile, bucket, key).unwrap()
}

#[test]
fn aws_reroutes_a_bucket_to_its_own_region_keeping_the_bucket_where_it_was() {
    let aws = profile(Preset::Aws {
        region: "eu-north-1".into(),
    });

    let virtual_hosted = aws.reroute(request_to(&aws, "photos", "a b.jpg"), "us-west-2").unwrap();
    assert_eq!(virtual_hosted.host, "photos.s3.us-west-2.amazonaws.com");
    assert_eq!(virtual_hosted.path, "/a%20b.jpg");

    let by_path = aws.reroute(request_to(&aws, "my.photos", "k"), "us-west-2").unwrap();
    assert_eq!(by_path.host, "s3.us-west-2.amazonaws.com");
    assert_eq!(by_path.path, "/my.photos/k");
}

/// Wasabi answers a wrong-region request the way AWS does and names the
/// bucket's region on every answer (live, 2026-10-02), so it routes too: by
/// path, to its own `s3.<region>.wasabisys.com`.
#[test]
fn wasabi_reroutes_a_bucket_to_its_own_regional_endpoint_by_path() {
    let wasabi = profile(Preset::Wasabi {
        region: "eu-central-1".into(),
    });
    let rerouted = wasabi
        .reroute(request_to(&wasabi, "photos", "a b.jpg"), "eu-west-1")
        .unwrap();
    assert_eq!(rerouted.host, "s3.eu-west-1.wasabisys.com");
    assert_eq!(rerouted.path, "/photos/a%20b.jpg");
}

/// ❗ An allowlist: a provider whose endpoint isn't per region, or whose
/// answers don't name the region, is never re-routed, whatever region an
/// answer claims.
#[test]
fn only_the_routing_allowlist_reroutes_and_only_to_a_region_a_hostname_can_carry() {
    for preset in [
        Preset::Aws {
            region: "eu-north-1".into(),
        },
        Preset::Wasabi {
            region: "eu-central-1".into(),
        },
    ] {
        let routed = profile(preset);
        assert!(routed.routes_by_region(), "{:?}", routed.kind);
        for junk in ["", "x.evil.com", "evil.com/", "EU-WEST-1"] {
            assert!(
                routed.reroute(request_to(&routed, "photos", "k"), junk).is_none(),
                "{:?} {junk:?}",
                routed.kind
            );
        }
    }
    for preset in [
        Preset::R2 {
            account_id: "abc123".into(),
        },
        Preset::B2 {
            region: "eu-central-003".into(),
        },
        Preset::Hetzner {
            location: "nbg1".into(),
        },
        Preset::Gcs,
        Preset::DigitalOcean { region: "fra1".into() },
        other("http://127.0.0.1:9000", true),
    ] {
        let unrouted = profile(preset);
        assert!(!unrouted.routes_by_region(), "{:?}", unrouted.kind);
        assert!(
            unrouted
                .reroute(request_to(&unrouted, "photos", "k"), "us-east-1")
                .is_none(),
            "{:?}",
            unrouted.kind
        );
    }
}

#[test]
fn gcs_is_one_global_path_style_endpoint_signed_for_auto() {
    let gcs = profile(Preset::Gcs);
    assert_eq!(gcs.kind, ProviderKind::Gcs);
    assert_eq!(gcs.endpoint_host, "storage.googleapis.com");
    assert_eq!(gcs.region, "auto");
    assert_eq!(gcs.addressing, Addressing::Path);
    // GCS bucket names may hold dots and underscores; the path carries them.
    assert_eq!(
        gcs.locate(Some("my.photos_2024"), Some("a b.jpg")).unwrap(),
        loc("storage.googleapis.com", "/my.photos_2024/a%20b.jpg")
    );
}

#[test]
fn spaces_is_regional_and_path_style() {
    let spaces = profile(Preset::DigitalOcean { region: "fra1".into() });
    assert_eq!(spaces.kind, ProviderKind::DigitalOcean);
    assert_eq!(spaces.endpoint_host, "fra1.digitaloceanspaces.com");
    assert_eq!(spaces.region, "fra1");
    assert_eq!(spaces.addressing, Addressing::Path);
    // "Cross-region and cross-cluster copies are not supported", and two
    // buckets of one region may sit on two clusters: stream between them.
    assert!(!spaces.cross_bucket_copy());
    assert_eq!(
        ProviderProfile::from_preset(&Preset::DigitalOcean {
            region: "fra1.evil.com/".into()
        })
        .err(),
        Some(ProfileError::InvalidHostPart)
    );
}

/// ❗ The characters a provider refuses in a key, from live evidence: GCS a
/// line break (`400 InvalidObjectName`), B2 any control character, a tab
/// included (`400 InvalidRequest`); everyone else stores them.
#[test]
fn gcs_and_b2_refuse_some_characters_in_a_key() {
    let gcs = profile(Preset::Gcs);
    assert_eq!(gcs.refused_key_char("a\nb"), Some('\n'));
    assert_eq!(gcs.refused_key_char("a\rb"), Some('\r'));
    assert_eq!(gcs.refused_key_char("a\tb"), None);
    let b2 = profile(Preset::B2 {
        region: "eu-central-003".into(),
    });
    assert_eq!(b2.refused_key_char("a\tb"), Some('\t'));
    assert_eq!(b2.refused_key_char("a\nb"), Some('\n'));
    assert_eq!(b2.refused_key_char("a\u{7f}b"), Some('\u{7f}'));
    assert_eq!(b2.refused_key_char("café 🦀 #?.txt"), None);
    let r2 = profile(Preset::R2 {
        account_id: "acct".into(),
    });
    assert_eq!(r2.refused_key_char("a\tb\nc"), None);
}
