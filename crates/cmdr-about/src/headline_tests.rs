use super::*;
use crate::card::AboutFact;
use crate::fact::UnknownWhy;
use crate::owner::OwnerPlace;
use crate::payload::Bundle;
use crate::provenance::{CmdrOperation, Initiator, download_fact};
use crate::test_support::{
    card, chrome_download, identity, land, land_no_records, last_opened, local, owned_by_id, phone, sniffed, text,
    total,
};

use AboutTarget::{AppBundle, ArchiveEntry, File, Folder, Package, Symlink};

const ZIP: &[u8] = b"PK\x03\x04\x14\0\0\0\x08\0";
const MACH_O: &[u8] = b"\xcf\xfa\xed\xfe\x0c\0\0\x01";

fn exact(bytes: u64) -> Option<HeadlineSize> {
    Some(HeadlineSize { bytes, at_least: false })
}

/// The calm case the whole card is judged by: a document with no provenance
/// still gets a complete sentence, from identity and use alone.
#[test]
fn a_plain_document_reads_as_kind_size_and_last_opened() {
    let mut document = card(File, Some("pages"), &local());
    land(&mut document, identity("Plan.pages", 2_100_000));
    land(&mut document, sniffed(ZIP, Some("pages")));
    land(&mut document, last_opened(1_759_400_000));
    land_no_records(&mut document);
    assert_eq!(
        headline(&document),
        Headline {
            what: HeadlineWhat::File {
                kind: Some(SniffedKind::Document),
                extension: Some(text("pages")),
            },
            size: exact(2_100_000),
            lead: HeadlineLead::LastOpened { at: 1_759_400_000 },
        }
    );
}

#[test]
fn a_document_nothing_recorded_anything_about_is_still_a_sentence() {
    let mut document = card(File, Some("txt"), &local());
    land(&mut document, identity("Notes.txt", 900));
    land(&mut document, sniffed(b"hello", Some("txt")));
    land_no_records(&mut document);
    assert_eq!(
        headline(&document),
        Headline {
            what: HeadlineWhat::File {
                kind: Some(SniffedKind::Text),
                extension: Some(text("txt")),
            },
            size: exact(900),
            lead: HeadlineLead::Plain,
        }
    );
}

#[test]
fn a_downloaded_file_leads_with_the_site_the_app_and_the_time() {
    let mut image = card(File, Some("dmg"), &local());
    land(&mut image, identity("Tool.dmg", 412_000_000));
    land(
        &mut image,
        chrome_download("https://downloads.example.com/v2/tok3n/Tool.dmg?sig=abc"),
    );
    land(&mut image, last_opened(1_759_400_000));
    assert_eq!(
        headline(&image).lead,
        HeadlineLead::DownloadedFrom {
            host: "downloads.example.com".to_owned(),
            by: Some(text("Chrome")),
            at: Some(0x6af6_442f),
        }
    );
}

#[test]
fn the_headline_grows_as_facts_land() {
    let mut file = card(File, Some("dmg"), &local());
    assert_eq!(
        headline(&file),
        Headline {
            what: HeadlineWhat::File {
                kind: None,
                extension: None
            },
            size: None,
            lead: HeadlineLead::Plain,
        }
    );
    land(&mut file, identity("Tool.dmg", 412_000_000));
    assert_eq!(headline(&file).size, exact(412_000_000));
    land(&mut file, last_opened(5));
    assert_eq!(headline(&file).lead, HeadlineLead::LastOpened { at: 5 });
    land(&mut file, chrome_download("https://example.com/Tool.dmg"));
    assert!(matches!(headline(&file).lead, HeadlineLead::DownloadedFrom { .. }));
}

#[test]
fn a_program_named_like_a_picture_is_headlined_as_a_program() {
    let mut liar = card(File, Some("jpg"), &local());
    land(&mut liar, sniffed(MACH_O, Some("jpg")));
    assert_eq!(
        headline(&liar).what,
        HeadlineWhat::File {
            kind: Some(SniffedKind::MachO),
            extension: Some(text("jpg")),
        }
    );
}

#[test]
fn a_cmdr_copy_leads_when_no_site_is_recorded() {
    let mut copied = card(File, Some("mkv"), &local());
    land(
        &mut copied,
        AboutFact::CmdrOperation(Fact::Known(CmdrOperation {
            kind: LandedBy::Copy,
            at: 1_760_000_000,
            initiator: Initiator::You,
            source_path: Some(text("/films/Trip.mkv")),
            source_volume: Some(text("Naspolya")),
        })),
    );
    land(&mut copied, last_opened(1_761_000_000));
    assert_eq!(
        headline(&copied).lead,
        HeadlineLead::PutHereByCmdr {
            kind: LandedBy::Copy,
            at: 1_760_000_000,
            source_volume: Some(text("Naspolya")),
        }
    );
}

#[test]
fn mail_and_airdrop_lead_without_naming_anyone() {
    let record = |entries: &[&str]| {
        let mut bytes = Vec::new();
        plist::Value::Array(
            entries
                .iter()
                .map(|entry| plist::Value::String((*entry).to_owned()))
                .collect(),
        )
        .to_writer_binary(&mut bytes)
        .expect("a plist value writes to memory");
        bytes
    };
    let mut attachment = card(File, Some("pdf"), &local());
    let mail = record(&[
        "Jane Example <jane@example.com>",
        "Re: the contract",
        "message:%3Cid@example.com%3E",
    ]);
    land(
        &mut attachment,
        AboutFact::Download(download_fact(Some(&mail), Some(b"0083;6af6442f;Mail;"))),
    );
    assert_eq!(
        headline(&attachment).lead,
        HeadlineLead::ReceivedByMail { at: Some(0x6af6_442f) }
    );

    let mut dropped = card(File, Some("heic"), &local());
    let airdrop = record(&["Jane's iPhone"]);
    land(
        &mut dropped,
        AboutFact::Download(download_fact(Some(&airdrop), Some(b"0081;6af6442f;sharingd;"))),
    );
    assert_eq!(
        headline(&dropped).lead,
        HeadlineLead::ReceivedByAirDrop { at: Some(0x6af6_442f) }
    );
}

