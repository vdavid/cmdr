//! What one provider can and can't do, chosen by the connect form's preset.
//!
//! The preset fixes the endpoint, the signing region, and the addressing
//! style, plus what each provider enforces: conditional writes, short bodies,
//! server-side copy, and part shapes. Verified live on every named preset
//! (`crates/cmdr-s3/DETAILS.md` § "Verified providers").
//! Re-run `apps/desktop/test/s3-servers/live.sh` before changing an entry.
//!
//! ❗ Conditional writes are an allowlist, ❌ never a probe: a server can ignore
//! `If-None-Match: *` and answer 200 while overwriting (Garage on Put,
//! Complete, and Copy; VersityGW on Copy; `apps/desktop/test/s3-servers/README.md`),
//! so a success proves nothing. Only what a provider documents enforcing gets
//! a header; everything else checks then writes. A `501 NotImplemented` on an
//! allowlisted operation downgrades it to check-then-write for the rest of the
//! session, logged once.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, AtomicU8, AtomicUsize, Ordering};

use unicode_normalization::{UnicodeNormalization, is_nfc};
use url::Url;

use crate::encoding::{KeyError, encode_component, encode_key};
use crate::multipart::ShortTail;
use crate::request::S3Request;

/// The connect form's choice, with what each preset asks for.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum Preset {
    Aws {
        region: String,
    },
    /// Cloudflare R2. Jurisdictional (EU, FedRAMP) endpoints aren't offered yet.
    R2 {
        account_id: String,
    },
    B2 {
        region: String,
    },
    Wasabi {
        region: String,
    },
    /// `fsn1`, `nbg1`, or `hel1`.
    Hetzner {
        location: String,
    },
    /// Google Cloud Storage through its XML API, with HMAC keys: one global
    /// endpoint, whatever the bucket's location.
    Gcs,
    /// DigitalOcean Spaces, in one region (`fra1`).
    DigitalOcean {
        region: String,
    },
    /// Any other S3-compatible server: a raw endpoint, an optional region
    /// (`us-east-1` when absent), and whether to address buckets by path.
    Other {
        endpoint: Url,
        region: Option<String>,
        path_style: bool,
    },
}

/// Which provider a profile describes, without the preset's parameters.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ProviderKind {
    Aws,
    R2,
    B2,
    Wasabi,
    Hetzner,
    Gcs,
    DigitalOcean,
    Other,
}

/// How a bucket goes into a URL.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum Addressing {
    /// `https://endpoint/bucket/key`: one host for every bucket, so one
    /// connection pool and one TLS certificate.
    Path,
    /// `https://bucket.endpoint/key`: AWS's preferred form (path style is
    /// deprecated there, with no date set). A bucket name that isn't a plain
    /// DNS label (a dot breaks the TLS wildcard) still goes by path.
    VirtualHosted,
}

/// The three writes that can refuse to overwrite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum ConditionalOp {
    Put,
    CompleteMultipart,
    Copy,
}

/// Which characters a provider refuses in a key (live evidence,
/// `DETAILS.md` § "Verified providers").
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum KeyChars {
    /// Every character S3 allows.
    Any,
    /// CR and LF: GCS (`400 InvalidObjectName`).
    LineBreaks,
    /// Every control character, U+0000 to U+001F and DEL: B2 (`400
    /// InvalidRequest`, its catch-all, so the code alone can't say why).
    ControlChars,
}

/// How a no-overwrite write is made safe right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum NoOverwrite {
    /// `If-None-Match: *`; a clash answers 412.
    IfNoneMatch,
    /// R2's `cf-copy-destination-if-none-match: *` on `CopyObject`.
    CloudflareCopyHeader,
    /// GCS's `x-goog-if-generation-match: 0` ("only if no live object"), on a
    /// request signed in GCS's own dialect (`request::Dialect::Goog`); a clash
    /// answers 412.
    GoogGenerationMatch,
    /// HEAD the destination first, then write. Racy by nature: a clash noticed
    /// afterwards is reported to the user, never hidden.
    CheckThenWrite,
}

impl NoOverwrite {
    fn to_u8(self) -> u8 {
        match self {
            Self::IfNoneMatch => 0,
            Self::CloudflareCopyHeader => 1,
            Self::CheckThenWrite => 2,
            Self::GoogGenerationMatch => 3,
        }
    }

    fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::IfNoneMatch,
            1 => Self::CloudflareCopyHeader,
            3 => Self::GoogGenerationMatch,
            _ => Self::CheckThenWrite,
        }
    }
}

/// One conditional operation's answer, downgradable for the session.
#[derive(Debug)]
struct ConditionalCell {
    /// The allowlist entry (a [`NoOverwrite`]): a header for an operation the
    /// provider documents enforcing, `CheckThenWrite` for everything else.
    /// Atomic only so a Docker cell can point a fixture at the header path
    /// ([`ProviderProfile::trust_conditional_writes`]).
    listed: AtomicU8,
    downgraded: AtomicBool,
}

impl ConditionalCell {
    fn new(listed: NoOverwrite) -> Self {
        Self {
            listed: AtomicU8::new(listed.to_u8()),
            downgraded: AtomicBool::new(false),
        }
    }

    fn current(&self) -> NoOverwrite {
        if self.downgraded.load(Ordering::Relaxed) {
            NoOverwrite::CheckThenWrite
        } else {
            NoOverwrite::from_u8(self.listed.load(Ordering::Relaxed))
        }
    }
}

/// A preset that can't make a working endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ProfileError {
    /// A region, location, or account ID with characters a hostname can't
    /// carry (only `a–z`, `0–9`, and `-` pass).
    InvalidHostPart,
    /// The endpoint isn't `http(s)://host[:port]` with nothing after it.
    InvalidEndpoint,
}

/// Why a bucket or key can't be addressed.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum LocateError {
    /// Empty, or holding a `/`.
    InvalidBucket,
    Key(KeyError),
}

/// Where a request goes: the `Host` value and the encoded path.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct Location {
    pub host: String,
    pub path: String,
}

/// One provider's endpoint and capabilities for a session.
#[derive(Debug)]
pub(crate) struct ProviderProfile {
    pub kind: ProviderKind,
    /// `http` or `https`.
    pub scheme: String,
    /// The endpoint's host, with `:port` when it isn't the scheme's default.
    pub endpoint_host: String,
    /// The SigV4 signing region.
    pub region: String,
    pub addressing: Addressing,
    /// Whether `CopyObject` / `UploadPartCopy` may name a source in another
    /// bucket. Not on Spaces, which documents no cross-cluster copy; a
    /// cross-bucket move there streams through the Mac. Atomic only so a
    /// Docker cell can make a fixture behave that way
    /// ([`Self::forbid_cross_bucket_copy`]); read through
    /// [`Self::cross_bucket_copy`].
    cross_bucket_copy: AtomicBool,
    /// Whether the provider refuses a PUT whose body ends before its
    /// `Content-Length` and keeps the old object, as S3's contract says. An
    /// allowlist, like conditional writes: VersityGW publishes the bytes that
    /// arrived (`apps/desktop/test/s3-servers/README.md`), so an in-place
    /// overwrite cut off there loses the original. Elsewhere an overwrite of
    /// an existing object goes as a multipart upload (`DETAILS.md` §
    /// "Overwrites in parts"). The evidence per provider: `DETAILS.md` §
    /// "Providers".
    pub refuses_short_body: bool,
    /// Whether `UploadPartCopy` refuses a part whose source no longer matches
    /// `x-amz-copy-source-if-match`, so a source replaced mid-copy can't be
    /// stitched from two versions. An allowlist: Hetzner, Spaces, and Wasabi
    /// ignore the pin (live, `DETAILS.md` § "Verified providers"), so off it a copy in
    /// parts HEADs its source before completing (`volume/server_copy.rs`).
    pub enforces_copy_source_pin: bool,
    /// GCS answers `400 InvalidArgument` to a `CopyObject` pinned to a
    /// multipart ETag (`"…-2"`), quoted or not, and honours a single-part one
    /// (live, `live_copy_object_etags`, 2026-10-02), so such a copy goes
    /// unpinned there (`ops::copy_object`).
    pub refuses_multipart_copy_pin: bool,
    /// R2 stores keys NFC, so an NFD key and its NFC twin are one object
    /// there. Composing before sending keeps our own comparisons honest.
    pub nfc_keys: bool,
    /// The characters the provider refuses in a key, refused here before a
    /// request goes out ([`Self::refused_key_char`]).
    pub refused_key_chars: KeyChars,
    /// Whether the provider serves an object stored with `Content-Encoding:
    /// gzip` decompressed unless the read says `Accept-Encoding: gzip` (GCS's
    /// "decompressive transcoding"; live, 2026-10-02). The transport then adds
    /// that header, unsigned, to every GET and HEAD.
    pub transcodes_gzip: bool,
    /// What a multipart plan does with a tail under 5 MiB: R2 refuses a last
    /// part larger than the rest, Garage a small `UploadPartCopy` source.
    pub short_tail: ShortTail,
    /// Whether the provider has `UploadPartCopy`. GCS answers `400
    /// NotImplemented`, so a server-side copy there is one `CopyObject`
    /// whatever its size, up to S3's 5 GiB ceiling.
    pub copies_in_parts: bool,
    /// Parts a server-side copy keeps in flight at most (the AIMD window's
    /// ceiling). Atomic only so a live cell can measure other widths
    /// ([`Self::set_concurrency`]); read through [`Self::copy_concurrency`].
    copy_concurrency: AtomicUsize,
    /// Parts one upload keeps in flight, each one buffered whole in memory.
    upload_concurrency: AtomicUsize,
    put: ConditionalCell,
    complete: ConditionalCell,
    copy: ConditionalCell,
    /// How one region's endpoint is spelled, for a provider whose account
    /// root routes each bucket to its own region. An allowlist like the
    /// others: only a provider seen answering a wrong-region request with the
    /// bucket's region (AWS, Wasabi; `DETAILS.md` § "Providers") gets one, and
    /// without one nothing is ever re-routed.
    regional_host: Option<RegionalHost>,
}

