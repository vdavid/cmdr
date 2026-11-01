//! Where a tool's location lives: answered once, by one resolver every tool shares.
//!
//! Pure over [`VolumeFacts`], so every rule here is a table test (`resolve_tests.rs`);
//! `facts.rs` gathers the facts. The rules, and why each one exists:
//! `tools/DETAILS.md` § The location contract.
//!
//! - A NAMED volume resolves whether or not it's connected. Its path is checked
//!   against the volume's root, and a path under a root the volume held earlier
//!   is rebased onto the current one (`rebased_from`).
//! - A BARE path resolves to a LIVE volume or to nothing: the deepest live root
//!   that holds it wins, and `root` holds only what the boot disk really does. A
//!   path under a mount area (`/Volumes/<name>`) with nothing live mounted there
//!   is [`LocationMiss::NoVolumeAtPath`]. ❌ No "saved last root" tier: two saved
//!   shares can share one last root, and a reused mount point would resolve to
//!   the wrong one.

use cmdr_fs::volume::ConnectionState;
use cmdr_fs::volume::app_paths::{path_is_under, path_under, rebase};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::Location;
use super::share_identity::ShareIdentity;
use crate::tools::resources::volumes::{VolumeKind, connection_state_token};

/// How many volumes a miss's `nearby` names at most.
const NEARBY_CAP: usize = 8;

/// A tool's location INPUT: the flat `path` + optional `volumeId` pair.
///
/// Flat, not nested, so an AI client's schema stays a pair of strings. Also what
/// a miss echoes as its `location`: the place that was asked about, which may name
/// no volume.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct LocationInput {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub volume_id: Option<String>,
    pub path: String,
}

impl LocationInput {
    /// Read the pair off a tool's params. `None` when `path` is missing or empty;
    /// an empty `volumeId` counts as absent.
    pub(crate) fn from_params(params: &Value) -> Option<Self> {
        let path = params.get("path").and_then(Value::as_str).filter(|p| !p.is_empty())?;
        let volume_id = params
            .get("volumeId")
            .and_then(Value::as_str)
            .filter(|id| !id.is_empty())
            .map(str::to_string);
        Some(Self {
            volume_id,
            path: path.to_string(),
        })
    }
}

/// Which drive index serves a volume and what it has, so a tool can pick a source
/// without asking the index again.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct IndexSources {
    /// The index that holds this volume's rows. ❗ Not always the volume's own id:
    /// a cloud drive in `~/Library/CloudStorage` is its own volume, but the boot
    /// disk's index (`root`) walks it.
    pub volume_id: String,
    /// A live index instance serves reads (registered; not a promise it's scanned).
    pub live: bool,
    /// An index database is on disk, live instance or not.
    pub stored: bool,
    /// When the on-disk index's last walk finished (unix seconds), for a volume
    /// that isn't live: what an offline answer is dated by. `None` for a live
    /// volume (its index's own status says how fresh it is) and for an index
    /// whose first walk never finished.
    pub stored_as_of: Option<u64>,
}

impl IndexSources {
    /// A volume's own index, with nothing in it: the base a test overrides.
    #[cfg(test)]
    pub(crate) fn none(volume_id: &str) -> Self {
        Self {
            volume_id: volume_id.to_string(),
            live: false,
            stored: false,
            stored_as_of: None,
        }
    }
}

/// One volume, as the resolver needs to know it. Gathered by `facts.rs`.
#[derive(Clone, Debug)]
pub(crate) struct VolumeFact {
    pub id: String,
    pub name: String,
    pub kind: VolumeKind,
    /// The app-path prefix every location on this volume starts with, from
    /// [`volume_root`].
    pub root: String,
    /// Registered right now, so its tools can read it.
    pub live: bool,
    pub connection_state: Option<ConnectionState>,
    /// Roots this volume answered at before its current one: the saved row's last
    /// mount path and every root it held this session. Never the current root.
    pub earlier_roots: Vec<String>,
    pub index: IndexSources,
    /// For an SMB share: which server, share, and account it is, so two ids of
    /// one share can be proven one (`same_share.rs`). `None` for anything else.
    pub share: Option<ShareIdentity>,
}

/// Everything the resolver knows about volumes, taken at one instant.
#[derive(Clone, Debug, Default)]
pub(crate) struct VolumeFacts {
    pub volumes: Vec<VolumeFact>,
    /// Folders the OS mounts volumes into (`/Volumes` on macOS). A path inside one
    /// belongs to whatever is mounted there, ❌ never to the disk the folder sits
    /// on: a dead NAS mount point is not a boot-disk folder.
    pub mount_areas: Vec<String>,
}

