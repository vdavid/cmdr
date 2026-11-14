//! The verdict eval's corpus: every card built as an `AboutCard` from the crate's own
//! types and derivations (`plan`, `download_fact`, `real_type`, `FolderDigest::of`,
//! `AppData::of`), so the projection the model reads is the one a person's card sends.
//!
//! ❌ No real file names, hosts, or tokens from anyone's disk: every name, host, team
//! id, and person here is made up, and the home folder is `/Users/jane`.
//!
//! The cards themselves live in `cards_*_test.rs`, one file per group; this file holds
//! the vocabulary and the builders they share.

use cmdr_about::card::{AboutCard, AboutFact, Applied, OsCaps, PlanInput, VolumeCaps, plan};
use cmdr_about::digest::{AppData, ByteShare, FolderClass, FolderDigest, LibraryFolder, SizedChild};
use cmdr_about::fact::{AboutTarget, Fact, FactKind, UnknownWhy};
use cmdr_about::owner::OwnerQuestion;
use cmdr_about::payload::{
    AppRef, ExecutableCheck, GitContext, GitStatus, Identity, Importance, ImportanceBand, RecursiveSize, Signature,
    Signer, SizeCoverage, StapledTicket, Usage,
};
use cmdr_about::provenance::download_fact;
use cmdr_about::sniff::real_type;
use cmdr_about::text::ItemText;
use serde::{Deserialize, Serialize};

use crate::about::verdict::peek::Peek;

/// The made-up person whose Mac the corpus describes.
pub(super) const HOME: &str = "/Users/jane";

/// "Now" for every date on a card: 2026-11-13. Fixed, so a rerun sends the same bytes.
pub(super) const NOW: i64 = 1_794_528_000;

pub(super) const DAY: i64 = 86_400;

/// Which part of the corpus a card belongs to, and so how many runs it gets.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(super) enum Group {
    /// Everyday items: does the take help, and does it stay on the facts?
    Ordinary,
    /// Items where a model most wants to say "go ahead and delete it".
    Tempting,
    /// One injection per untrusted field.
    Hostile,
}

/// One card of the corpus.
pub(super) struct EvalCard {
    /// Stable id, used in the results and the notes.
    pub(super) id: &'static str,
    pub(super) group: Group,
    /// What the card tests, for the judge and the notes: which field is hostile, what a
    /// green light would look like here.
    pub(super) purpose: &'static str,
    /// Where the item is. It rides the projection, so it's synthetic too.
    pub(super) path: String,
    pub(super) card: AboutCard,
    /// The look inside, from the real `peek::look` over a file the eval wrote, or `None`.
    pub(super) peek: Option<Peek>,
    /// Rows still out when the verdict started (left `pending` on the card).
    pub(super) still_gathering: Vec<FactKind>,
    /// Also run in the eval's other languages.
    pub(super) languages: bool,
    /// The card is mostly unknowns or protected: rubric question 4 applies.
    pub(super) unknown_heavy: bool,
    /// Written for the second round, after the first showed a weakness. A card from a
    /// later round is the proof a prompt change holds beyond the cards it was tuned on.
    pub(super) round: u8,
}

impl EvalCard {
    /// A first-round card with no look inside, in English only.
    pub(super) fn new(id: &'static str, group: Group, purpose: &'static str, path: &str, card: AboutCard) -> Self {
        Self {
            id,
            group,
            purpose,
            path: path.to_string(),
            card,
            peek: None,
            still_gathering: Vec::new(),
            languages: false,
            unknown_heavy: false,
            round: 1,
        }
    }

    pub(super) fn peek(mut self, peek: Option<Peek>) -> Self {
        assert!(peek.is_some(), "{}: the look inside found nothing", self.id);
        self.peek = peek;
        self
    }

    pub(super) fn languages(mut self) -> Self {
        self.languages = true;
        self
    }

    pub(super) fn unknown_heavy(mut self) -> Self {
        self.unknown_heavy = true;
        self
    }

    pub(super) fn round(mut self, round: u8) -> Self {
        self.round = round;
        self
    }

    /// The card's rows still pending become the message's "still gathering" list.
    pub(super) fn still_gathering(mut self) -> Self {
        self.still_gathering = self.card.pending();
        assert!(!self.still_gathering.is_empty(), "{}: nothing is pending", self.id);
        self
    }
}

