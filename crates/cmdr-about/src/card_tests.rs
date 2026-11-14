use super::*;
use crate::fact::Prominence;
use crate::test_support::{card, identity, land, local, macos, mounted_share, phone, text};

use AboutTarget::{AppBundle, ArchiveEntry, File, Folder, Package, Symlink};
use FactKind as K;

fn plan_of(target: AboutTarget, extension: Option<&str>, caps: &VolumeCaps) -> AboutPlan {
    plan(PlanInput {
        target,
        extension,
        is_executable: false,
        contents_local: true,
        caps,
        os: &macos(),
        owner: &OwnerQuestion::Receipt,
        in_a_repo: true,
        git_snapshot: false,
    })
}

fn sections(plan: &AboutPlan) -> Vec<(Section, Vec<FactKind>)> {
    plan.sections
        .iter()
        .map(|planned| (planned.section, planned.kinds.clone()))
        .collect()
}

// ── The plan, per target on a local disk ────────────────────────────────────

#[test]
fn a_file_leads_with_where_it_came_from() {
    assert_eq!(
        sections(&plan_of(File, Some("pdf"), &local())),
        [
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Nature, vec![K::Identity, K::RealType]),
            (Section::Use, vec![K::LastOpened, K::DefaultApp]),
            (Section::Context, vec![K::Owner, K::Git]),
            (Section::Removal, vec![]),
        ]
    );
}

#[test]
fn a_folder_leads_with_whats_inside() {
    assert_eq!(
        sections(&plan_of(Folder, None, &local())),
        [
            (Section::Contents, vec![K::FolderDigest]),
            (Section::Nature, vec![K::Identity, K::RecursiveSize]),
            (Section::Context, vec![K::Owner, K::Git]),
            (Section::Use, vec![K::Importance]),
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Removal, vec![]),
        ]
    );
}

#[test]
fn an_app_leads_with_what_it_is() {
    assert_eq!(
        sections(&plan_of(AppBundle, Some("app"), &local())),
        [
            (
                Section::Nature,
                vec![K::Identity, K::Bundle, K::Signature, K::RecursiveSize]
            ),
            (Section::Context, vec![K::Owner, K::Git, K::AppData]),
            (Section::Use, vec![K::LastOpened]),
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Removal, vec![]),
        ]
    );
}

#[test]
fn a_package_is_a_bundle_that_opens_in_an_app() {
    assert_eq!(
        sections(&plan_of(Package, Some("rtfd"), &local())),
        [
            (
                Section::Nature,
                vec![K::Identity, K::Bundle, K::Signature, K::RecursiveSize]
            ),
            (Section::Context, vec![K::Owner, K::Git]),
            (Section::Use, vec![K::LastOpened, K::DefaultApp]),
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Removal, vec![]),
        ]
    );
}

#[test]
fn a_symlink_has_no_download_record_or_content_of_its_own() {
    assert_eq!(
        sections(&plan_of(Symlink, None, &local())),
        [
            (Section::Origin, vec![K::CmdrOperation]),
            (Section::Nature, vec![K::Identity]),
            (Section::Context, vec![K::Owner, K::Git]),
            (Section::Removal, vec![]),
        ]
    );
}

#[test]
fn a_picture_says_what_the_image_index_recognized_right_after_what_it_is() {
    assert_eq!(
        sections(&plan_of(File, Some("JPG"), &local()))[1],
        (Section::Nature, vec![K::Identity, K::RealType, K::ImageContents])
    );
    // A document, a folder named like a picture, and a link to one have no such row.
    assert!(!plan_of(File, Some("pdf"), &local()).has(K::ImageContents));
    assert!(!plan_of(File, None, &local()).has(K::ImageContents));
    assert!(!plan_of(Folder, Some("png"), &local()).has(K::ImageContents));
    assert!(!plan_of(Symlink, Some("png"), &local()).has(K::ImageContents));
}