impl VolumeFacts {
    /// The volume `id` names, if any.
    pub(crate) fn get(&self, id: &str) -> Option<&VolumeFact> {
        self.volumes.iter().find(|v| v.id == id)
    }

    /// The mount point `path` falls under inside a mount area, and its name:
    /// `/Volumes/naspi/papers` → (`/Volumes/naspi`, `naspi`).
    fn mount_point_of<'a>(&self, path: &'a str) -> Option<(String, &'a str)> {
        self.mount_areas.iter().find_map(|area| {
            let rest = path_under(path, area).filter(|rest| !rest.is_empty())?;
            let name = rest.split('/').next()?;
            Some((format!("{}/{name}", area.trim_end_matches('/')), name))
        })
    }

    /// Whether `path` sits inside a mount area, where only a mounted volume can
    /// hold it. The case a path-routing tool can't tell from a boot-disk folder.
    pub(crate) fn under_mount_area(&self, path: &str) -> bool {
        self.mount_point_of(path).is_some()
    }

    /// Whether `volume` really holds `path`: it's under the volume's root, and the
    /// volume isn't a disk a mount area merely sits on.
    fn holds(&self, volume: &VolumeFact, path: &str) -> bool {
        if !path_is_under(path, &volume.root) {
            return false;
        }
        match self.mount_point_of(path) {
            Some((mount_point, _)) => path_is_under(&volume.root, &mount_point),
            None => true,
        }
    }
}

/// Where a volume's root comes from, in the order [`volume_root`] trusts them.
#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RootSources<'a> {
    /// The registered volume's mount (or scheme) root.
    pub live: Option<&'a str>,
    /// The index database's `volume_path` meta.
    pub index_meta: Option<&'a str>,
    /// The saved row's last mount path.
    pub saved: Option<&'a str>,
}

/// A volume's root: the ONE answer every tool reads, so two tools never disagree
/// about where a volume's paths start. The live root when registered, else the
/// index's recorded root, else the saved row's last mount path.
pub(crate) fn volume_root(sources: RootSources<'_>) -> Option<String> {
    sources
        .live
        .or(sources.index_meta)
        .or(sources.saved)
        .filter(|root| !root.is_empty())
        .map(str::to_string)
}

/// A location the resolver placed, plus what a tool needs to read it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct ResolvedLocation {
    /// The app path on the volume it was placed on: what the tool reads and echoes.
    pub location: Location,
    pub name: String,
    pub kind: VolumeKind,
    pub root: String,
    /// The volume is registered, so its live sources can answer.
    pub live: bool,
    pub index: IndexSources,
    /// The path as asked, when it sat under a root the volume has since left.
    pub rebased_from: Option<String>,
    /// The volume the read was ASKED about, when this location is a proven
    /// same-share, same-account sibling answering in its place
    /// ([`VolumeFacts::readers`]). `None` for the volume asked.
    pub via: Option<String>,
}

/// Why the resolver couldn't place a location. Every variant carries the request
/// (tilde-expanded), so the tool can echo it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) enum LocationMiss {
    /// The named volume isn't one the app knows.
    UnknownVolume { requested: LocationInput },
    /// The named volume exists, but the path is under neither its root nor any
    /// root it held earlier.
    PathOutsideVolume { requested: LocationInput, root: String },
    /// A bare path no live volume holds: a dead mount point, or a scheme path
    /// nothing live serves.
    NoVolumeAtPath {
        requested: LocationInput,
        nearby: Vec<NearbyVolume>,
    },
}

/// A volume a miss suggests instead, with the next call already spelled out.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct NearbyVolume {
    pub volume_id: String,
    pub name: String,
    pub kind: &'static str,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub connection_state: Option<&'static str>,
    pub root: String,
    /// The requested place on THIS volume (`/Volumes/naspi/papers` →
    /// `/Volumes/naspi-1/papers`), so the next call is a copy, not arithmetic.
    pub candidate: Location,
}

/// Place `input` on a volume, or say why it can't be placed.
pub(crate) fn resolve(facts: &VolumeFacts, input: &LocationInput) -> Result<ResolvedLocation, LocationMiss> {
    // App paths start with `/` or a scheme, so a leading `~` can only mean this
    // machine's home folder.
    let requested = LocationInput {
        volume_id: input.volume_id.clone(),
        path: cmdr_fs::tilde::expand_tilde(&input.path),
    };
    match requested.volume_id.as_deref() {
        Some(id) => resolve_named(facts, id, &requested),
        None => resolve_bare(facts, &requested),
    }
}

