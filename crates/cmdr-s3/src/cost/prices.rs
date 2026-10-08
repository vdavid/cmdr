//! The price table: what each provider charges at list prices, in the shape
//! `apps/api-server` serves at `/s3-prices/v1` and this crate bundles as the
//! fallback (`s3-prices.json`).
//!
//! ❗ The two copies stay byte-identical (`apps/api-server` tests it): a price
//! change edits both, the Worker deploy carries it to every install at once, and
//! the next release carries it into the fallback.

use std::collections::HashMap;

use serde::Deserialize;

/// The schema version this build reads. A breaking change to the shape bumps it
/// and moves the endpoint to `/s3-prices/v2`.
const SCHEMA_VERSION: u32 = 1;

/// The bundled copy.
const BUNDLED: &str = include_str!("s3-prices.json");

/// Every billed request this backend sends, by its S3 operation name. The table
/// names them the same way, and every provider must price each one.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub(crate) enum RequestKind {
    /// A one-request upload.
    PutObject,
    /// A one-request server-side copy.
    CopyObject,
    /// The start of an upload or copy in parts.
    CreateMultipartUpload,
    /// One uploaded part.
    UploadPart,
    /// One part copied on the server.
    UploadPartCopy,
    /// The end of an upload or copy in parts.
    CompleteMultipartUpload,
    /// One page of a listing.
    ListObjectsV2,
    /// A download.
    GetObject,
    /// A stat: before a copy, and to verify every write.
    HeadObject,
    /// One object or folder marker deleted.
    DeleteObject,
    /// Up to 1,000 objects deleted in one request.
    DeleteObjects,
    /// An unfinished upload cancelled.
    AbortMultipartUpload,
}

impl RequestKind {
    pub(crate) const ALL: [Self; 12] = [
        Self::PutObject,
        Self::CopyObject,
        Self::CreateMultipartUpload,
        Self::UploadPart,
        Self::UploadPartCopy,
        Self::CompleteMultipartUpload,
        Self::ListObjectsV2,
        Self::GetObject,
        Self::HeadObject,
        Self::DeleteObject,
        Self::DeleteObjects,
        Self::AbortMultipartUpload,
    ];

    /// The operation's name as S3's API reference and the table spell it.
    pub(crate) fn name(self) -> &'static str {
        match self {
            Self::PutObject => "PutObject",
            Self::CopyObject => "CopyObject",
            Self::CreateMultipartUpload => "CreateMultipartUpload",
            Self::UploadPart => "UploadPart",
            Self::UploadPartCopy => "UploadPartCopy",
            Self::CompleteMultipartUpload => "CompleteMultipartUpload",
            Self::ListObjectsV2 => "ListObjectsV2",
            Self::GetObject => "GetObject",
            Self::HeadObject => "HeadObject",
            Self::DeleteObject => "DeleteObject",
            Self::DeleteObjects => "DeleteObjects",
            Self::AbortMultipartUpload => "AbortMultipartUpload",
        }
    }

    fn from_name(name: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|kind| kind.name() == name)
    }
}

/// Every provider's list prices, validated.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceTable {
    /// By the table's provider key (`S3Provider::kind_name`: `aws`, `r2`, `b2`,
    /// `wasabi`, `hetzner`, `gcs`, `digitalocean`).
    providers: HashMap<String, ProviderPrices>,
}

/// Why a table can't be used. The caller keeps the copy it already has and logs
/// this; nothing downstream branches on which problem it was, so the problem
/// stays inside the crate.
#[derive(Debug, Clone, PartialEq)]
pub struct PriceTableError(pub(crate) TableProblem);

impl std::fmt::Display for PriceTableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            TableProblem::Malformed(detail) => write!(f, "the price table isn't valid JSON of its schema: {detail}"),
            TableProblem::UnsupportedVersion(version) => {
                write!(f, "price table schema {version} is newer than this build")
            }
            TableProblem::InvalidProvider { provider, problem } => {
                write!(f, "the price table's `{provider}` entry is unusable: {problem:?}")
            }
        }
    }
}

impl std::error::Error for PriceTableError {}

/// What's wrong with a table.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum TableProblem {
    /// Not JSON, or not this schema's shape. The detail is serde's, for the log.
    Malformed(String),
    /// A schema version this build doesn't read.
    UnsupportedVersion(u32),
    /// One provider's entry can't price every operation honestly.
    InvalidProvider { provider: String, problem: ProviderProblem },
}

