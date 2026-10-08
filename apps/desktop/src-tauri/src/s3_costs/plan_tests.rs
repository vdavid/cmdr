//! The planner against hand-computed amounts at the bundled table's list prices
//! (`crates/cmdr-s3/src/cost/s3-prices.json`): AWS "PUT, COPY, POST, LIST"
//! $5.00/M, "GET and all other requests" $0.40/M, egress $0.09/GB; R2 Class A
//! $4.50/M, Class B $0.36/M; Wasabi storage $0.00780273/GB-month, 90 days.

use cmdr_s3::S3Provider;
use cmdr_s3::cost::{Estimate, PriceTable, Workload};

use super::plan::{ClashPlan, CostedOperation, KnownClash, Overwrite, Sides, overwritten, plan};
use crate::file_system::volume::ScannedFile;
use crate::file_system::write_operations::ScanCostFacts;

const MIB: u64 = 1024 * 1024;
const GIB: u64 = 1024 * MIB;
const DAY: u64 = 86_400;
const NOW: u64 = 1_790_000_000;

fn aws() -> Workload {
    Workload::for_provider_at(
        &S3Provider::Aws {
            region: "us-east-1".into(),
        },
        NOW,
    )
}
fn r2() -> Workload {
    Workload::for_provider_at(
        &S3Provider::R2 {
            account_id: "0123456789abcdef0123456789abcdef".into(),
        },
        NOW,
    )
}
fn wasabi() -> Workload {
    Workload::for_provider_at(
        &S3Provider::Wasabi {
            region: "eu-central-1".into(),
        },
        NOW,
    )
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

fn totals(workloads: &[Workload]) -> Vec<f64> {
    let table = PriceTable::bundled();
    workloads
        .iter()
        .map(|work| Estimate::of(&table, work).expect("a priced provider").total)
        .collect()
}

fn close(actual: f64, expected: f64) {
    assert!((actual - expected).abs() < 1e-12, "expected {expected}, got {actual}");
}

#[test]
fn nothing_on_s3_plans_nothing() {
    let sides = Sides {
        source: None,
        destination: None,
        server_copy: false,
    };
    assert!(plan(CostedOperation::Copy, sides, &facts(&[(MIB, None)], 0), &[]).is_empty());
}

#[test]
fn an_upload_to_aws_is_a_put_and_a_head_per_file_and_folder() {
    let sides = Sides {
        source: None,
        destination: Some(aws()),
        server_copy: false,
    };
    let files: Vec<_> = (0..1_000).map(|_| (MIB, None)).collect();
    let planned = plan(CostedOperation::Copy, sides, &facts(&files, 2), &[]);
    // 1,000 files into two folders this upload makes: 1,000 PUTs and their
    // verifying HEADs (no no-overwrite HEAD: the folders are fresh). The
    // folders: two marker PUTs, four HEADs, six LISTs. Readying the
    // destination: three LISTs; probing the selected folder's name: a HEAD and
    // a LIST. PUT-class: 1,002 PUTs + 10 LISTs; HEADs: 1,005.
    close(totals(&planned)[0], 1_012.0 * 5e-6 + 1_005.0 * 4e-7);
}

#[test]
fn a_download_from_aws_bills_its_bytes() {
    let sides = Sides {
        source: Some(aws()),
        destination: None,
        server_copy: false,
    };
    let planned = plan(CostedOperation::Copy, sides, &facts(&[(10 * GIB, None)], 0), &[]);
    // The selected file's stat (a HEAD), one GET, and 10 GB × $0.09.
    close(totals(&planned)[0], 0.9 + 2.0 * 4e-7);
}

#[test]
fn a_move_within_one_aws_account_copies_on_the_server_and_deletes_the_source() {
    let sides = Sides {
        source: Some(aws()),
        destination: Some(aws()),
        server_copy: true,
    };
    let planned = plan(CostedOperation::Move, sides, &facts(&[(MIB, None)], 1), &[]);
    // One workload: the account is billed once. LISTs: the scan's stat (1)
    // and its listing plus the walk's (2), readying the destination (3) and
    // checking the move (3), the name probe (1), making the folder (3), the
    // sweep's two and the emptied folder's capped one (3): 16. HEADs: the
    // stat, the probe, two making the folder, the copy's source (no
    // no-overwrite or verifying HEAD in a fresh folder), the sweep's: 6. Plus
    // one `CopyObject` and the marker's PUT; the deletes are free.
    assert_eq!(planned.len(), 1);
    close(totals(&planned)[0], 18.0 * 5e-6 + 6.0 * 4e-7);
}

#[test]
fn a_copy_between_two_accounts_downloads_from_one_and_uploads_to_the_other() {
    let sides = Sides {
        source: Some(r2()),
        destination: Some(aws()),
        server_copy: false,
    };
    let planned = plan(
        CostedOperation::Copy,
        sides,
        &facts(&[(MIB, None), (MIB, None)], 0),
        &[],
    );
    let amounts = totals(&planned);
    // R2: the two selected files' stats and their GETs at $0.36/M, egress
    // free. AWS: readying the destination (3 LISTs), a name probe per file (a
    // HEAD and a LIST), two PUTs and their verifying HEADs.
    close(amounts[0], 4.0 * 0.36e-6);
    close(amounts[1], 7.0 * 5e-6 + 4.0 * 4e-7);
}

#[test]
fn a_wasabi_delete_bills_the_young_objects_remaining_days() {
    let sides = Sides {
        source: Some(wasabi()),
        destination: None,
        server_copy: false,
    };
    let planned = plan(
        CostedOperation::Delete,
        sides,
        &facts(&[(GIB, Some(NOW - 30 * DAY)), (GIB, Some(NOW - 200 * DAY))], 1),
        &[],
    );
    // Only the 30-day-old object: 1 GB × 60 days × $0.00780273 / 30.
    close(totals(&planned)[0], 60.0 * 0.00780273 / 30.0);
}

#[test]
fn a_move_off_wasabi_bills_early_deletion_on_the_source() {
    let sides = Sides {
        source: Some(wasabi()),
        destination: None,
        server_copy: false,
    };
    let planned = plan(CostedOperation::Move, sides, &facts(&[(GIB, Some(NOW))], 0), &[]);
    close(totals(&planned)[0], 90.0 * 0.00780273 / 30.0);
}

#[test]
fn a_copy_off_wasabi_deletes_nothing_and_costs_nothing() {
    let sides = Sides {
        source: Some(wasabi()),
        destination: None,
        server_copy: false,
    };
    let planned = plan(CostedOperation::Copy, sides, &facts(&[(GIB, Some(NOW))], 0), &[]);
    close(totals(&planned)[0], 0.0);
}

#[test]
fn without_a_per_file_list_the_bytes_spread_evenly_over_the_files() {
    let sides = Sides {
        source: None,
        destination: Some(aws()),
        server_copy: false,
    };
    let bare = ScanCostFacts {
        files: 4,
        dirs: 0,
        bytes: 400 * MIB,
        per_file: None,
        selected_folders: 0,
        selected_file_sizes: vec![100 * MIB; 4],
    };
    let planned = plan(CostedOperation::Copy, sides, &bare, &[]);
    // Four 100 MiB files, each two 64 MiB-floor parts plus Create and Complete
    // (16), readying the destination and probing each name (7 LISTs): 23
    // PUT-class requests; four name probes and four verifying HEADs.
    close(totals(&planned)[0], 23.0 * 5e-6 + 8.0 * 4e-7);
}

fn clash(source_size: u64, dest_size: u64, source_modified: Option<u64>, dest_modified: Option<u64>) -> KnownClash {
    KnownClash {
        source_size,
        dest_size,
        source_modified,
        dest_modified,
    }
}

/// An upload onto Wasabi that overwrites a young object: the replaced object's
/// remaining days, and nothing more (Wasabi refuses a short body, so a one-PUT
/// overwrite goes straight to the key and writes nothing beside it to bill).
#[test]
fn an_upload_overwriting_on_wasabi_bills_only_the_replaced_object() {
    let sides = Sides {
        source: None,
        destination: Some(wasabi()),
        server_copy: false,
    };
    let overwrites = [Overwrite {
        incoming_size: GIB / 32,
        replaced: ScannedFile {
            size: GIB,
            modified_at: Some(NOW - 10 * DAY),
        },
    }];
    let planned = plan(
        CostedOperation::Copy,
        sides,
        &facts(&[(GIB / 32, None)], 0),
        &overwrites,
    );
    close(totals(&planned)[0], 80.0 * 0.00780273 / 30.0);
}

/// A server-side copy onto an existing key replaces it in one request: only
/// the replaced object's remaining days.
#[test]
fn a_server_copy_overwriting_on_wasabi_bills_only_the_replaced_object() {
    let sides = Sides {
        source: Some(wasabi()),
        destination: Some(wasabi()),
        server_copy: true,
    };
    let overwrites = [Overwrite {
        incoming_size: MIB,
        replaced: ScannedFile {
            size: GIB,
            modified_at: Some(NOW - 30 * DAY),
        },
    }];
    let planned = plan(CostedOperation::Copy, sides, &facts(&[(MIB, None)], 0), &overwrites);
    close(totals(&planned)[0], 60.0 * 0.00780273 / 30.0);
}

#[test]
fn the_policy_decides_which_known_clashes_are_overwritten() {
    use crate::file_system::write_operations::ConflictResolution;
    let clashes = vec![
        clash(10, 5, Some(200), Some(100)),
        clash(10, 20, Some(100), Some(200)),
        clash(10, 10, None, Some(100)),
    ];
    let sizes = |resolution| -> Vec<u64> {
        overwritten(&ClashPlan {
            resolution,
            clashes: clashes.clone(),
        })
        .iter()
        .map(|overwrite| overwrite.replaced.size)
        .collect()
    };
    assert_eq!(sizes(ConflictResolution::Overwrite), [5, 20, 10]);
    assert_eq!(
        sizes(ConflictResolution::OverwriteSmaller),
        [5],
        "strictly smaller only"
    );
    assert_eq!(
        sizes(ConflictResolution::OverwriteOlder),
        [5],
        "strictly older, both dates known"
    );
    assert!(sizes(ConflictResolution::Skip).is_empty());
    assert!(sizes(ConflictResolution::Rename).is_empty());
    assert!(
        sizes(ConflictResolution::Stop).is_empty(),
        "each clash is asked about, so none is assumed"
    );
}