fn resolve_named(facts: &VolumeFacts, id: &str, requested: &LocationInput) -> Result<ResolvedLocation, LocationMiss> {
    let Some(volume) = facts.get(id) else {
        return Err(LocationMiss::UnknownVolume {
            requested: requested.clone(),
        });
    };
    if facts.holds(volume, &requested.path) {
        return Ok(placed(volume, requested.path.clone(), None));
    }
    // The most specific earlier root wins, the same rule as for live roots.
    let rebased = volume
        .earlier_roots
        .iter()
        .filter(|earlier| path_is_under(&requested.path, earlier))
        .max_by_key(|earlier| earlier.trim_end_matches('/').len())
        .and_then(|earlier| rebase(&requested.path, earlier, &volume.root));
    match rebased {
        Some(path) => Ok(placed(volume, path, Some(requested.path.clone()))),
        None => Err(LocationMiss::PathOutsideVolume {
            requested: requested.clone(),
            root: volume.root.clone(),
        }),
    }
}

fn resolve_bare(facts: &VolumeFacts, requested: &LocationInput) -> Result<ResolvedLocation, LocationMiss> {
    let owner = facts
        .volumes
        .iter()
        .filter(|v| v.live && facts.holds(v, &requested.path))
        .max_by_key(|v| v.root.trim_end_matches('/').len());
    match owner {
        Some(volume) => Ok(placed(volume, requested.path.clone(), None)),
        None => Err(LocationMiss::NoVolumeAtPath {
            requested: requested.clone(),
            nearby: nearby(facts, &requested.path, None),
        }),
    }
}

fn placed(volume: &VolumeFact, path: String, rebased_from: Option<String>) -> ResolvedLocation {
    ResolvedLocation {
        location: Location {
            volume_id: volume.id.clone(),
            path,
        },
        name: volume.name.clone(),
        kind: volume.kind,
        root: volume.root.clone(),
        live: volume.live,
        index: volume.index.clone(),
        rebased_from,
        via: None,
    }
}

/// The volumes a miss on `path` points to instead, live ones first, at most
/// [`NEARBY_CAP`]. `exclude` is the volume the miss is about.
///
/// In order of preference: volumes whose mount name shares the requested one's
/// stem (`naspi-1` for `/Volumes/naspi`, macOS's suffix for a second mount), else
/// volumes whose root holds the path, else every remote volume. Each carries the
/// requested place re-rooted onto it as `candidate`.
pub(crate) fn nearby(facts: &VolumeFacts, path: &str, exclude: Option<&str>) -> Vec<NearbyVolume> {
    let others = || facts.volumes.iter().filter(move |v| Some(v.id.as_str()) != exclude);
    let mount = facts.mount_point_of(path);
    // Where the requested place lands on another volume's root.
    let candidate_on = |volume: &VolumeFact| match &mount {
        Some((mount_point, _)) => rebase(path, mount_point, &volume.root).unwrap_or_else(|| volume.root.clone()),
        None if path_is_under(path, &volume.root) => path.to_string(),
        None => volume.root.clone(),
    };

    let mut picks: Vec<&VolumeFact> = match &mount {
        Some((_, name)) => {
            let wanted = mount_stem(name);
            others()
                .filter(|v| {
                    v.root
                        .trim_end_matches('/')
                        .rsplit('/')
                        .next()
                        .is_some_and(|last| mount_stem(last).eq_ignore_ascii_case(wanted))
                })
                .collect()
        }
        None => others().filter(|v| path_is_under(path, &v.root)).collect(),
    };
    if picks.is_empty() {
        picks = others().filter(|v| v.kind != VolumeKind::Local).collect();
    }
    picks.sort_by(|a, b| b.live.cmp(&a.live).then_with(|| a.name.cmp(&b.name)));
    picks
        .into_iter()
        .take(NEARBY_CAP)
        .map(|v| NearbyVolume {
            volume_id: v.id.clone(),
            name: v.name.clone(),
            kind: v.kind.token(),
            connection_state: v.connection_state.map(connection_state_token),
            root: v.root.clone(),
            candidate: Location {
                volume_id: v.id.clone(),
                path: candidate_on(v),
            },
        })
        .collect()
}

/// A mount name without the `-N` macOS appends to a second mount of the same
/// name: `naspi-1` → `naspi`.
fn mount_stem(name: &str) -> &str {
    match name.rsplit_once('-') {
        Some((stem, suffix))
            if !stem.is_empty() && !suffix.is_empty() && suffix.bytes().all(|b| b.is_ascii_digit()) =>
        {
            stem
        }
        _ => name,
    }
}

#[cfg(test)]
#[path = "resolve_tests.rs"]
mod tests;