impl ProviderProfile {
    pub(crate) fn from_preset(preset: &Preset) -> Result<Self, ProfileError> {
        use NoOverwrite::{CheckThenWrite, CloudflareCopyHeader, GoogGenerationMatch, IfNoneMatch};

        let profile = match preset {
            Preset::Aws { region } => {
                let mut aws = Self::https(
                    ProviderKind::Aws,
                    AWS_HOSTS.host(host_part(region)?),
                    region,
                    Addressing::VirtualHosted,
                    [IfNoneMatch; 3],
                );
                aws.regional_host = Some(AWS_HOSTS);
                aws.refuses_short_body = true;
                // Documented and seen live: a part copy whose source fails
                // the pin is 412.
                aws.enforces_copy_source_pin = true;
                aws
            }
            Preset::R2 { account_id } => {
                let mut r2 = Self::https(
                    ProviderKind::R2,
                    format!("{}.r2.cloudflarestorage.com", host_part(account_id)?),
                    "auto",
                    Addressing::Path,
                    [IfNoneMatch, IfNoneMatch, CloudflareCopyHeader],
                );
                r2.nfc_keys = true;
                r2.refuses_short_body = true;
                r2.enforces_copy_source_pin = true;
                r2
            }
            Preset::B2 { region } => {
                let mut b2 = Self::https(
                    ProviderKind::B2,
                    format!("s3.{}.backblazeb2.com", host_part(region)?),
                    region,
                    Addressing::Path,
                    [CheckThenWrite; 3],
                );
                b2.refuses_short_body = true;
                b2.enforces_copy_source_pin = true;
                b2.refused_key_chars = KeyChars::ControlChars;
                b2
            }
            Preset::Wasabi { region } => {
                let mut wasabi = Self::https(
                    ProviderKind::Wasabi,
                    WASABI_HOSTS.host(host_part(region)?),
                    region,
                    Addressing::Path,
                    [CheckThenWrite; 3],
                );
                // Answers a wrong-region request as AWS does and names the
                // bucket's region on every answer (live, 2026-10-02).
                wasabi.regional_host = Some(WASABI_HOSTS);
                wasabi.refuses_short_body = true;
                wasabi
            }
            Preset::Hetzner { location } => {
                let mut hetzner = Self::https(
                    ProviderKind::Hetzner,
                    format!("{}.your-objectstorage.com", host_part(location)?),
                    location,
                    Addressing::Path,
                    [IfNoneMatch, CheckThenWrite, CheckThenWrite],
                );
                hetzner.refuses_short_body = true;
                hetzner
            }
            // Path style: a GCS bucket name may hold dots (and underscores),
            // which a virtual host's TLS wildcard can't carry.
            Preset::Gcs => {
                // GCS ignores `If-None-Match` but enforces its own create-only
                // precondition on a PUT and a `CopyObject` (live, two runs,
                // 2026-10-02); its multipart completion ignores it.
                let mut gcs = Self::https(
                    ProviderKind::Gcs,
                    "storage.googleapis.com".to_string(),
                    "auto",
                    Addressing::Path,
                    [GoogGenerationMatch, CheckThenWrite, GoogGenerationMatch],
                );
                gcs.refuses_short_body = true;
                gcs.refuses_multipart_copy_pin = true;
                gcs.copies_in_parts = false;
                gcs.refused_key_chars = KeyChars::LineBreaks;
                gcs.transcodes_gzip = true;
                gcs
            }
            Preset::DigitalOcean { region } => {
                let mut spaces = Self::https(
                    ProviderKind::DigitalOcean,
                    format!("{}.digitaloceanspaces.com", host_part(region)?),
                    region,
                    Addressing::Path,
                    [IfNoneMatch, CheckThenWrite, CheckThenWrite],
                );
                spaces.refuses_short_body = true;
                // Spaces documents no cross-cluster copy, and two buckets of
                // one region may sit on two clusters.
                spaces.cross_bucket_copy = AtomicBool::new(false);
                spaces
            }
            Preset::Other {
                endpoint,
                region,
                path_style,
            } => {
                let (scheme, host, is_ip) = endpoint_parts(endpoint)?;
                let region = match region.as_deref() {
                    None => "us-east-1",
                    Some(r) if !r.is_empty() && !r.contains(|c: char| c.is_whitespace() || c == '/') => r,
                    Some(_) => return Err(ProfileError::InvalidHostPart),
                };
                // A bucket can't be a subdomain of an IP address.
                let addressing = if *path_style || is_ip {
                    Addressing::Path
                } else {
                    Addressing::VirtualHosted
                };
                let mut other = Self::https(ProviderKind::Other, host, region, addressing, [CheckThenWrite; 3]);
                other.scheme = scheme;
                // Garage refuses an `UploadPartCopy` source under 5 MiB even
                // as the last part, and "Other" may be Garage.
                other.short_tail = ShortTail::Fold;
                other
            }
        };
        Ok(profile)
    }

