//! A [`Workload`] priced against a [`PriceTable`].

use super::prices::{PriceTable, RequestKind};
use super::workload::Workload;

/// What a workload costs at one provider's list prices.
#[derive(Debug, Clone, PartialEq)]
pub struct Estimate {
    /// The provider as its prices page names it (`AWS`, `Cloudflare R2`).
    pub provider_label: String,
    /// ISO 4217 (`USD`, `EUR`).
    pub currency: String,
    /// The sum of `line_items`, unrounded.
    pub total: f64,
    /// What the total is made of, in table order: request classes, then
    /// downloads, then early deletions. A class with no requests is left out.
    pub line_items: Vec<LineItem>,
}

/// One part of an [`Estimate`].
#[derive(Debug, Clone, PartialEq)]
pub enum LineItem {
    /// Requests in one of the provider's request classes.
    Requests {
        /// The class as the provider names it (`Class A`).
        class: String,
        /// How many requests fall in it.
        count: u64,
        /// What they cost.
        amount: f64,
    },
    /// Bytes downloaded to the Mac.
    Egress {
        /// How many.
        bytes: u64,
        /// What they cost.
        amount: f64,
    },
    /// Objects deleted before the provider's minimum storage duration, billed
    /// for the days they had left.
    EarlyDeletion {
        /// How many objects.
        objects: u64,
        /// Their billable size times their remaining days, in GiB-days.
        gb_days: f64,
        /// What that costs.
        amount: f64,
    },
}

impl Estimate {
    /// What `workload` costs at its provider's list prices in `table`, or `None`
    /// when the table has no prices for that provider.
    pub fn of(table: &PriceTable, workload: &Workload) -> Option<Self> {
        let prices = table.provider(workload.price_key?)?;
        let mut line_items = Vec::new();

        let mut counts = vec![0_u64; prices.request_classes.len()];
        let batches = workload.deleted_objects.div_ceil(DELETE_BATCH);
        let sent = workload
            .requests
            .iter()
            .map(|(kind, count)| (*kind, *count))
            .chain(std::iter::once((RequestKind::DeleteObjects, batches)));
        for (kind, count) in sent {
            // Validation guarantees every kind a class.
            if let Some(&class) = prices.class_of.get(&kind) {
                counts[class] += count;
            }
        }
        for (class, count) in prices.request_classes.iter().zip(counts) {
            if count > 0 {
                line_items.push(LineItem::Requests {
                    class: class.name.clone(),
                    count,
                    amount: count as f64 * class.per_million / 1_000_000.0,
                });
            }
        }

        if workload.egress_bytes > 0 {
            line_items.push(LineItem::Egress {
                bytes: workload.egress_bytes,
                amount: workload.egress_bytes as f64 / GIB * prices.egress_per_gb,
            });
        }

        let minimum_days = u64::from(prices.minimum_storage_days);
        let (objects, gb_days) = workload
            .dated_deletions
            .iter()
            .filter(|(_, age_days)| *age_days < minimum_days)
            .fold((0_u64, 0.0_f64), |(objects, gb_days), (size, age_days)| {
                let billed = (*size).max(prices.minimum_billable_object_bytes) as f64 / GIB;
                (objects + 1, gb_days + billed * (minimum_days - age_days) as f64)
            });
        if objects > 0 {
            line_items.push(LineItem::EarlyDeletion {
                objects,
                gb_days,
                amount: gb_days * prices.storage_per_gb_month / DAYS_PER_MONTH,
            });
        }

        let total = line_items.iter().map(LineItem::amount).sum();
        Some(Self {
            provider_label: prices.label.clone(),
            currency: prices.currency.clone(),
            total,
            line_items,
        })
    }
}

impl LineItem {
    /// What this part costs.
    pub fn amount(&self) -> f64 {
        match self {
            Self::Requests { amount, .. } | Self::Egress { amount, .. } | Self::EarlyDeletion { amount, .. } => *amount,
        }
    }
}

/// Keys per `DeleteObjects`, S3's cap and what `volume/batch.rs` sends.
pub(super) const DELETE_BATCH: u64 = 1_000;

/// Providers bill by the binary gigabyte (AWS's "GB" is 2^30 bytes, and
/// Wasabi's FAQ divides a TB by 1,024).
const GIB: f64 = 1024.0 * 1024.0 * 1024.0;

/// Wasabi's month for per-day storage, from its FAQ: "/ (30 days in a month)".
const DAYS_PER_MONTH: f64 = 30.0;