#[test]
fn a_quarantine_alone_leads_with_the_app() {
    let mut file = card(File, Some("zip"), &local());
    land(
        &mut file,
        AboutFact::Download(download_fact(None, Some(b"0081;6af6442f;Firefox;"))),
    );
    assert_eq!(
        headline(&file).lead,
        HeadlineLead::DownloadedBy {
            app: text("Firefox"),
            at: Some(0x6af6_442f)
        }
    );
}

#[test]
fn a_folder_takes_its_size_from_the_index_with_its_coverage() {
    let mut folder = card(Folder, None, &local());
    land(&mut folder, identity("Projects", 192));
    assert_eq!(headline(&folder).size, None, "a folder's own entry size says nothing");
    land(
        &mut folder,
        AboutFact::RecursiveSize(Fact::Known(total(5_000_000_000, SizeCoverage::LowerBound))),
    );
    assert_eq!(
        headline(&folder),
        Headline {
            what: HeadlineWhat::Folder,
            size: Some(HeadlineSize {
                bytes: 5_000_000_000,
                at_least: true
            }),
            lead: HeadlineLead::Plain,
        }
    );
    land(
        &mut folder,
        AboutFact::RecursiveSize(Fact::Known(total(5_000_000_000, SizeCoverage::ExactButStale))),
    );
    assert_eq!(headline(&folder).size, exact(5_000_000_000));
    land(
        &mut folder,
        AboutFact::RecursiveSize(Fact::unknown(UnknownWhy::NotIndexed)),
    );
    assert_eq!(headline(&folder).size, None);
}

#[test]
fn a_library_folder_leads_with_its_app_or_that_the_app_is_gone() {
    let mut owned = card(Folder, None, &local());
    land(&mut owned, owned_by_id(OwnerPlace::ApplicationSupport, Some("Tool")));
    assert_eq!(
        headline(&owned).lead,
        HeadlineLead::DataOfApp {
            app: text("Tool"),
            certainty: Certainty::Certain
        }
    );
    let mut leftover = card(Folder, None, &local());
    land(&mut leftover, owned_by_id(OwnerPlace::Containers, None));
    assert_eq!(
        headline(&leftover).lead,
        HeadlineLead::DataOfMissingApp {
            bundle_id: text("com.example.tool")
        }
    );
    let mut probable = card(Folder, None, &local());
    land(
        &mut probable,
        AboutFact::Owner(Fact::Known(OwnerClaim::ByName {
            app: text("Slack"),
            place: OwnerPlace::ApplicationSupport,
        })),
    );
    assert_eq!(
        headline(&probable).lead,
        HeadlineLead::DataOfApp {
            app: text("Slack"),
            certainty: Certainty::Probable
        }
    );
}

#[test]
fn an_app_is_headlined_with_its_version_and_whole_size() {
    let mut app = card(AppBundle, Some("app"), &local());
    land(
        &mut app,
        AboutFact::Bundle(Fact::Known(Bundle {
            bundle_id: Some(text("com.example.tool")),
            display_name: Some(text("Tool")),
            version: Some(text("4.2")),
        })),
    );
    land(
        &mut app,
        AboutFact::RecursiveSize(Fact::Known(total(300_000_000, SizeCoverage::Exact))),
    );
    land(&mut app, last_opened(1_759_400_000));
    assert_eq!(
        headline(&app),
        Headline {
            what: HeadlineWhat::App {
                version: Some(text("4.2"))
            },
            size: exact(300_000_000),
            lead: HeadlineLead::LastOpened { at: 1_759_400_000 },
        }
    );
}

#[test]
fn the_other_targets_are_named_for_what_they_are() {
    assert_eq!(
        headline(&card(Package, Some("rtfd"), &local())).what,
        HeadlineWhat::Package
    );
    assert_eq!(headline(&card(Symlink, None, &local())).what, HeadlineWhat::Symlink);

    let mut entry = card(ArchiveEntry, Some("jpg"), &phone());
    land(&mut entry, identity("a.jpg", 1_000));
    assert_eq!(
        headline(&entry),
        Headline {
            what: HeadlineWhat::ArchiveEntry,
            size: exact(1_000),
            lead: HeadlineLead::Plain,
        }
    );
}

/// The headline never names a removal consequence: its lead's template keys
/// are origin and use facts only.
#[test]
fn the_template_key_rides_on_the_lead() {
    let mut file = card(File, Some("dmg"), &local());
    land(&mut file, chrome_download("https://example.com/Tool.dmg"));
    let wire = serde_json::to_value(headline(&file)).expect("a headline serializes");
    assert_eq!(
        wire,
        serde_json::json!({
            "what": { "type": "file", "kind": null, "extension": null },
            "size": null,
            "lead": { "template": "downloadedFrom", "host": "example.com", "by": "Chrome", "at": 0x6af6_442f },
        })
    );
}
