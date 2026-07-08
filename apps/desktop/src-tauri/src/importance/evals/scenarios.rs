//! The committed synthetic scenarios and the small builder that keeps authoring
//! one cheap.
//!
//! A scenario is folders + derived [`FolderSignals`] + ranking expectations. Rather
//! than hand-writing a full `FolderSignals` per folder (error-prone, and it would
//! drift from how production classifies names/paths), [`ScenarioBuilder`] takes a
//! terse per-folder descriptor — some files with extensions, an age, a couple of
//! flags — and derives the signals through the SAME shared [`classify`] module the
//! production signal-assembly uses. So a synthetic scenario classifies a
//! `node_modules` or a `~/Library/Caches` exactly the way the live scheduler would,
//! and adding a scenario is a few readable lines.
//!
//! Each scenario documents its story in `description` and inline comments: what
//! kind of home it models and which orderings it's meant to exercise. The four
//! shipped scenarios span a developer home, a media/photo home, a downloads-heavy
//! tree, and an SMB/NAS archive (listing-only, so it exercises redistribution).
//!
//! [`classify`]: crate::importance::classify

use super::constraints::Constraint;
use super::scenario::{Availability, Scenario, ScenarioFolder};
use crate::importance::classify::{is_denylisted, is_hidden_or_system, leaf_name, path_class, under_floored_paths};
use crate::importance::scorer::{FolderSignals, PathClass, extension_count};

/// Seconds in a day, for readable age offsets.
const DAY: u64 = 24 * 60 * 60;

/// A fixed "now" all synthetic scenarios score against, so recency is
/// deterministic. An arbitrary round Unix timestamp (2001-09-09T01:46:40Z); the
/// absolute value doesn't matter, only that folder ages are relative to it.
pub const SYNTHETIC_NOW: u64 = 1_000_000_000;

/// A terse description of one folder, from which the builder derives its
/// [`FolderSignals`]. Authoring a scenario is a list of these.
pub struct FolderSpec {
    /// The folder's absolute path. Its leaf name drives the denylist / hidden
    /// classification and its full path drives the path-class prior, exactly as in
    /// production — so name a `node_modules` `node_modules` and it floors.
    pub path: String,
    /// Representative file names directly in the folder (extensions are what
    /// matter — `["a.pdf", "b.jpg"]` reads as two-kind diversity). The count is the
    /// file count; the distinct extensions drive diversity.
    pub files: Vec<String>,
    /// How many days ago the folder was last modified (recency input). `None` means
    /// no mtime known (neutral recency).
    pub age_days: Option<u64>,
    /// Whether a project marker (`.git`, `Cargo.toml`, …) sits here or below —
    /// raises the folder to a project root.
    pub has_marker: bool,
    /// Navigation-visit count, if the visit signal applies (local scenarios). `None`
    /// leaves the optional signal absent.
    pub visits: Option<u32>,
    /// Sampled Spotlight last-used, as days-ago. `None` leaves it absent (and on a
    /// listing-only scenario it's unavailable regardless).
    pub last_used_days: Option<u64>,
}

impl FolderSpec {
    /// A folder at `path` with the given file names, aged `age_days`, no marker, no
    /// optional signals. Chainable setters cover the rest.
    pub fn new(path: &str, files: &[&str], age_days: u64) -> Self {
        Self {
            path: path.to_string(),
            files: files.iter().map(|s| s.to_string()).collect(),
            age_days: Some(age_days),
            has_marker: false,
            visits: None,
            last_used_days: None,
        }
    }

    /// Mark this folder as (at or above) a project root.
    pub fn with_marker(mut self) -> Self {
        self.has_marker = true;
        self
    }

    /// Give this folder a navigation-visit count.
    pub fn with_visits(mut self, visits: u32) -> Self {
        self.visits = Some(visits);
        self
    }

    /// Give this folder a Spotlight last-used age (days ago).
    pub fn with_last_used_days(mut self, days: u64) -> Self {
        self.last_used_days = Some(days);
        self
    }
}