/// What makes a provider's entry unusable.
#[derive(Debug, Clone, PartialEq)]
pub(crate) enum ProviderProblem {
    /// No request class names this operation.
    Unpriced(RequestKind),
    /// Two request classes name it.
    PricedTwice(RequestKind),
    /// A price or minimum that's negative or not a finite number, by its JSON
    /// field name.
    BadNumber(&'static str),
}

/// One provider's validated prices.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct ProviderPrices {
    pub label: String,
    /// ISO 4217 (`USD`, `EUR`).
    pub currency: String,
    pub request_classes: Vec<RequestClass>,
    /// Index into `request_classes` per operation; every [`RequestKind`] has one.
    pub class_of: HashMap<RequestKind, usize>,
    /// Per GiB downloaded, at the price a normal account pays. Allowances (the
    /// free first 100 GB, "free up to your storage") are notes, not math.
    pub egress_per_gb: f64,
    /// Per GiB stored for 30 days.
    pub storage_per_gb_month: f64,
    /// An object deleted younger than this bills the remaining days.
    pub minimum_storage_days: u32,
    /// An object smaller than this bills as this.
    pub minimum_billable_object_bytes: u64,
}

/// One request class: a name as the provider's page spells it, and its price.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct RequestClass {
    pub name: String,
    pub per_million: f64,
}

impl PriceTable {
    /// Parses and validates a table: the schema version, and that every
    /// provider prices every operation exactly once with sane numbers. Unknown
    /// providers, operations, and fields are ignored, so a newer server stays
    /// readable.
    pub fn parse(json: &str) -> Result<Self, PriceTableError> {
        // The version first, on its own: a v2 table may not fit v1's shape, and
        // "a newer schema" is the more useful thing to log than serde's detail.
        let version: VersionOnly =
            serde_json::from_str(json).map_err(|e| PriceTableError(TableProblem::Malformed(e.to_string())))?;
        if version.schema_version != SCHEMA_VERSION {
            return Err(PriceTableError(TableProblem::UnsupportedVersion(
                version.schema_version,
            )));
        }
        let raw: RawTable =
            serde_json::from_str(json).map_err(|e| PriceTableError(TableProblem::Malformed(e.to_string())))?;
        let providers = raw
            .providers
            .into_iter()
            .map(|(key, raw)| {
                let prices = validate(raw).map_err(|problem| {
                    PriceTableError(TableProblem::InvalidProvider {
                        provider: key.clone(),
                        problem,
                    })
                })?;
                Ok((key, prices))
            })
            .collect::<Result<_, _>>()?;
        Ok(Self { providers })
    }

    /// The copy built into the app.
    pub fn bundled() -> Self {
        Self::parse(BUNDLED).expect("the bundled price table is valid (cost_test)")
    }

    /// The prices under `key`, when the table has them.
    pub(crate) fn provider(&self, key: &str) -> Option<&ProviderPrices> {
        self.providers.get(key)
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct VersionOnly {
    schema_version: u32,
}

/// The wire shape, before validation. Fields the estimator doesn't read
/// (`asOf`, `source`, `scope`, `notes`) are for the people keeping the table.
#[derive(Deserialize)]
struct RawTable {
    providers: HashMap<String, RawProvider>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawProvider {
    label: String,
    currency: String,
    request_classes: Vec<RawRequestClass>,
    egress_per_gb: f64,
    storage_per_gb_month: f64,
    minimum_storage_days: f64,
    minimum_billable_object_bytes: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawRequestClass {
    name: String,
    per_million: f64,
    operations: Vec<String>,
}

/// A provider's entry, checked: every operation priced exactly once (names this
/// build doesn't know are skipped), every number finite and not negative.
fn validate(raw: RawProvider) -> Result<ProviderPrices, ProviderProblem> {
    let number = |value: f64, field: &'static str| {
        if value.is_finite() && value >= 0.0 {
            Ok(value)
        } else {
            Err(ProviderProblem::BadNumber(field))
        }
    };
    let egress_per_gb = number(raw.egress_per_gb, "egressPerGb")?;
    let storage_per_gb_month = number(raw.storage_per_gb_month, "storagePerGbMonth")?;
    // Whole numbers on the wire; a fraction is truncated, which errs low.
    let minimum_storage_days = number(raw.minimum_storage_days, "minimumStorageDays")? as u32;
    let minimum_billable_object_bytes = number(raw.minimum_billable_object_bytes, "minimumBillableObjectBytes")? as u64;

    let mut request_classes = Vec::with_capacity(raw.request_classes.len());
    let mut class_of = HashMap::new();
    for (index, class) in raw.request_classes.into_iter().enumerate() {
        let per_million = number(class.per_million, "perMillion")?;
        for kind in class.operations.iter().filter_map(|name| RequestKind::from_name(name)) {
            if class_of.insert(kind, index).is_some() {
                return Err(ProviderProblem::PricedTwice(kind));
            }
        }
        request_classes.push(RequestClass {
            name: class.name,
            per_million,
        });
    }
    if let Some(kind) = RequestKind::ALL.into_iter().find(|kind| !class_of.contains_key(kind)) {
        return Err(ProviderProblem::Unpriced(kind));
    }
    Ok(ProviderPrices {
        label: raw.label,
        currency: raw.currency,
        request_classes,
        class_of,
        egress_per_gb,
        storage_per_gb_month,
        minimum_storage_days,
        minimum_billable_object_bytes,
    })
}