/// The index's own database answers it: a picture on a share has the row, and an
/// online-only one too, since nothing reads its bytes.
#[test]
fn the_image_row_reads_the_index_so_a_share_and_an_evicted_picture_have_it() {
    assert!(plan_of(File, Some("heic"), &mounted_share()).has(K::ImageContents));
    let evicted = plan(PlanInput {
        target: File,
        extension: Some("png"),
        is_executable: false,
        contents_local: false,
        caps: &local(),
        os: &macos(),
        owner: &OwnerQuestion::Nothing,
        in_a_repo: false,
        git_snapshot: false,
    });
    assert!(evicted.has(K::ImageContents));
    assert!(!evicted.has(K::RealType));
    let no_images = VolumeCaps {
        image_index: false,
        ..mounted_share()
    };
    assert!(!plan_of(File, Some("heic"), &no_images).has(K::ImageContents));
    assert!(!plan_of(File, Some("heic"), &phone()).has(K::ImageContents));
}

// ── The plan, per volume ────────────────────────────────────────────────────

#[test]
fn a_local_disk_needs_no_note() {
    for target in [File, Folder, AppBundle, Package, Symlink] {
        assert_eq!(plan_of(target, None, &local()).volume_note, None);
    }
}

#[test]
fn a_mounted_share_answers_what_reads_and_cmdrs_own_stores_can() {
    let share = mounted_share();
    assert_eq!(
        sections(&plan_of(File, Some("pdf"), &share)),
        [
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Nature, vec![K::Identity, K::RealType]),
            (Section::Removal, vec![]),
        ]
    );
    assert_eq!(
        sections(&plan_of(Folder, None, &share)),
        [
            (Section::Contents, vec![K::FolderDigest]),
            (Section::Nature, vec![K::Identity, K::RecursiveSize]),
            (Section::Use, vec![K::Importance]),
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Removal, vec![]),
        ]
    );
    // No framework is asked about an app on a share: no bundle, signature, or app data.
    assert_eq!(
        sections(&plan_of(AppBundle, Some("app"), &share)),
        [
            (Section::Nature, vec![K::Identity, K::RecursiveSize]),
            (Section::Origin, vec![K::Download, K::CmdrOperation]),
            (Section::Removal, vec![]),
        ]
    );
    assert_eq!(
        plan_of(File, None, &share).volume_note,
        Some(VolumeNote {
            volume: text("Naspolya"),
            limit: VolumeLimit::SomeFacts
        })
    );
}

#[test]
fn a_phone_answers_identity_and_says_so_once() {
    let phone = phone();
    for target in [File, Folder, AppBundle, Package, Symlink] {
        let plan = plan_of(target, Some("dmg"), &phone);
        assert_eq!(
            sections(&plan),
            [(Section::Nature, vec![K::Identity]), (Section::Removal, vec![])],
            "{target:?}"
        );
        assert_eq!(
            plan.volume_note,
            Some(VolumeNote {
                volume: text("Pixel 9"),
                limit: VolumeLimit::IdentityOnly
            })
        );
    }
}

#[test]
fn an_archive_entry_answers_identity_and_the_archives_own_download_record() {
    let archive = VolumeCaps {
        download_record: true,
        ..VolumeCaps::identity_only(text("Photos.zip"))
    };
    assert_eq!(
        sections(&plan_of(ArchiveEntry, Some("jpg"), &archive)),
        [
            (Section::Origin, vec![K::Download]),
            (Section::Nature, vec![K::Identity]),
            (Section::Removal, vec![]),
        ]
    );
    // Even on a volume that answers everything, an entry has no owner, repo, or
    // operation of its own.
    assert_eq!(
        sections(&plan_of(ArchiveEntry, Some("jpg"), &local())),
        [
            (Section::Origin, vec![K::Download]),
            (Section::Nature, vec![K::Identity]),
            (Section::Removal, vec![]),
        ]
    );
    // The download row says which archive it's in, so no note says what "the
    // volume" answers.
    assert_eq!(plan_of(ArchiveEntry, Some("jpg"), &archive).volume_note, None);
    assert_eq!(plan_of(ArchiveEntry, Some("jpg"), &phone()).volume_note, None);
}

fn snapshot_plan(target: AboutTarget, caps: &VolumeCaps) -> AboutPlan {
    plan(PlanInput {
        target,
        extension: Some("rs"),
        is_executable: false,
        contents_local: false,
        caps,
        os: &macos(),
        owner: &OwnerQuestion::Receipt,
        in_a_repo: true,
        git_snapshot: true,
    })
}