    fn https(
        kind: ProviderKind,
        endpoint_host: String,
        region: &str,
        addressing: Addressing,
        [put, complete, copy]: [NoOverwrite; 3],
    ) -> Self {
        Self {
            kind,
            scheme: "https".to_string(),
            endpoint_host,
            region: region.to_string(),
            addressing,
            cross_bucket_copy: AtomicBool::new(true),
            refuses_short_body: false,
            enforces_copy_source_pin: false,
            refuses_multipart_copy_pin: false,
            nfc_keys: false,
            refused_key_chars: KeyChars::Any,
            transcodes_gzip: false,
            short_tail: ShortTail::Keep,
            copies_in_parts: true,
            copy_concurrency: AtomicUsize::new(DEFAULT_COPY_CONCURRENCY),
            upload_concurrency: AtomicUsize::new(DEFAULT_UPLOAD_CONCURRENCY),
            put: ConditionalCell::new(put),
            complete: ConditionalCell::new(complete),
            copy: ConditionalCell::new(copy),
            regional_host: None,
        }
    }

    /// How a no-overwrite `op` is made safe right now.
    pub(crate) fn no_overwrite(&self, op: ConditionalOp) -> NoOverwrite {
        self.cell(op).current()
    }

    /// The server answered `501 NotImplemented` to `op`'s conditional header:
    /// fall back to check-then-write for the rest of the session. Logs the
    /// first time only, and says whether this call was that first time.
    pub(crate) fn downgrade(&self, op: ConditionalOp) -> bool {
        let first = !self.cell(op).downgraded.swap(true, Ordering::Relaxed);
        if first {
            log::info!(
                "S3 ({:?}, {}): {op:?} refused its conditional header; using check-then-write for this session",
                self.kind,
                self.endpoint_host
            );
        }
        first
    }