/// Builds a [`Scenario`] from a home root + a list of [`FolderSpec`]s, deriving
/// each folder's signals through the production classifiers.
pub struct ScenarioBuilder {
    name: String,
    description: String,
    home: String,
    availability: Availability,
    folders: Vec<FolderSpec>,
    hard: Vec<Constraint>,
    soft: Vec<Constraint>,
}

impl ScenarioBuilder {
    /// Start a scenario named `name` whose folders live under `home`, scored under
    /// `availability`.
    pub fn new(name: &str, description: &str, home: &str, availability: Availability) -> Self {
        Self {
            name: name.to_string(),
            description: description.to_string(),
            home: home.to_string(),
            availability,
            folders: Vec::new(),
            hard: Vec::new(),
            soft: Vec::new(),
        }
    }

    /// Add a folder.
    pub fn folder(mut self, spec: FolderSpec) -> Self {
        self.folders.push(spec);
        self
    }

    /// Add a hard constraint (must always hold).
    pub fn hard(mut self, c: Constraint) -> Self {
        self.hard.push(c);
        self
    }

    /// Add a soft constraint (counted into the quality score).
    pub fn soft(mut self, c: Constraint) -> Self {
        self.soft.push(c);
        self
    }

    /// Derive signals for every folder and finish the scenario.
    pub fn build(self) -> Scenario {
        let now = SYNTHETIC_NOW;
        // Derive the descendant-floor signal once over the whole folder set, the
        // same shared derivation production uses (a folder under a self-flooring
        // ancestor floors too). Computed here rather than per-spec so a scenario
        // exercises the exact cross-folder logic the scheduler walk does.
        let under_floored = under_floored_paths(self.folders.iter().map(|s| s.path.as_str()), &self.home);
        let folders = self
            .folders
            .iter()
            .map(|spec| {
                let signals = derive_signals(spec, &self.home, now, under_floored.contains(&spec.path));
                ScenarioFolder {
                    path: spec.path.clone(),
                    signals,
                }
            })
            .collect();
        Scenario {
            name: self.name,
            description: self.description,
            availability: self.availability,
            now_secs: now,
            folders,
            hard: self.hard,
            soft: self.soft,
        }
    }
}

/// Derive a [`FolderSignals`] from a spec, using the production classifiers so a
/// synthetic folder classifies exactly as the live scheduler would. A folder with
/// a project marker is a project root (the strongest prior); otherwise the path
/// alone classifies it.
fn derive_signals(spec: &FolderSpec, home: &str, now: u64, under_floored_ancestor: bool) -> FolderSignals {
    let name = leaf_name(&spec.path);
    let file_refs: Vec<&str> = spec.files.iter().map(|s| s.as_str()).collect();
    let path_class = if spec.has_marker {
        PathClass::ProjectRoot
    } else {
        path_class(&spec.path, home)
    };
    FolderSignals {
        name_denylisted: is_denylisted(&name),
        hidden_or_system: is_hidden_or_system(&spec.path, &name, home),
        under_floored_ancestor,
        distinct_extension_count: extension_count(file_refs.iter().copied()),
        file_count: spec.files.len() as u32,
        mtime_secs: spec.age_days.map(|d| now.saturating_sub(d * DAY)),
        has_project_marker: spec.has_marker,
        path_class,
        visit_count: spec.visits,
        last_used_secs: spec.last_used_days.map(|d| now.saturating_sub(d * DAY)),
    }
}

/// Every committed synthetic scenario. The harness scores all of these and pins
/// their aggregate soft-score to a floor.
pub fn all() -> Vec<Scenario> {
    vec![dev_home(), media_home(), downloads_heavy(), nas_archive()]
}