/// A file in `.git/commits/<id>/` is git's record of it, not a file on the disk:
/// its snapshot row leads, with nothing the disk would answer and no note.
#[test]
fn a_git_snapshot_entry_leads_with_its_snapshot_and_nothing_of_the_disks() {
    let snapshot = VolumeCaps::identity_only(text("cmdr"));
    for target in [File, Folder] {
        let plan = snapshot_plan(target, &snapshot);
        let expected = if target == Folder {
            vec![
                (Section::Nature, vec![K::Identity]),
                (Section::Origin, vec![K::GitSnapshot]),
                (Section::Removal, vec![]),
            ]
        } else {
            vec![
                (Section::Origin, vec![K::GitSnapshot]),
                (Section::Nature, vec![K::Identity]),
                (Section::Removal, vec![]),
            ]
        };
        assert_eq!(sections(&plan), expected, "{target:?}");
        assert_eq!(plan.volume_note, None);
    }
    // Whatever the host claims the volume answers, no row of the disk's appears.
    let plan = snapshot_plan(File, &local());
    for kind in [K::Download, K::CmdrOperation, K::Owner, K::Git] {
        assert!(!plan.has(kind), "{kind:?}");
    }
    assert!(!plan_of(File, Some("rs"), &local()).has(K::GitSnapshot));
}

// ── The signature row ───────────────────────────────────────────────────────

#[test]
fn a_signature_is_planned_only_for_what_can_carry_one() {
    let signed = |target, extension, is_executable| {
        plan(PlanInput {
            target,
            extension,
            is_executable,
            contents_local: true,
            caps: &local(),
            os: &macos(),
            owner: &OwnerQuestion::Receipt,
            in_a_repo: true,
            git_snapshot: false,
        })
        .has(K::Signature)
    };
    assert!(signed(AppBundle, Some("app"), false));
    assert!(signed(Package, Some("framework"), false));
    assert!(signed(File, Some("DMG"), false));
    assert!(
        !signed(File, Some("pkg"), false),
        "an installer package's signature is one the code-signing API can't read"
    );
    assert!(signed(File, None, true));
    // A program hiding behind a picture's name shows through the type row's
    // mismatch; with its executable bit set it's planned like any program.
    assert!(signed(File, Some("jpg"), true));
    assert!(!signed(File, Some("txt"), false));
    assert!(!signed(File, None, false));
    assert!(!signed(Folder, None, false));
    assert!(!signed(Folder, None, true), "a folder's search bit is no program");
    assert!(!signed(Symlink, None, true));
}

// ── What the OS and the bytes allow ─────────────────────────────────────────

/// Linux keeps no Spotlight, no code signatures, and (until its default-app
/// lookup lands) no default app: those rows are left out, and the disk's note
/// stays quiet, since the disk answers everything a disk can.
#[test]
fn an_os_without_use_history_or_signing_plans_neither_row_and_adds_no_note() {
    let linux = OsCaps {
        download_records: true,
        use_history: false,
        code_signing: false,
        default_app: false,
        install_receipts: false,
    };
    let plan = plan(PlanInput {
        target: File,
        extension: Some("dmg"),
        is_executable: true,
        contents_local: true,
        caps: &local(),
        os: &linux,
        owner: &OwnerQuestion::Receipt,
        in_a_repo: true,
        git_snapshot: false,
    });
    assert!(!plan.has(K::LastOpened));
    assert!(!plan.has(K::Signature));
    assert!(!plan.has(K::DefaultApp));
    assert!(plan.has(K::RealType));
    assert_eq!(plan.volume_note, None);
}

/// Reading an online-only file's bytes downloads it, so the rows that read them
/// aren't planned.
#[test]
fn an_online_only_file_plans_no_row_that_reads_its_bytes() {
    let plan = plan(PlanInput {
        target: File,
        extension: Some("dmg"),
        is_executable: false,
        contents_local: false,
        caps: &local(),
        os: &macos(),
        owner: &OwnerQuestion::Receipt,
        in_a_repo: true,
        git_snapshot: false,
    });
    assert!(!plan.has(K::RealType));
    assert!(!plan.has(K::Signature));
    assert!(plan.has(K::Download), "an xattr is metadata, kept for an evicted file");
}

/// ❗ An online-only app or package (iCloud Drive) plans no row that opens its
/// `Info.plist`: the bundle row and an app's data row would download it.
#[test]
fn an_online_only_bundle_plans_no_row_that_opens_its_description() {
    for target in [AppBundle, Package] {
        let plan = plan(PlanInput {
            target,
            extension: Some("app"),
            is_executable: false,
            contents_local: false,
            caps: &local(),
            os: &macos(),
            owner: &OwnerQuestion::Nothing,
            in_a_repo: false,
            git_snapshot: false,
        });
        assert!(!plan.has(K::Bundle), "{target:?}");
        assert!(!plan.has(K::AppData), "{target:?}");
        assert!(!plan.has(K::Signature), "{target:?}");
    }
}