    /// Whether a server-side copy may read from another bucket than it writes
    /// to (not on Spaces).
    pub(crate) fn cross_bucket_copy(&self) -> bool {
        self.cross_bucket_copy.load(Ordering::Relaxed)
    }

    /// Makes this profile copy within one bucket only, the way Spaces' does,
    /// for a Docker cell proving that a cross-bucket copy streams instead.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn forbid_cross_bucket_copy(&self) {
        self.cross_bucket_copy.store(false, Ordering::Relaxed);
    }

    /// The most parts a server-side copy keeps in flight.
    pub(crate) fn copy_concurrency(&self) -> usize {
        self.copy_concurrency.load(Ordering::Relaxed)
    }

    /// The parts one upload keeps in flight (and so in memory).
    pub(crate) fn upload_concurrency(&self) -> usize {
        self.upload_concurrency.load(Ordering::Relaxed)
    }

    /// Runs copies and uploads at other widths, for a live cell measuring
    /// throughput. Each is at least one.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn set_concurrency(&self, copy: usize, upload: usize) {
        self.copy_concurrency.store(copy.max(1), Ordering::Relaxed);
        self.upload_concurrency.store(upload.max(1), Ordering::Relaxed);
    }

    /// The first character of `key` this provider refuses to store, if any.
    pub(crate) fn refused_key_char(&self, key: &str) -> Option<char> {
        match self.refused_key_chars {
            KeyChars::Any => None,
            KeyChars::LineBreaks => key.chars().find(|c| matches!(c, '\r' | '\n')),
            KeyChars::ControlChars => key.chars().find(|c| c.is_ascii_control()),
        }
    }

    /// The key as this provider stores it: NFC on R2, untouched elsewhere.
    pub(crate) fn normalize_key<'a>(&self, key: &'a str) -> Cow<'a, str> {
        if self.nfc_keys && !is_nfc(key) {
            Cow::Owned(key.nfc().collect())
        } else {
            Cow::Borrowed(key)
        }
    }

    /// Where a request about `bucket` (and `key`, for an object) goes. With no
    /// bucket, the account root (`ListBuckets`).
    pub(crate) fn locate(&self, bucket: Option<&str>, key: Option<&str>) -> Result<Location, LocateError> {
        let Some(bucket) = bucket else {
            return Ok(Location {
                host: self.endpoint_host.clone(),
                path: "/".to_string(),
            });
        };
        if bucket.is_empty() || bucket.contains('/') {
            return Err(LocateError::InvalidBucket);
        }
        let key = match key {
            Some(key) => Some(encode_key(&self.normalize_key(key)).map_err(LocateError::Key)?),
            None => None,
        };
        if self.addressing == Addressing::VirtualHosted && is_dns_label(bucket) {
            return Ok(Location {
                host: format!("{bucket}.{}", self.endpoint_host),
                path: format!("/{}", key.unwrap_or_default()),
            });
        }
        let bucket = encode_component(bucket);
        Ok(Location {
            host: self.endpoint_host.clone(),
            path: match key {
                Some(key) => format!("/{bucket}/{key}"),
                None => format!("/{bucket}"),
            },
        })
    }

    /// Whether an account root on this provider routes each bucket to its own
    /// region (`routing.rs`): on the allowlist, AWS and Wasabi.
    pub(crate) fn routes_by_region(&self) -> bool {
        self.regional_host.is_some()
    }

    /// `request` sent to `region`'s endpoint instead of this profile's, for a
    /// bucket that lives elsewhere: only the host changes (the bucket stays in
    /// the host or the path, wherever `locate` put it), and the caller signs
    /// for `region`. `None` off the routing allowlist, for a region a hostname
    /// can't carry, or for a request that isn't on this profile's endpoint.
    pub(crate) fn reroute(&self, mut request: S3Request, region: &str) -> Option<S3Request> {
        let regional = self.regional_host?.host(host_part(region).ok()?);
        let bucket_part = request.host.strip_suffix(&self.endpoint_host)?;
        request.host = format!("{bucket_part}{regional}");
        Some(request)
    }

    /// Sends `If-None-Match: *` on Put and Complete whatever the allowlist
    /// says, for a Docker cell that proves the header path against VersityGW
    /// (which honours both; `apps/desktop/test/s3-servers/README.md`). ❌
    /// Never in production: the allowlist is the only thing that may trust a
    /// server's header, because an ignored one answers 200 and overwrites.
    #[cfg(any(test, feature = "testing"))]
    pub(crate) fn trust_conditional_writes(&self) {
        for cell in [&self.put, &self.complete] {
            cell.listed.store(NoOverwrite::IfNoneMatch.to_u8(), Ordering::Relaxed);
            cell.downgraded.store(false, Ordering::Relaxed);
        }
    }

    fn cell(&self, op: ConditionalOp) -> &ConditionalCell {
        match op {
            ConditionalOp::Put => &self.put,
            ConditionalOp::CompleteMultipart => &self.complete,
            ConditionalOp::Copy => &self.copy,
        }
    }
}