/// A developer's home directory. The archetypal case: an active project root (a
/// `.git` repo with mixed source) must dominate, while the machine output it
/// generates (`node_modules`, build caches, a logs monoculture) must sink to the
/// bottom. Documents and a mixed Downloads sit in between. This is the scenario the
/// scorer most has to get right — it's the "matters vs. machine output" split.
fn dev_home() -> Scenario {
    let home = "/home/dev";
    let p = |rel: &str| format!("{home}/{rel}");
    ScenarioBuilder::new(
        "dev-home",
        "A developer home: an active .git project vs. the node_modules, caches, and logs it generates.",
        home,
        Availability::Local,
    )
    // The active project root: mixed source, recently touched, visited often.
    .folder(
        FolderSpec::new(
            &p("projects/webapp/src"),
            &["main.ts", "app.svelte", "styles.css", "api.ts"],
            1,
        )
        .with_marker()
        .with_visits(12)
        .with_last_used_days(1),
    )
    .folder(
        FolderSpec::new(
            &p("projects/webapp"),
            &["package.json", "README.md", "tsconfig.json"],
            1,
        )
        .with_marker()
        .with_visits(20)
        .with_last_used_days(0),
    )
    // Machine output under the project.
    .folder(FolderSpec::new(&p("projects/webapp/node_modules"), &["index.js"], 1))
    .folder(FolderSpec::new(
        &p("projects/webapp/node_modules/react"),
        &["index.js", "react.js"],
        30,
    ))
    // Deep node_modules internals: these live UNDER a node_modules, so they floor
    // even though their name isn't denylisted. Recent, mixed-extension, and (for
    // the .bin dir) even carrying what looks like project structure — exactly the
    // folders that climbed to the top before the descendant-floor fix.
    .folder(FolderSpec::new(
        &p("projects/webapp/node_modules/react/cjs"),
        &["react.development.js", "react.production.js", "react.min.js"],
        1,
    ))
    .folder(FolderSpec::new(
        &p("projects/webapp/node_modules/.bin"),
        &["tsc", "eslint", "vite"],
        1,
    ))
    // A vendored repo inside node_modules: it has a .git, so absent the fix it would
    // read as a project root and score near the top. Floor beats marker — it stays
    // floored because it lives under node_modules.
    .folder(
        FolderSpec::new(
            &p("projects/webapp/node_modules/vendored-lib"),
            &["package.json", "index.ts", "README.md"],
            1,
        )
        .with_marker(),
    )
    .folder(FolderSpec::new(
        &p("projects/webapp/node_modules/vendored-lib/src"),
        &["core.ts", "utils.ts", "types.ts"],
        1,
    ))
    .folder(FolderSpec::new(&p("projects/webapp/.git"), &["HEAD", "config"], 1))
    // A .git internal subtree: floored as a descendant of a floored .git dir.
    .folder(FolderSpec::new(
        &p("projects/webapp/.git/refs/heads"),
        &["main", "dev", "release"],
        1,
    ))
    .folder(FolderSpec::new(&p("projects/webapp/target"), &["app.o"], 2))
    // A build-output subtree: floored as a descendant of a denylisted `target`.
    .folder(FolderSpec::new(
        &p("projects/webapp/target/debug/deps"),
        &["a.rlib", "b.rlib", "c.d"],
        1,
    ))
    // A logs monoculture: 200 .log files, one extension, stale — machine output.
    .folder(FolderSpec {
        path: p("projects/webapp/logs"),
        files: (0..40).map(|i| format!("run_{i}.log")).collect(),
        age_days: Some(60),
        has_marker: false,
        visits: None,
        last_used_days: None,
    })
    // User content: a mixed Documents tree and a Downloads grab-bag.
    .folder(
        FolderSpec::new(&p("Documents/invoices"), &["jan.pdf", "jan.xlsx", "feb.pdf"], 10)
            .with_visits(4)
            .with_last_used_days(8),
    )
    .folder(
        FolderSpec::new(
            &p("Downloads"),
            &["report.pdf", "installer.dmg", "photos.zip", "shot.png"],
            2,
        )
        .with_visits(6)
        .with_last_used_days(2),
    )
    // A system cache: hidden/system, floored.
    .folder(FolderSpec::new(
        &p("Library/Caches/com.apple.Safari"),
        &["cache_0.bin", "cache_1.bin"],
        40,
    ))
    // ── Hard constraints: must always hold ──
    .hard(Constraint::Above {
        above: p("projects/webapp"),
        below: p("projects/webapp/logs"),
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/node_modules"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/.git"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("Library/Caches/com.apple.Safari"),
        max: 0.0,
    })
    // Descendant-floor: every folder living UNDER a node_modules / .git / target
    // floors to 0, not just the denylisted folder itself.
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/node_modules/react"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/node_modules/react/cjs"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/node_modules/.bin"),
        max: 0.0,
    })
    // A vendored repo inside node_modules stays floored: floor beats project marker.
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/node_modules/vendored-lib"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/node_modules/vendored-lib/src"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/.git/refs/heads"),
        max: 0.0,
    })
    .hard(Constraint::ScoreAtMost {
        path: p("projects/webapp/target/debug/deps"),
        max: 0.0,
    })
    // ── Soft constraints: desirable orderings ──
    .soft(Constraint::TopN {
        path: p("projects/webapp"),
        n: 3,
    })
    .soft(Constraint::TopN {
        path: p("projects/webapp/src"),
        n: 3,
    })
    .soft(Constraint::Above {
        above: p("Downloads"),
        below: p("projects/webapp/logs"),
    })
    .soft(Constraint::Above {
        above: p("Documents/invoices"),
        below: p("projects/webapp/logs"),
    })
    .soft(Constraint::Above {
        above: p("projects/webapp/src"),
        below: p("Downloads"),
    })
    .soft(Constraint::DecileAtMost {
        path: p("Documents/invoices"),
        at_most: 6,
    })
    // A build-output dir is machine output — it should floor like the rest.
    .soft(Constraint::ScoreAtMost {
        path: p("projects/webapp/target"),
        max: 0.0,
    })
    .build()
}