/// An OS whose download records Cmdr doesn't read (Windows) plans no Download row: a
/// row it can only answer "no record" would claim a record isn't there.
#[test]
fn an_os_without_download_records_plans_no_download_row() {
    let windows = OsCaps {
        download_records: false,
        ..macos()
    };
    let plan = plan(PlanInput {
        target: File,
        extension: Some("dmg"),
        is_executable: false,
        contents_local: true,
        caps: &local(),
        os: &windows,
        owner: &OwnerQuestion::Nothing,
        in_a_repo: false,
        git_snapshot: false,
    });
    assert!(!plan.has(K::Download));
    assert!(plan.has(K::RealType));
}

#[test]
fn keeping_only_the_answered_rows_drops_emptied_sections_and_keeps_removal() {
    let mut plan = plan_of(File, Some("pdf"), &local());
    plan.retain_kinds(|kind| matches!(kind, K::Identity | K::LastOpened));
    assert_eq!(
        sections(&plan),
        [
            (Section::Nature, vec![K::Identity]),
            (Section::Use, vec![K::LastOpened]),
            (Section::Removal, vec![]),
        ]
    );
}

// ── The owner and git rows ──────────────────────────────────────────────────

fn context_plan(owner: &OwnerQuestion, in_a_repo: bool, os: &OsCaps) -> AboutPlan {
    plan(PlanInput {
        target: File,
        extension: Some("txt"),
        is_executable: false,
        contents_local: true,
        caps: &local(),
        os,
        owner,
        in_a_repo,
        git_snapshot: false,
    })
}

/// A plain document outside every repo has nothing to say about an owner or git,
/// so neither row is drawn: the card stays short.
#[test]
fn a_document_no_path_rule_claims_and_no_repo_holds_has_no_context_rows() {
    let plan = context_plan(&OwnerQuestion::Nothing, false, &macos());
    assert!(!plan.has(K::Owner));
    assert!(!plan.has(K::Git));
    assert!(plan.sections.iter().all(|planned| planned.section != Section::Context));
}

#[test]
fn a_path_that_names_an_owner_or_sits_in_a_repo_plans_its_row() {
    let in_app_data = OwnerQuestion::BundleId {
        id: "com.example.tool".to_string(),
        place: crate::owner::OwnerPlace::Containers,
    };
    assert!(context_plan(&in_app_data, false, &macos()).has(K::Owner));
    assert!(context_plan(&OwnerQuestion::Nothing, true, &macos()).has(K::Git));
}

#[test]
fn a_receipt_question_plans_an_owner_row_only_where_the_os_keeps_receipts() {
    assert!(context_plan(&OwnerQuestion::Receipt, false, &macos()).has(K::Owner));
    let linux = OsCaps {
        install_receipts: false,
        ..macos()
    };
    assert!(!context_plan(&OwnerQuestion::Receipt, false, &linux).has(K::Owner));
}

// ── Plan invariants ─────────────────────────────────────────────────────────

fn every_plan() -> Vec<AboutPlan> {
    let mut plans = Vec::new();
    for caps in [local(), mounted_share(), phone()] {
        for target in [File, Folder, AppBundle, Package, Symlink, ArchiveEntry] {
            for is_executable in [false, true] {
                plans.push(plan(PlanInput {
                    target,
                    extension: Some("dmg"),
                    is_executable,
                    contents_local: true,
                    caps: &caps,
                    os: &macos(),
                    owner: &OwnerQuestion::Receipt,
                    in_a_repo: true,
                    git_snapshot: false,
                }));
            }
        }
    }
    plans
}

#[test]
fn every_plan_ends_with_removal_and_starts_with_rows() {
    for plan in every_plan() {
        let last = plan.sections.last().expect("a plan has sections");
        assert_eq!(last.section, Section::Removal, "{:?}", plan.target);
        assert!(last.kinds.is_empty());
        assert!(
            plan.sections[..plan.sections.len() - 1]
                .iter()
                .all(|section| !section.kinds.is_empty()),
            "a section with no row is left out"
        );
        assert!(plan.has(K::Identity), "every card says what the item is");
    }
}