/// Real files for the look inside: each is written to a scratch folder and read back
/// through the verdict's own `peek::look`, so the window, its caps, and its cleaning are
/// what ships. Only the item's synthetic path rides the projection.
pub(super) struct PeekFiles {
    dir: crate::test_support::TestDir,
}

impl PeekFiles {
    pub(super) fn new() -> Self {
        Self {
            dir: crate::test_support::TestDir::new("about-verdict-eval"),
        }
    }

    async fn look(&self, file: &std::path::Path) -> Option<Peek> {
        let path = file.to_str().expect("a UTF-8 scratch path");
        let (_contents, peek) = crate::about::verdict::peek::look(
            path,
            crate::about::verdict::peek::inspect_reader(),
            std::time::Duration::from_secs(10),
        )
        .await;
        peek
    }

    /// Where `name` is written: its own folder, so its extension (what the read goes by)
    /// is the item's. Each look reads its file right after writing it.
    fn fresh(&self, name: &str) -> std::path::PathBuf {
        let folder = self.dir.as_ref().join(folder_for(name));
        std::fs::create_dir_all(&folder).expect("make a scratch folder");
        folder.join(name.replace('/', "_"))
    }

    pub(super) async fn text(&self, name: &str, contents: &str) -> Option<Peek> {
        let file = self.fresh(name);
        std::fs::write(&file, contents).expect("write the text");
        self.look(&file).await
    }

    /// A zip's top-level entry names, shaped as `peek::shape` shapes an archive listing
    /// (each name through `model_text`, the first `PEEK_ENTRIES`). Built here rather than
    /// read: `inspect_file` lists an archive through the volume manager's archive route,
    /// which a unit test process doesn't register, and that listing has its own tests.
    pub(super) async fn zip(&self, _name: &str, entries: &[&str]) -> Option<Peek> {
        Some(Peek::EntryNames {
            format: "zip".to_string(),
            names: entries
                .iter()
                .take(crate::about::verdict::peek::PEEK_ENTRIES)
                .map(|entry| cmdr_about::text::model_text(entry))
                .collect(),
            total: entries.len(),
        })
    }

    pub(super) async fn pdf(&self, name: &str, title: &str, lines: &[&str]) -> Option<Peek> {
        let file = self.fresh(name);
        std::fs::write(&file, one_page_pdf(title, lines)).expect("write the PDF");
        self.look(&file).await
    }

    pub(super) async fn photo(&self, name: &str, make: &str, model: &str, taken: &str) -> Option<Peek> {
        let file = self.fresh(name);
        std::fs::write(&file, jpeg_with_camera(make, model, taken)).expect("write the photo");
        self.look(&file).await
    }
}

/// A folder name per item name, so scratch files never share a folder by accident.
fn folder_for(name: &str) -> String {
    use std::hash::{Hash, Hasher};
    let mut hasher = std::collections::hash_map::DefaultHasher::new();
    name.hash(&mut hasher);
    format!("{:016x}", hasher.finish())
}

/// A one-page PDF with `title` in its info dictionary and `lines` of text on the page.
fn one_page_pdf(title: &str, lines: &[&str]) -> Vec<u8> {
    let escape = |s: &str| s.replace('\\', "\\\\").replace('(', "\\(").replace(')', "\\)");
    let mut stream = String::from("BT /F1 12 Tf 72 720 Td 14 TL\n");
    for line in lines {
        stream.push_str(&format!("({}) Tj T*\n", escape(line)));
    }
    stream.push_str("ET");
    let objects = [
        "<< /Type /Catalog /Pages 2 0 R >>".to_string(),
        "<< /Type /Pages /Kids [3 0 R] /Count 1 >>".to_string(),
        "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 612 792] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>"
            .to_string(),
        format!("<< /Length {} >>\nstream\n{stream}\nendstream", stream.len()),
        "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>".to_string(),
        format!("<< /Title ({}) >>", escape(title)),
    ];
    let mut out = b"%PDF-1.4\n".to_vec();
    let mut offsets = Vec::new();
    for (i, body) in objects.iter().enumerate() {
        offsets.push(out.len());
        out.extend_from_slice(format!("{} 0 obj\n{body}\nendobj\n", i + 1).as_bytes());
    }
    let xref_at = out.len();
    out.extend_from_slice(format!("xref\n0 {}\n0000000000 65535 f \n", objects.len() + 1).as_bytes());
    for offset in offsets {
        out.extend_from_slice(format!("{offset:010} 00000 n \n").as_bytes());
    }
    out.extend_from_slice(
        format!(
            "trailer\n<< /Size {} /Root 1 0 R /Info 6 0 R >>\nstartxref\n{xref_at}\n%%EOF\n",
            objects.len() + 1
        )
        .as_bytes(),
    );
    out
}

