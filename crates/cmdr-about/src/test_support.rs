//! Fixtures the card, headline, and removal tests share.

use crate::card::{AboutCard, AboutFact, Applied, OsCaps, PlanInput, VolumeCaps, plan};
use crate::digest::{FolderClass, FolderDigest};
use crate::fact::{AboutTarget, Fact, UnknownWhy};
use crate::owner::{InstalledApp, OwnerClaim, OwnerPlace, OwnerQuestion};
use crate::payload::{GitContext, GitStatus, Identity, RecursiveSize, SizeCoverage, Usage};
use crate::provenance::{Download, download_fact};
use crate::sniff::real_type;
use crate::text::ItemText;

pub(crate) fn text(raw: &str) -> ItemText {
    ItemText::new(raw)
}

/// The boot disk.
pub(crate) fn local() -> VolumeCaps {
    VolumeCaps::everything(text("Macintosh HD"))
}

/// A share mounted by the OS: its files can be read and carry xattrs, and Cmdr
/// indexes and logs it, and nothing asks the OS frameworks about it.
pub(crate) fn mounted_share() -> VolumeCaps {
    VolumeCaps {
        system: false,
        git: false,
        ..VolumeCaps::everything(text("Naspolya"))
    }
}

/// macOS, which keeps use history, code signatures, and default apps.
pub(crate) fn macos() -> OsCaps {
    OsCaps {
        use_history: true,
        code_signing: true,
        default_app: true,
        install_receipts: true,
    }
}

/// A phone.
pub(crate) fn phone() -> VolumeCaps {
    VolumeCaps::identity_only(text("Pixel 9"))
}

/// A fresh card for `target` named with `extension`, on `caps`.
pub(crate) fn card(target: AboutTarget, extension: Option<&str>, caps: &VolumeCaps) -> AboutCard {
    AboutCard::new(&plan(PlanInput {
        target,
        extension,
        is_executable: false,
        contents_local: true,
        caps,
        os: &macos(),
        owner: &OwnerQuestion::Receipt,
        in_a_repo: true,
        git_snapshot: false,
    }))
}

/// Applies `fact`, which the card's plan must have a row for.
pub(crate) fn land(card: &mut AboutCard, fact: AboutFact) {
    assert_eq!(card.apply(fact), Applied::Yes, "the plan has no row for this fact");
}

/// Lands "no record" in every row still waiting: what a plain item's jobs answer.
pub(crate) fn land_no_records(card: &mut AboutCard) {
    card.settle_pending(UnknownWhy::NoRecord);
}

pub(crate) fn identity(name: &str, bytes: u64) -> AboutFact {
    AboutFact::Identity(Fact::Known(Identity {
        name: text(name),
        logical_size: Some(bytes),
        physical_size: Some(bytes),
        created: Some(1_750_000_000),
        modified: Some(1_760_000_000),
        hard_links: Some(1),
        symlink_target: None,
    }))
}

/// A real-type fact for a file whose first bytes are `head`.
pub(crate) fn sniffed(head: &[u8], extension: Option<&str>) -> AboutFact {
    AboutFact::RealType(Fact::Known(real_type(head, None, extension)))
}

/// A download record as macOS writes it for `url`, downloaded by Chrome.
pub(crate) fn chrome_download(url: &str) -> AboutFact {
    let mut record = Vec::new();
    plist::Value::Array(vec![plist::Value::String(url.to_owned())])
        .to_writer_binary(&mut record)
        .expect("a plist value writes to memory");
    let fact: Fact<Download> = download_fact(Some(&record), Some(b"0081;6af6442f;Chrome;"));
    AboutFact::Download(fact)
}

pub(crate) fn last_opened(at: i64) -> AboutFact {
    AboutFact::LastOpened(Fact::Known(Usage {
        last_opened: at,
        use_count: Some(12),
    }))
}

pub(crate) fn git(status: Fact<GitStatus>) -> AboutFact {
    AboutFact::Git(Fact::Known(GitContext {
        repo_root: text("/Users/jane/code/tool"),
        branch: Some(text("main")),
        status,
    }))
}

pub(crate) fn owned_by_id(place: OwnerPlace, app: Option<&str>) -> AboutFact {
    AboutFact::Owner(Fact::Known(OwnerClaim::ByBundleId {
        bundle_id: text("com.example.tool"),
        place,
        app: match app {
            Some(name) => InstalledApp::Installed { name: text(name) },
            None => InstalledApp::NotInstalled,
        },
    }))
}

pub(crate) fn total(bytes: u64, coverage: SizeCoverage) -> RecursiveSize {
    RecursiveSize {
        logical: bytes,
        physical: bytes,
        files: 10,
        folders: 2,
        coverage,
        updating: false,
        has_symlinks: false,
    }
}

pub(crate) fn digest(class: FolderClass) -> AboutFact {
    AboutFact::FolderDigest(Fact::Known(FolderDigest {
        class,
        children: Fact::unknown(UnknownWhy::NotIndexed),
        largest_files: Fact::unknown(UnknownWhy::NotIndexed),
        unchanged_for_a_year: Fact::unknown(UnknownWhy::NotIndexed),
    }))
}