#[test]
fn every_kind_sits_in_exactly_one_section() {
    let all_sections = [
        Section::Origin,
        Section::Nature,
        Section::Use,
        Section::Context,
        Section::Contents,
        Section::Removal,
    ];
    let mut placed: Vec<FactKind> = all_sections
        .into_iter()
        .flat_map(|section| rows_of(section).to_vec())
        .collect();
    placed.sort();
    assert_eq!(placed, FactKind::ALL);
}

#[test]
fn no_plan_lists_a_row_twice() {
    for plan in every_plan() {
        let mut kinds = plan.kinds();
        let listed = kinds.len();
        kinds.sort();
        kinds.dedup();
        assert_eq!(kinds.len(), listed);
    }
}

// ── The card ────────────────────────────────────────────────────────────────

#[test]
fn a_new_card_has_every_planned_row_pending_and_no_other() {
    let card = card(File, Some("pdf"), &local());
    assert_eq!(
        card.pending(),
        [
            K::Identity,
            K::RealType,
            K::Download,
            K::CmdrOperation,
            K::LastOpened,
            K::DefaultApp,
            K::Owner,
            K::Git
        ]
    );
    assert_eq!(card.bundle, None);
    assert_eq!(card.signature, None);
    assert_eq!(card.folder_digest, None);
}

#[test]
fn an_answer_lands_in_its_row_and_one_outside_the_plan_is_refused() {
    let mut card = card(File, Some("pdf"), &local());
    land(&mut card, identity("Report.pdf", 2_100_000));
    assert!(card.identity.as_ref().is_some_and(|fact| fact.known().is_some()));
    assert!(!card.pending().contains(&K::Identity));

    let digest = AboutFact::FolderDigest(Fact::unknown(UnknownWhy::NotIndexed));
    assert_eq!(card.apply(digest), Applied::NotPlanned);
    assert_eq!(card.folder_digest, None);
}

#[test]
fn settling_gives_every_waiting_row_a_typed_unknown_and_leaves_the_rest() {
    let mut card = card(File, Some("pdf"), &local());
    land(&mut card, identity("Report.pdf", 2_100_000));
    let settled = card.settle_pending(UnknownWhy::Unreachable);
    assert_eq!(
        settled.iter().map(AboutFact::kind).collect::<Vec<_>>(),
        [
            K::RealType,
            K::Download,
            K::CmdrOperation,
            K::LastOpened,
            K::DefaultApp,
            K::Owner,
            K::Git
        ]
    );
    assert!(card.pending().is_empty());
    assert!(card.identity.as_ref().is_some_and(|fact| fact.known().is_some()));
    let git = card.git.as_ref().expect("git is planned");
    assert_eq!(git.unknown_why(), Some(UnknownWhy::Unreachable));
    assert_eq!(card.settle_pending(UnknownWhy::Unreachable), []);
}

#[test]
fn a_fact_and_its_unknown_agree_on_their_kind() {
    for kind in FactKind::ALL {
        let fact = AboutFact::unknown(kind, UnknownWhy::TimedOut);
        assert_eq!(fact.kind(), kind);
    }
}

#[test]
fn a_fact_crosses_the_wire_under_its_kind() {
    let fact = AboutFact::unknown(K::LastOpened, UnknownWhy::NoRecord);
    assert_eq!(
        serde_json::to_value(&fact).expect("a fact serializes"),
        serde_json::json!({
            "kind": "lastOpened",
            "fact": { "state": "unknown", "value": { "why": "noRecord", "prominence": "routine" } },
        })
    );
    let unknown = Fact::<u8>::unknown(UnknownWhy::NoRecord);
    assert!(matches!(unknown, Fact::Unknown(unknown) if unknown.prominence() == Prominence::Routine));
}

#[test]
fn a_plan_crosses_the_wire_in_camel_case() {
    let plan = plan_of(File, None, &phone());
    assert_eq!(
        serde_json::to_value(&plan).expect("a plan serializes"),
        serde_json::json!({
            "target": "file",
            "sections": [
                { "section": "nature", "kinds": ["identity"] },
                { "section": "removal", "kinds": [] },
            ],
            "volumeNote": { "volume": "Pixel 9", "limit": "identityOnly" },
        })
    );
}