/// A photographer's / media home. The important folders are a curated photo
/// library and an active editing project; the noise is a raw camera dump (a
/// monoculture of one extension), an app-generated screenshots pile, and a
/// thumbnail cache. Exercises "diverse user content over single-kind machine
/// dumps" without any project marker in play.
fn media_home() -> Scenario {
    let home = "/home/photo";
    let p = |rel: &str| format!("{home}/{rel}");
    ScenarioBuilder::new(
        "media-home",
        "A media home: a curated photo library and edit project vs. a raw camera dump, screenshots, and a thumb cache.",
        home,
        Availability::Local,
    )
    // Curated, mixed, frequently opened library.
    .folder(
        FolderSpec::new(
            &p("Pictures/Library/2026/edited"),
            &["a.jpg", "b.tiff", "c.png", "d.raw", "e.psd"],
            3,
        )
        .with_visits(15)
        .with_last_used_days(1),
    )
    .folder(
        FolderSpec::new(
            &p("Documents/editing/wedding"),
            &["seq.prproj", "notes.md", "cover.jpg"],
            2,
        )
        .with_visits(9)
        .with_last_used_days(1),
    )
    // A raw camera dump: hundreds of one-extension files, rarely revisited.
    .folder(FolderSpec {
        path: p("Pictures/CameraDump/2026-06-30"),
        files: (0..60).map(|i| format!("DSC_{i:04}.raw")).collect(),
        age_days: Some(1),
        has_marker: false,
        visits: Some(1),
        last_used_days: Some(1),
    })
    // Auto-captured screenshots: monoculture, app-generated.
    .folder(FolderSpec {
        path: p("Pictures/Screenshots"),
        files: (0..30).map(|i| format!("Screenshot {i}.png")).collect(),
        age_days: Some(2),
        has_marker: false,
        visits: None,
        last_used_days: None,
    })
    // A thumbnail cache: hidden/system, floored.
    .folder(FolderSpec::new(
        &p("Library/Caches/Thumbnails"),
        &["t0.bin", "t1.bin"],
        5,
    ))
    // An app's on-disk cache tree under Pictures: the `.cache` dir self-floors
    // (denylisted + dot-prefixed), and its internals floor as descendants — even
    // though they're recent and mixed-extension.
    .folder(FolderSpec::new(&p("Pictures/.cache"), &["v0.dat"], 1))
    .folder(FolderSpec::new(
        &p("Pictures/.cache/thumbnails/large"),
        &["a.jpg", "b.png", "c.webp"],
        1,
    ))
    // A neutral misc folder for contrast.
    .folder(FolderSpec::new(&p("Desktop"), &["todo.txt", "sketch.png"], 1).with_visits(3))
    // ── Hard constraints ──
    .hard(Constraint::ScoreAtMost {
        path: p("Library/Caches/Thumbnails"),
        max: 0.0,
    })
    // Descendant-floor: a recent, mixed cache-internal subtree still floors.
    .hard(Constraint::ScoreAtMost {
        path: p("Pictures/.cache/thumbnails/large"),
        max: 0.0,
    })
    .hard(Constraint::Above {
        above: p("Pictures/Library/2026/edited"),
        below: p("Pictures/CameraDump/2026-06-30"),
    })
    .hard(Constraint::Above {
        above: p("Pictures/Library/2026/edited"),
        below: p("Pictures/Screenshots"),
    })
    // ── Soft constraints ──
    .soft(Constraint::TopN {
        path: p("Pictures/Library/2026/edited"),
        n: 2,
    })
    .soft(Constraint::Above {
        above: p("Documents/editing/wedding"),
        below: p("Pictures/CameraDump/2026-06-30"),
    })
    .soft(Constraint::Above {
        above: p("Pictures/Library/2026/edited"),
        below: p("Desktop"),
    })
    .soft(Constraint::Above {
        above: p("Pictures/CameraDump/2026-06-30"),
        below: p("Pictures/Screenshots"),
    })
    .soft(Constraint::DecileAtMost {
        path: p("Documents/editing/wedding"),
        at_most: 4,
    })
    .build()
}