/// A JPEG whose EXIF names a camera and a date, and no place.
fn jpeg_with_camera(make: &str, model: &str, taken: &str) -> Vec<u8> {
    use ::exif::experimental::Writer;
    use ::exif::{Field, In, Tag, Value};
    let ascii = |tag, text: &str| Field {
        tag,
        ifd_num: In::PRIMARY,
        value: Value::Ascii(vec![text.as_bytes().to_vec()]),
    };
    let fields = [
        ascii(Tag::Make, make),
        ascii(Tag::Model, model),
        ascii(Tag::DateTimeOriginal, taken),
    ];
    let mut writer = Writer::new();
    for field in &fields {
        writer.push_field(field);
    }
    let mut blob = std::io::Cursor::new(Vec::new());
    writer.write(&mut blob, false).expect("write the EXIF block");
    let mut payload = b"Exif\0\0".to_vec();
    payload.extend(blob.into_inner());
    let mut bytes = vec![0xFF, 0xD8, 0xFF, 0xE1];
    let len = u16::try_from(payload.len() + 2).expect("a small block");
    bytes.extend_from_slice(&len.to_be_bytes());
    bytes.extend_from_slice(&payload);
    bytes.extend_from_slice(&[0xFF, 0xD9]);
    bytes
}

/// The macOS every card is gathered on.
pub(super) const MACOS: OsCaps = OsCaps {
    download_records: true,
    use_history: true,
    code_signing: true,
    default_app: true,
    install_receipts: true,
};

/// The boot disk.
pub(super) fn boot_disk() -> VolumeCaps {
    VolumeCaps::everything(ItemText::new("Macintosh HD"))
}

/// A card under construction: the plan's rows, then the facts as the jobs land them.
pub(super) struct Draft {
    card: AboutCard,
}

/// What kind of item, where, and on what volume.
pub(super) struct Item<'a> {
    pub(super) target: AboutTarget,
    pub(super) path: &'a str,
    pub(super) executable: bool,
    pub(super) caps: VolumeCaps,
    pub(super) in_repo: bool,
}

impl<'a> Item<'a> {
    pub(super) fn file(path: &'a str) -> Self {
        Self {
            target: AboutTarget::File,
            path,
            executable: false,
            caps: boot_disk(),
            in_repo: false,
        }
    }

    pub(super) fn folder(path: &'a str) -> Self {
        Self {
            target: AboutTarget::Folder,
            ..Self::file(path)
        }
    }

    pub(super) fn app(path: &'a str) -> Self {
        Self {
            target: AboutTarget::AppBundle,
            ..Self::file(path)
        }
    }

    pub(super) fn executable(mut self) -> Self {
        self.executable = true;
        self
    }

    pub(super) fn in_repo(mut self) -> Self {
        self.in_repo = true;
        self
    }

    pub(super) fn on(mut self, caps: VolumeCaps) -> Self {
        self.caps = caps;
        self
    }

    /// The plan for this item, as the host makes it.
    pub(super) fn plan(self) -> Draft {
        let name = name_of(self.path);
        let extension = name
            .rsplit_once('.')
            .filter(|(stem, _)| !stem.is_empty())
            .map(|(_, extension)| extension);
        let owner = OwnerQuestion::of(self.path, HOME);
        let plan = plan(PlanInput {
            target: self.target,
            extension,
            is_executable: self.executable,
            contents_local: true,
            caps: &self.caps,
            os: &MACOS,
            owner: &owner,
            in_a_repo: self.in_repo,
            git_snapshot: false,
        });
        Draft {
            card: AboutCard::new(&plan),
        }
    }
}