/// Parts a server-side copy keeps in flight on every provider. 16 was the
/// fastest of 4, 8, and 16 on R2, Hetzner, and Spaces, with no throttle (a
/// 140 MiB copy in 8 MiB parts: R2 3.3 → 1.5 s, Hetzner 1.2 → 0.6 s, Spaces
/// 0.7 → 0.4 s; `live_throughput_by_part_width`, 2026-10-02). GCS copies
/// whole, so it doesn't apply there.
const DEFAULT_COPY_CONCURRENCY: usize = 16;

/// Parts an upload keeps in flight on every provider, each a whole part
/// (64 MiB at the floor) in memory. Four saturated a ~250 Mbit/s uplink on
/// R2, Hetzner, and Spaces (eight was no faster); GCS in `us-central1`, a
/// long way from Stockholm, still gained at eight, which isn't worth 512 MiB
/// of buffers (`live_throughput_by_part_width`, 2026-10-02).
const DEFAULT_UPLOAD_CONCURRENCY: usize = 4;

/// How a provider whose endpoint is per region spells one region's host:
/// `s3.<region><suffix>`. The preset's own endpoint and every reroute are
/// built from the same one.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct RegionalHost {
    suffix: &'static str,
}

impl RegionalHost {
    /// The host for `region`, which must already be a valid host part.
    fn host(self, region: &str) -> String {
        format!("s3.{region}{}", self.suffix)
    }
}

const AWS_HOSTS: RegionalHost = RegionalHost {
    suffix: ".amazonaws.com",
};

const WASABI_HOSTS: RegionalHost = RegionalHost {
    suffix: ".wasabisys.com",
};

/// A region, location, or account ID that goes into a hostname: lowercase
/// letters, digits, and `-` only, so it can't smuggle in a dot or a path.
fn host_part(text: &str) -> Result<&str, ProfileError> {
    if !text.is_empty()
        && text
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    {
        Ok(text)
    } else {
        Err(ProfileError::InvalidHostPart)
    }
}

/// An "Other" endpoint's scheme, its `Host` spelling, and whether that host
/// is an IP address. Only `http(s)://host[:port]` with nothing after it.
fn endpoint_parts(endpoint: &Url) -> Result<(String, String, bool), ProfileError> {
    let scheme = endpoint.scheme();
    let plain = (scheme == "http" || scheme == "https")
        && endpoint.username().is_empty()
        && endpoint.password().is_none()
        && endpoint.path() == "/"
        && endpoint.query().is_none()
        && endpoint.fragment().is_none();
    let host = endpoint.host().filter(|_| plain).ok_or(ProfileError::InvalidEndpoint)?;
    let is_ip = !matches!(host, url::Host::Domain(_));
    // `port()` is `None` for the scheme's default, which is how `Host` spells it.
    let host = match endpoint.port() {
        Some(port) => format!("{host}:{port}"),
        None => host.to_string(),
    };
    Ok((scheme.to_string(), host, is_ip))
}

/// A bucket name that can be a subdomain under one TLS wildcard: 3–63
/// lowercase letters, digits, and inner hyphens, no dots.
fn is_dns_label(bucket: &str) -> bool {
    let bytes = bucket.as_bytes();
    (3..=63).contains(&bytes.len())
        && bytes
            .iter()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || *b == b'-')
        && bytes.first().is_some_and(u8::is_ascii_alphanumeric)
        && bytes.last().is_some_and(u8::is_ascii_alphanumeric)
}

#[cfg(test)]
#[path = "profile_test.rs"]
mod profile_test;