/// A downloads-heavy tree. A Downloads folder that's become a dumping ground:
/// installers, an unpacked archive tree, some genuinely-useful documents mixed in.
/// The scorer should surface the folder the person actually curates (a mixed,
/// revisited subfolder) over the disposable installer piles and the unpacked
/// archive's internal directories.
fn downloads_heavy() -> Scenario {
    let home = "/home/grabbag";
    let p = |rel: &str| format!("{home}/{rel}");
    ScenarioBuilder::new(
        "downloads-heavy",
        "A Downloads dumping ground: installers and unpacked archives vs. a curated, revisited subfolder.",
        home,
        Availability::Local,
    )
    // The curated keep pile: mixed kinds, revisited.
    .folder(
        FolderSpec::new(
            &p("Downloads/keep"),
            &["contract.pdf", "budget.xlsx", "plan.md", "logo.svg"],
            4,
        )
        .with_visits(8)
        .with_last_used_days(3),
    )
    // The top-level Downloads: a grab-bag of installers and archives.
    .folder(
        FolderSpec::new(
            &p("Downloads"),
            &["app1.dmg", "app2.dmg", "tool.pkg", "data.zip", "photos.zip"],
            1,
        )
        .with_visits(10),
    )
    // An unpacked archive's internal tree: one-kind, never really browsed.
    .folder(FolderSpec {
        path: p("Downloads/dataset-v2/images"),
        files: (0..80).map(|i| format!("img_{i:04}.jpg")).collect(),
        age_days: Some(1),
        has_marker: false,
        visits: None,
        last_used_days: None,
    })
    .folder(FolderSpec {
        path: p("Downloads/dataset-v2/raw"),
        files: (0..80).map(|i| format!("rec_{i:04}.bin")).collect(),
        age_days: Some(1),
        has_marker: false,
        visits: None,
        last_used_days: None,
    })
    // Installer leftovers: a mounted-dmg staging dir, disposable.
    .folder(FolderSpec::new(&p("Downloads/.installer-tmp"), &["payload.pkg"], 1))
    // A browser/download-manager cache under Downloads: `.cache` self-floors and
    // its recent, mixed internals floor as descendants.
    .folder(FolderSpec::new(&p("Downloads/.cache"), &["state.db"], 1))
    .folder(FolderSpec::new(
        &p("Downloads/.cache/partials"),
        &["a.part", "b.tmp", "c.bin"],
        1,
    ))
    // A genuinely-useful doc folder outside Downloads, for contrast.
    .folder(
        FolderSpec::new(
            &p("Documents/taxes/2025"),
            &["return.pdf", "w2.pdf", "summary.xlsx"],
            20,
        )
        .with_visits(5)
        .with_last_used_days(15),
    )
    // ── Hard constraints ──
    .hard(Constraint::ScoreAtMost {
        path: p("Downloads/.installer-tmp"),
        max: 0.0,
    })
    // Descendant-floor: a recent, mixed cache-internal subtree still floors.
    .hard(Constraint::ScoreAtMost {
        path: p("Downloads/.cache/partials"),
        max: 0.0,
    })
    .hard(Constraint::Above {
        above: p("Downloads/keep"),
        below: p("Downloads/dataset-v2/raw"),
    })
    // ── Soft constraints ──
    .soft(Constraint::Above {
        above: p("Downloads/keep"),
        below: p("Downloads/dataset-v2/images"),
    })
    .soft(Constraint::Above {
        above: p("Documents/taxes/2025"),
        below: p("Downloads/dataset-v2/raw"),
    })
    .soft(Constraint::DecileAtMost {
        path: p("Downloads/keep"),
        at_most: 5,
    })
    .build()
}