impl Draft {
    /// Lands `fact`, which the plan must have a row for.
    pub(super) fn with(mut self, fact: AboutFact) -> Self {
        let kind = fact.kind();
        assert_eq!(self.card.apply(fact), Applied::Yes, "the plan has no {kind:?} row");
        self
    }

    /// Lands `fact` when the plan has a row for it, and drops it otherwise (a row the plan
    /// leaves out for this item on this volume).
    pub(super) fn maybe(mut self, fact: AboutFact) -> Self {
        let _ = self.card.apply(fact);
        self
    }

    /// Every row still waiting answers "no record", the routine answer of a plain item.
    pub(super) fn settled(mut self) -> AboutCard {
        self.card.settle_pending(UnknownWhy::NoRecord);
        self.card
    }

    /// Every row still waiting answers `why`.
    pub(super) fn settled_as(mut self, why: UnknownWhy) -> AboutCard {
        self.card.settle_pending(why);
        self.card
    }

    /// The card as it stands, rows still out left `pending` (the verdict's 1.5 s cap).
    pub(super) fn unsettled(self) -> AboutCard {
        self.card
    }
}

pub(super) fn name_of(path: &str) -> &str {
    path.rsplit('/').next().unwrap_or(path)
}

/// A file's identity, sized and dated in days before [`NOW`].
pub(super) fn file_identity(path: &str, bytes: u64, created_days_ago: i64, modified_days_ago: i64) -> AboutFact {
    identity_of(path, Some(bytes), created_days_ago, modified_days_ago, 1)
}

/// A folder's identity: its own entry size says nothing, so there's none.
pub(super) fn folder_identity(path: &str, created_days_ago: i64, modified_days_ago: i64) -> AboutFact {
    identity_of(path, None, created_days_ago, modified_days_ago, 1)
}

pub(super) fn identity_of(
    path: &str,
    bytes: Option<u64>,
    created_days_ago: i64,
    modified_days_ago: i64,
    hard_links: u64,
) -> AboutFact {
    let name = name_of(path);
    AboutFact::Identity(Fact::Known(Identity {
        name: ItemText::new(name),
        name_hid: cmdr_about::text::hidden_in_name(name),
        logical_size: bytes,
        physical_size: bytes,
        created: Some(NOW - created_days_ago * DAY),
        modified: Some(NOW - modified_days_ago * DAY),
        hard_links: Some(hard_links),
        symlink_target: None,
    }))
}

/// What the file's first bytes (and, for a disk image, its trailer) are, against its
/// extension.
pub(super) fn sniffed(path: &str, head: &[u8], tail: Option<&[u8]>) -> AboutFact {
    let name = name_of(path);
    let extension = name
        .rsplit_once('.')
        .filter(|(stem, _)| !stem.is_empty())
        .map(|(_, extension)| extension);
    AboutFact::RealType(Fact::Known(real_type(head, tail, extension)))
}

pub(super) const ZIP: &[u8] = b"PK\x03\x04\x14\0\0\0\x08\0";
pub(super) const PDF: &[u8] = b"%PDF-1.7\n";
pub(super) const MACH_O: &[u8] = b"\xcf\xfa\xed\xfe\x0c\0\0\x01\0\0\0\0";
pub(super) const XAR: &[u8] = b"xar!\0\x1c\0\x01";
pub(super) const SCRIPT: &[u8] = b"#!/bin/sh\necho hi\n";
pub(super) const TEXT: &[u8] = b"Plain words in a file.\n";

/// A UDIF disk image's trailer (`koly` at the start of the last 512 bytes).
pub(super) fn dmg_trailer() -> Vec<u8> {
    let mut tail = b"koly".to_vec();
    tail.resize(512, 0);
    tail
}

/// A download record as macOS writes it: `kMDItemWhereFroms` as a binary plist, and
/// the quarantine field naming `agent`, `days_ago` before [`NOW`].
pub(super) fn downloaded(sources: &[&str], agent: &str, days_ago: i64) -> AboutFact {
    let mut record = Vec::new();
    plist::Value::Array(sources.iter().map(|s| plist::Value::String((*s).to_owned())).collect())
        .to_writer_binary(&mut record)
        .expect("a plist value writes to memory");
    let quarantine = format!("0081;{:x};{agent};", NOW - days_ago * DAY);
    AboutFact::Download(download_fact(Some(&record), Some(quarantine.as_bytes())))
}

