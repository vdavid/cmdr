//! Machine-output folder names: what a search hides by default and the importance
//! scorer treats as known-unimportant.

/// Common system, build, and cache directory names: machine output nobody searches
/// for and nobody ranks as important.
///
/// One list, read by three consumers so they can't drift: search
/// applies it when `SearchQuery::exclude_system_dirs` isn't `Some(false)`, the
/// importance scorer treats a match as known-unimportant, and the folder-size
/// tooltip command skips a match when it sums a directory. ❌ Match on NAME
/// EQUALITY, never a substring: a folder called
/// `my-build-notes` is not build output.
///
/// ❌ The SCANNER is not one of them, and never should be (Decision 6 of
/// `docs/specs/unindexed-search-plan.md`): this tier is large and sits under
/// folders people search, so skipping it at walk time would stamp coverage on
/// parents whose `dir_stats` are badly short. `cmdr-index`'s `should_exclude` is the
/// structural policy the walk does apply, and it shares no names with this list.
pub const SYSTEM_DIR_EXCLUDES: &[&str] = &[
    // Package managers & build tools
    "node_modules",
    ".pnpm-store",
    ".npm",
    ".yarn",
    ".cargo",
    ".m2",
    ".gradle",
    // VCS
    ".git",
    ".svn",
    ".hg",
    // Python
    "__pycache__",
    ".venv",
    "venv",
    ".tox",
    // JS/TS build output
    "build",
    "dist",
    ".next",
    ".nuxt",
    ".cache",
    ".parcel-cache",
    "target",
    // macOS system & caches
    "Caches",
    "CacheStorage",
    "Cache",
    "GPUCache",
    "ScriptCache",
    "GrShaderCache",
    "ShaderCache",
    "Logs",
    "Cookies",
    "WebKit",
    "Saved Application State",
    ".Trash",
    ".Spotlight-V100",
    ".fseventsd",
    ".DocumentRevisions-V100",
    // IDE workspace caches
    "workspaceStorage",
    "DerivedData",
];