/// An SMB/NAS archive, scored listing-only (no Spotlight over a share, so
/// `last_used` is unavailable and its weight redistributes onto the listing
/// signals — the same degradation the SMB scheduler applies). A backups tree, a
/// photo archive, and a media library, with machine-output noise. Exercises the
/// redistribution path: rankings must still separate real content from noise using
/// only the listing signals plus Cmdr-navigation visits.
fn nas_archive() -> Scenario {
    let root = "/Volumes/nas";
    let p = |rel: &str| format!("{root}/{rel}");
    ScenarioBuilder::new(
        "nas-archive",
        "An SMB/NAS archive scored listing-only (no Spotlight): backups, a photo archive, and a media library vs. noise.",
        root,
        Availability::ListingOnly,
    )
    // A curated media library: mixed kinds, browsed over SMB (visits apply).
    .folder(
        FolderSpec::new(&p("media/movies"), &["a.mkv", "b.mp4", "cover.jpg", "info.nfo"], 20)
            .with_visits(14),
    )
    // A photo archive: mixed, occasionally browsed.
    .folder(
        FolderSpec::new(&p("photos/2024"), &["a.jpg", "b.heic", "c.png", "d.mov"], 120)
            .with_visits(6),
    )
    // A backups tree: one-kind archive blobs, old, never browsed for content.
    .folder(FolderSpec {
        path: p("backups/timemachine"),
        files: (0..50).map(|i| format!("band_{i}.spbundle")).collect(),
        age_days: Some(200),
        has_marker: false,
        visits: None,
        last_used_days: None,
    })
    // A media library's auto-generated transcodes: monoculture, machine output.
    .folder(FolderSpec {
        path: p("media/movies/.transcodes"),
        files: (0..40).map(|i| format!("t_{i}.ts")).collect(),
        age_days: Some(5),
        has_marker: false,
        visits: None,
        last_used_days: None,
    })
    // A per-title subdir of the transcode cache: floors as a descendant of the
    // self-flooring `.transcodes` dir even over a listing-only NAS.
    .folder(FolderSpec::new(
        &p("media/movies/.transcodes/1080p"),
        &["a.ts", "b.ts", "index.m3u8"],
        5,
    ))
    // A cache dir on the share: floored (hidden/system by name convention).
    .folder(FolderSpec::new(&p("media/@eaDir"), &["thumb.jpg"], 30))
    // ── Hard constraints ──
    .hard(Constraint::Above {
        above: p("media/movies"),
        below: p("backups/timemachine"),
    })
    .hard(Constraint::Above {
        above: p("media/movies"),
        below: p("media/movies/.transcodes"),
    })
    .hard(Constraint::ScoreAtMost {
        path: p("media/movies/.transcodes"),
        max: 0.0,
    })
    // Descendant-floor over a listing-only NAS: the transcode subdir floors too.
    .hard(Constraint::ScoreAtMost {
        path: p("media/movies/.transcodes/1080p"),
        max: 0.0,
    })
    // ── Soft constraints ──
    .soft(Constraint::TopN {
        path: p("media/movies"),
        n: 2,
    })
    .soft(Constraint::Above {
        above: p("photos/2024"),
        below: p("backups/timemachine"),
    })
    .soft(Constraint::Above {
        above: p("media/movies"),
        below: p("photos/2024"),
    })
    .soft(Constraint::DecileAtMost {
        path: p("photos/2024"),
        at_most: 6,
    })
    .build()
}
