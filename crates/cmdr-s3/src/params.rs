//! How to reach one S3 place, and how to reach it again.

use url::Url;

use crate::profile::{Preset, ProfileError, ProviderProfile};

/// The connect form's provider choice, with what each preset asks for.
///
/// ❗ The preset decides the endpoint, the signing region, and the addressing,
/// so the user never types an endpoint for a provider Cmdr knows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum S3Provider {
    /// Amazon S3, in one region (`eu-west-1`).
    Aws {
        /// The region the endpoint is in.
        region: String,
    },
    /// Cloudflare R2, by account ID.
    R2 {
        /// The 32-hex account ID from the R2 dashboard.
        account_id: String,
    },
    /// Backblaze B2, in one region (`us-west-004`).
    B2 {
        /// The region from the bucket's S3 endpoint.
        region: String,
    },
    /// Wasabi, in one region (`eu-central-1`).
    Wasabi {
        /// The region the endpoint is in.
        region: String,
    },
    /// Hetzner Object Storage, in one location (`fsn1`, `nbg1`, `hel1`).
    Hetzner {
        /// The location the endpoint is in.
        location: String,
    },
    /// Google Cloud Storage, through its S3-compatible XML API with HMAC keys.
    Gcs,
    /// DigitalOcean Spaces, in one region (`fra1`).
    DigitalOcean {
        /// The region the endpoint is in.
        region: String,
    },
    /// Any other S3-compatible server.
    Other {
        /// `http(s)://host[:port]`, nothing after it.
        endpoint: Url,
        /// The signing region; `us-east-1` when absent.
        region: Option<String>,
        /// Whether buckets go in the path rather than the host name.
        path_style: bool,
    },
}

impl S3Provider {
    /// The preset's fixed name (`aws`, `r2`, `b2`, `wasabi`, `hetzner`,
    /// `gcs`, `digitalocean`, `other`), for the PII-free `s3_connected` counter. ❌ Never a field.
    pub fn kind_name(&self) -> &'static str {
        match self {
            Self::Aws { .. } => "aws",
            Self::R2 { .. } => "r2",
            Self::B2 { .. } => "b2",
            Self::Wasabi { .. } => "wasabi",
            Self::Hetzner { .. } => "hetzner",
            Self::Gcs => "gcs",
            Self::DigitalOcean { .. } => "digitalocean",
            Self::Other { .. } => "other",
        }
    }

    /// The provider's profile: what it enforces and how it cuts parts.
    pub(crate) fn profile(&self) -> Result<ProviderProfile, ProfileError> {
        ProviderProfile::from_preset(&self.preset())
    }

    fn preset(&self) -> Preset {
        match self.clone() {
            Self::Aws { region } => Preset::Aws { region },
            Self::R2 { account_id } => Preset::R2 { account_id },
            Self::B2 { region } => Preset::B2 { region },
            Self::Wasabi { region } => Preset::Wasabi { region },
            Self::Hetzner { location } => Preset::Hetzner { location },
            Self::Gcs => Preset::Gcs,
            Self::DigitalOcean { region } => Preset::DigitalOcean { region },
            Self::Other {
                endpoint,
                region,
                path_style,
            } => Preset::Other {
                endpoint,
                region,
                path_style,
            },
        }
    }
}

/// A provider whose fields can't make an endpoint: a region, location, or
/// account ID with characters a host name can't carry, or an endpoint URL with
/// a path, a query, or credentials in it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InvalidProvider;

/// Everything needed to open (and later re-open) one S3 place.
///
/// ❗ No secret lives here. The secret access key comes from the
/// `CredentialStore` seam at the moment a client is built and dies with it.
#[derive(Debug, Clone)]
pub struct S3ConnectionParams {
    provider: S3Provider,
    /// The account's key. ❗ Part of the volume's identity: two keys on one
    /// endpoint may see different buckets.
    access_key_id: String,
    /// The place: one bucket, or `None` for the account root, which lists the
    /// buckets (and needs a key that may).
    bucket: Option<String>,
    scheme: String,
    host: String,
    port: u16,
    /// Whether Cmdr may re-probe unattended when a request finds the server
    /// gone. ❗ The user's own per-server switch, independent of whether a
    /// secret is remembered (`volume::UnattendedReconnect`).
    pub auto_reconnect: bool,
}

impl S3ConnectionParams {
    /// Params for one place, with "reconnect automatically" on. An empty bucket
    /// name means the account root.
    pub fn new(provider: S3Provider, access_key_id: &str, bucket: Option<&str>) -> Result<Self, InvalidProvider> {
        let (scheme, host, port) = endpoint_of(&provider)?;
        Ok(Self {
            provider,
            access_key_id: access_key_id.to_string(),
            bucket: bucket.filter(|bucket| !bucket.is_empty()).map(str::to_string),
            scheme,
            host,
            port,
            auto_reconnect: true,
        })
    }

    /// The provider this place is on.
    pub fn provider(&self) -> &S3Provider {
        &self.provider
    }

    /// The access key id this place signs in with.
    pub fn access_key_id(&self) -> &str {
        &self.access_key_id
    }

    /// The bucket this place is, or `None` for the account root.
    pub fn bucket(&self) -> Option<&str> {
        self.bucket.as_deref()
    }

    /// The endpoint's host name, as the provider spells it (`s3.eu-west-1.amazonaws.com`).
    pub fn host(&self) -> &str {
        &self.host
    }

    /// The endpoint's effective port: explicit, else the scheme's default.
    pub fn port(&self) -> u16 {
        self.port
    }

    /// How this account is keyed in the secret store:
    /// `s3+<scheme>://<host>:<port>`, scoped by the access key id.
    ///
    /// ❗ The ACCOUNT, never the place: every bucket under one key shares one
    /// secret. ❗ `s3+` keeps it apart from a WebDAV entry on the same host and
    /// port.
    pub fn credential_service(&self) -> String {
        format!("s3+{}://{}:{}", self.scheme, self.host, self.port)
    }

    /// The place's root, server-side: `/` or `/<bucket>`.
    pub fn remote_root(&self) -> String {
        match &self.bucket {
            Some(bucket) => format!("/{bucket}"),
            None => "/".to_string(),
        }
    }

    /// The provider profile requests are built against.
    pub(crate) fn profile(&self) -> Result<ProviderProfile, ProfileError> {
        ProviderProfile::from_preset(&self.provider.preset())
    }
}

/// The scheme, host, and effective port a provider's endpoint has.
fn endpoint_of(provider: &S3Provider) -> Result<(String, String, u16), InvalidProvider> {
    if let S3Provider::Other { endpoint, .. } = provider {
        // The profile refuses an endpoint with anything after the authority,
        // so the host and port here are the whole of it.
        ProviderProfile::from_preset(&provider.preset()).map_err(|_| InvalidProvider)?;
        let host = endpoint.host_str().ok_or(InvalidProvider)?;
        let port = endpoint.port_or_known_default().ok_or(InvalidProvider)?;
        return Ok((endpoint.scheme().to_string(), host.to_string(), port));
    }
    let profile = ProviderProfile::from_preset(&provider.preset()).map_err(|_| InvalidProvider)?;
    Ok((profile.scheme.clone(), profile.endpoint_host.clone(), 443))
}

#[cfg(test)]
#[path = "params_test.rs"]
mod params_test;