pub(super) fn signed(signer: Signer, ticket: StapledTicket, checked: ExecutableCheck) -> AboutFact {
    AboutFact::Signature(Fact::Known(Signature::read(signer, ticket, checked)))
}

pub(super) fn developer(team: &str, org: &str) -> Signer {
    Signer::DeveloperId {
        team: ItemText::new(team),
        org: ItemText::new(org),
    }
}

pub(super) fn opened(days_ago: i64, use_count: Option<u64>) -> AboutFact {
    AboutFact::LastOpened(Fact::Known(Usage {
        last_opened: NOW - days_ago * DAY,
        use_count,
    }))
}

pub(super) fn opens_with(name: &str, id: &str) -> AboutFact {
    AboutFact::DefaultApp(Fact::Known(AppRef {
        name: ItemText::new(name),
        id: Some(ItemText::new(id)),
    }))
}

pub(super) fn importance(band: ImportanceBand, visits: Option<u64>) -> AboutFact {
    AboutFact::Importance(Fact::Known(Importance {
        band,
        stale: false,
        reasons: Vec::new(),
        visits,
    }))
}

pub(super) fn owner(claim: cmdr_about::owner::OwnerClaim) -> AboutFact {
    AboutFact::Owner(Fact::Known(claim))
}

pub(super) fn git(repo: &str, branch: &str, status: Fact<GitStatus>) -> AboutFact {
    AboutFact::Git(Fact::Known(GitContext {
        repo_root: ItemText::new(repo),
        branch: Some(ItemText::new(branch)),
        status,
    }))
}

pub(super) fn size(bytes: u64, files: u64, folders: u64) -> RecursiveSize {
    RecursiveSize {
        logical: bytes,
        physical: bytes,
        files,
        folders,
        coverage: SizeCoverage::Exact,
        updating: false,
        has_symlinks: false,
    }
}

pub(super) fn total(bytes: u64, files: u64, folders: u64) -> AboutFact {
    AboutFact::RecursiveSize(size(bytes, files, folders).into_fact())
}

/// One child row of a digest: `(name, is_folder, bytes)`.
pub(super) type Child<'a> = (&'a str, bool, u64);

/// What a folder holds, from the index: its children, its largest files, and how much
/// sat unchanged for a year (`unchanged` bytes of `of`).
pub(super) fn digest(
    folder: &str,
    class: FolderClass,
    children: &[Child<'_>],
    largest: &[Child<'_>],
    unchanged: Option<(u64, u64)>,
) -> AboutFact {
    let row = |(name, is_folder, bytes): &Child<'_>| SizedChild {
        name: ItemText::new(name),
        path: format!("{folder}/{name}"),
        is_folder: *is_folder,
        size: Fact::Known(size(*bytes, if *is_folder { 40 } else { 1 }, u64::from(*is_folder))),
    };
    let unchanged = match unchanged {
        Some((bytes, of)) => ByteShare::unchanged(bytes, true, &Fact::Known(size(of, 1, 1))),
        None => Fact::unknown(UnknownWhy::NotIndexed),
    };
    AboutFact::FolderDigest(Fact::Known(FolderDigest::of(
        Fact::Known(children.iter().map(row).collect()),
        Fact::Known(largest.iter().map(row).collect()),
        unchanged,
        class,
    )))
}

/// An app's data in the Library: `(path, bytes)` per folder.
pub(super) fn app_data(folders: &[(&str, u64)]) -> AboutFact {
    let folders = folders
        .iter()
        .map(|(path, bytes)| {
            let place = cmdr_about::digest::APP_DATA_PLACES
                .iter()
                .find(|(segment, _)| path.contains(&format!("/Library/{segment}/")))
                .map(|(_, place)| *place)
                .expect("an app-data path names one of the Library's places");
            LibraryFolder {
                name: ItemText::new(name_of(path)),
                path: (*path).to_string(),
                place,
                size: Fact::Known(size(*bytes, 200, 20)),
            }
        })
        .collect();
    AboutFact::AppData(AppData::of(folders, Vec::new()))
}
