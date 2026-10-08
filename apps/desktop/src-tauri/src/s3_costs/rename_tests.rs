//! F2's rename by move, priced from the editor's bounded tally: whether it's
//! free enough to start without the Move dialog.

use cmdr_s3::S3Provider;
use cmdr_s3::cost::{PriceTable, Workload};

use super::{CostEstimate, priced_rename, rounds_to_zero};
use crate::file_system::volume::ScannedFile;
use crate::file_system::write_operations::ScanCostFacts;

const MIB: u64 = 1024 * 1024;
const DAY: u64 = 86_400;
const NOW: u64 = 1_790_000_000;

fn at(provider: S3Provider) -> Workload {
    Workload::for_provider_at(&provider, NOW)
}

fn facts(files: &[(u64, Option<u64>)], dirs: usize) -> ScanCostFacts {
    ScanCostFacts {
        files: files.len(),
        dirs,
        bytes: files.iter().map(|(size, _)| size).sum(),
        per_file: Some(
            files
                .iter()
                .map(|&(size, modified_at)| ScannedFile { size, modified_at })
                .collect(),
        ),
        // A selection of one folder holding the files, or of the files
        // themselves when there's no folder.
        selected_folders: usize::from(dirs > 0),
        selected_file_sizes: if dirs > 0 {
            Vec::new()
        } else {
            files.iter().map(|(size, _)| *size).collect()
        },
    }
}

fn usd(amount: f64) -> CostEstimate {
    CostEstimate {
        amount,
        currency: "USD".into(),
        provider_label: "AWS".into(),
    }
}

#[test]
fn a_small_rename_on_aws_rounds_to_nothing() {
    let aws = at(S3Provider::Aws {
        region: "us-east-1".into(),
    });
    let old = Some(NOW - 400 * DAY);
    let estimates = priced_rename(
        aws,
        &facts(&[(MIB, old), (MIB, old), (MIB, old)], 1),
        &PriceTable::bundled(),
    );
    assert!(rounds_to_zero(&estimates), "three copies and a delete: {estimates:?}");
}

/// Wasabi bills a deleted object's remaining days of its 90, so renaming a
/// young one costs money however few files it carries.
#[test]
fn a_rename_of_young_wasabi_objects_is_not_free() {
    let wasabi = at(S3Provider::Wasabi {
        region: "eu-central-1".into(),
    });
    let young = Some(NOW - DAY);
    let estimates = priced_rename(wasabi, &facts(&[(5 * 1024 * MIB, young)], 1), &PriceTable::bundled());
    assert!(!rounds_to_zero(&estimates), "five young gigabytes: {estimates:?}");
}

#[test]
fn an_estimate_rounds_to_zero_under_half_a_cent() {
    assert!(rounds_to_zero(&[]));
    assert!(rounds_to_zero(&[usd(0.004_99)]));
    assert!(!rounds_to_zero(&[usd(0.005)]));
    assert!(!rounds_to_zero(&[usd(0.0), usd(0.02)]), "any provider's share counts");
}
