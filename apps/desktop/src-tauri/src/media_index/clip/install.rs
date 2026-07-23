//! On-demand CLIP model install: download → SHA-256 verify → zip unpack → gate (plan
//! Decision 9 — new code, reusing only the resumable HTTP GET in [`ai::download`]).
//!
//! Distinct from the GGUF two-flag gate: Core ML models ship as `.mlpackage` DIRECTORY
//! bundles (zipped), so this adds a generic zip extractor and — unlike `ai/`'s size-only
//! check — a **checksum** verify. The bytes at the download URL must match the SHA-256
//! pinned in [`CLIP_TOWERS`] exactly, or install refuses: a truncated or tampered
//! download is never unpacked, so a half-model can never load and mis-embed (data safety).
//!
//! The app ships nothing; it downloads the two towers on demand and compiles each
//! `.mlpackage` to `.mlmodelc` on-device at first use (`.mlmodelc` is OS-version-specific
//! — never bundle a prebuilt one). The towers are produced by the out-of-tree conversion
//! script (`apps/desktop/scripts/convert-clip-model/`).
//!
//! [`ai::download`]: crate::ai::download

use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};

/// The CLIP model identifier baked into the provenance stamp. A change here (a new model)
/// bumps every row's `clip_stamp` and re-embeds (the two-part staleness contract).
pub const CLIP_MODEL_ID: &str = "openai-clip-vit-b32";

/// One downloadable tower: the artifact name, its pinned download URL, the byte size, and
/// the SHA-256 of the zip the [conversion script](../../../../scripts/convert-clip-model)
/// produced. The hash + size are the exact values the script printed; install verifies the
/// download against them, so the bytes at `url` MUST match (a mismatch refuses to install).
///
/// The artifacts must be uploaded to `url` (David-only — agents never upload). Until an
/// artifact is live at its URL, a download fails and the feature stays gated off; the
/// pinned hash still guarantees that whatever downloads is exactly the converted bytes.
pub struct ClipTowerSpec {
    /// The artifact/zip filename (also the `.mlpackage` dir name once unpacked).
    pub artifact: &'static str,
    /// The pinned download URL.
    pub url: &'static str,
    /// The lowercase hex SHA-256 of the zip (from the conversion script's output).
    pub sha256: &'static str,
    /// The zip's byte size (for the honest download-size UI copy).
    pub size_bytes: u64,
    /// The `.mlpackage` directory name once unpacked (inside the model dir).
    pub package_dir: &'static str,
}

/// The unfilled-hash sentinel: while a tower's `sha256` is this, install refuses (there is
/// no real artifact to verify against yet). Retained as the "not configured" guard even
/// though the real hashes are pinned below.
pub const PLACEHOLDER_SHA: &str = "0000000000000000000000000000000000000000000000000000000000000000";

/// The two towers the semantic-search feature needs: the image tower (enrichment embeds
/// every photo) and the text tower (query encoding). Hash + size are the conversion
/// script's printed output (OpenAI CLIP ViT-B/32, fp16, non-palettized; conversion
/// fidelity cosine 1.0000 vs the torch reference — verified 2026-07-16). Combined ~392 MB.
///
/// Bigger than a palettized model would be (8-bit palettization computed NaN on the text
/// tower — see the conversion script), so it's the honest correct-but-larger download;
/// shrinking it via a per-layer palettization exclusion is a future optimization.
///
/// Hosted on Hugging Face (`veszelovszki/cmdr-clip-vit-b32-coreml`, public); `resolve` URLs
/// 302-redirect to the CDN (reqwest follows) and honor Range requests (the resumable
/// download's 206 resume — both verified 2026-07-16, byte-exact against the pinned hashes,
/// unauthenticated). The hash guarantees whatever downloads is exactly the converted,
/// verified model.
pub const CLIP_TOWERS: &[ClipTowerSpec] = &[
    ClipTowerSpec {
        artifact: "clip-image.mlpackage.zip",
        url: "https://huggingface.co/veszelovszki/cmdr-clip-vit-b32-coreml/resolve/main/clip-image.mlpackage.zip",
        sha256: "b3e3a3fe9a2268a05ea0d9e97f60e3a905d07f83a51678a467b03a629f77b237",
        size_bytes: 207_920_562,
        package_dir: "clip-image.mlpackage",
    },
    ClipTowerSpec {
        artifact: "clip-text.mlpackage.zip",
        url: "https://huggingface.co/veszelovszki/cmdr-clip-vit-b32-coreml/resolve/main/clip-text.mlpackage.zip",
        sha256: "d48091c587b32033920870dfb9db3d30866162e46f3e69d07e79df1a99e5d7d3",
        size_bytes: 183_694_108,
        package_dir: "clip-text.mlpackage",
    },
];

/// The combined download size of all towers, for the honest "~X MB" settings copy.
pub fn total_download_bytes() -> u64 {
    CLIP_TOWERS.iter().map(|t| t.size_bytes).sum()
}

/// A typed install failure. Never string-matched (`no-string-matching`).
#[derive(Debug)]
pub enum InstallError {
    /// The downloaded bytes' SHA-256 didn't match the pinned hash — refuse to install
    /// (a truncated or tampered download). Carries the expected + actual for logging.
    ChecksumMismatch { expected: String, actual: String },
    /// The tower's pinned hash is still the placeholder (no real artifact uploaded yet).
    NotConfigured,
    /// A filesystem or zip-extraction error.
    Io(std::io::Error),
    /// The zip was structurally invalid.
    Zip(String),
}

impl std::fmt::Display for InstallError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            InstallError::ChecksumMismatch { expected, actual } => {
                write!(f, "CLIP model checksum mismatch (expected {expected}, got {actual})")
            }
            InstallError::NotConfigured => write!(f, "CLIP model artifact is not configured yet"),
            InstallError::Io(e) => write!(f, "CLIP model install io error: {e}"),
            InstallError::Zip(m) => write!(f, "CLIP model archive invalid: {m}"),
        }
    }
}

impl std::error::Error for InstallError {}

impl From<std::io::Error> for InstallError {
    fn from(e: std::io::Error) -> Self {
        InstallError::Io(e)
    }
}

/// The directory a volume-agnostic CLIP model install lives in, beside the app's other
/// model data. Both towers unpack here.
pub fn clip_model_dir(data_dir: &Path) -> PathBuf {
    data_dir.join("clip-model")
}

/// The lowercase hex SHA-256 of `bytes`.
pub fn sha256_hex(bytes: &[u8]) -> String {
    let mut hasher = Sha256::new();
    hasher.update(bytes);
    hex_lower(&hasher.finalize())
}

/// The lowercase hex SHA-256 of a file, streamed (never loads the whole archive into RAM).
pub fn sha256_file(path: &Path) -> Result<String, InstallError> {
    use std::io::Read;
    let mut file = std::fs::File::open(path)?;
    let mut hasher = Sha256::new();
    let mut buf = [0u8; 1 << 16];
    loop {
        let n = file.read(&mut buf)?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex_lower(&hasher.finalize()))
}

fn hex_lower(bytes: &[u8]) -> String {
    let mut s = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        s.push(char::from_digit((b >> 4) as u32, 16).unwrap_or('0'));
        s.push(char::from_digit((b & 0xf) as u32, 16).unwrap_or('0'));
    }
    s
}

/// Verify `zip_path` against `expected` (lowercase hex SHA-256). `Ok(())` only on an exact
/// match — the gate that stops a truncated/tampered download from being unpacked. A
/// placeholder `expected` (no real artifact yet) is [`InstallError::NotConfigured`].
pub fn verify_checksum(zip_path: &Path, expected: &str) -> Result<(), InstallError> {
    if expected == PLACEHOLDER_SHA {
        return Err(InstallError::NotConfigured);
    }
    let actual = sha256_file(zip_path)?;
    if actual == expected {
        Ok(())
    } else {
        Err(InstallError::ChecksumMismatch {
            expected: expected.to_string(),
            actual,
        })
    }
}

/// Install one downloaded tower: verify the checksum FIRST, and only then unpack the zip
/// into `model_dir`. The verify-before-unpack order is the data-safety guarantee — a
/// truncated download's bytes never reach the extractor, so a half-model can't be
/// assembled and loaded.
pub fn install_tower(zip_path: &Path, expected_sha: &str, model_dir: &Path) -> Result<(), InstallError> {
    verify_checksum(zip_path, expected_sha)?;
    std::fs::create_dir_all(model_dir)?;
    unzip_into(zip_path, model_dir)
}

/// Extract every entry of `zip_path` under `dest_dir`, rejecting any entry whose path would
/// escape `dest_dir` (a zip-slip guard). Directory bundles (`.mlpackage`) are recreated
/// verbatim.
pub fn unzip_into(zip_path: &Path, dest_dir: &Path) -> Result<(), InstallError> {
    let file = std::fs::File::open(zip_path)?;
    let mut archive = zip::ZipArchive::new(file).map_err(|e| InstallError::Zip(e.to_string()))?;
    for i in 0..archive.len() {
        let mut entry = archive.by_index(i).map_err(|e| InstallError::Zip(e.to_string()))?;
        // Reject absolute / `..` entries that would write outside dest (zip-slip).
        let Some(rel) = entry.enclosed_name() else {
            return Err(InstallError::Zip(format!("unsafe archive entry: {}", entry.name())));
        };
        let out_path = dest_dir.join(rel);
        if entry.is_dir() {
            std::fs::create_dir_all(&out_path)?;
        } else {
            if let Some(parent) = out_path.parent() {
                std::fs::create_dir_all(parent)?;
            }
            let mut out = std::fs::File::create(&out_path)?;
            std::io::copy(&mut entry, &mut out)?;
        }
    }
    Ok(())
}

/// A tower's downloaded `.mlpackage` source directory inside the model dir.
pub fn package_path(model_dir: &Path, tower: &ClipTowerSpec) -> PathBuf {
    model_dir.join(tower.package_dir)
}

/// A tower's compiled `.mlmodelc` directory (sibling of the `.mlpackage`). This is what the
/// worker actually loads; it's kept after the `.mlpackage` source is reclaimed (plan M5a).
pub fn compiled_path(model_dir: &Path, tower: &ClipTowerSpec) -> PathBuf {
    model_dir.join(tower.package_dir).with_extension("mlmodelc")
}

/// Whether both towers are present on disk — either as the downloaded `.mlpackage` source OR
/// as the compiled `.mlmodelc` (plan M5a). Cheap existence check the gate seeds from at
/// startup.
///
/// The either/or is the M5a reclaim: after a verified compile the ~550 MB combined
/// `.mlpackage` sources are deleted, leaving only the compiled models, and the feature must
/// still read as installed. If a compiled model later fails to load (an OS upgrade can
/// invalidate it) and no `.mlpackage` remains to recompile from, the load path deletes the
/// stale `.mlmodelc` (see [`drop_compiled`]), flipping this back to `false` so the standard
/// download flow refetches the pinned zip.
pub fn is_installed(data_dir: &Path) -> bool {
    let dir = clip_model_dir(data_dir);
    CLIP_TOWERS
        .iter()
        .all(|t| package_path(&dir, t).is_dir() || compiled_path(&dir, t).is_dir())
}

/// Delete a tower's downloaded `.mlpackage` source once its compiled `.mlmodelc` exists (plan
/// M5a): the compiled model is all the worker needs, so the source is dead weight. Returns
/// `Ok(true)` when it removed the source, `Ok(false)` when there was nothing to remove or no
/// compiled model yet (the source is NEVER removed before a verified compile). Best-effort —
/// an error is surfaced for logging, never fatal (keeping the package only costs disk).
pub fn reclaim_source_package(model_dir: &Path, tower: &ClipTowerSpec) -> std::io::Result<bool> {
    let package = package_path(model_dir, tower);
    if !compiled_path(model_dir, tower).is_dir() || !package.is_dir() {
        return Ok(false);
    }
    std::fs::remove_dir_all(&package)?;
    Ok(true)
}

/// Delete a tower's compiled `.mlmodelc` (plan M5a fallback): called when it failed to load
/// and no `.mlpackage` remains to recompile from, so [`is_installed`] flips back to `false`
/// and the standard download flow refetches the pinned zip. Best-effort.
pub fn drop_compiled(model_dir: &Path, tower: &ClipTowerSpec) -> std::io::Result<()> {
    let compiled = compiled_path(model_dir, tower);
    if compiled.is_dir() {
        std::fs::remove_dir_all(&compiled)?;
    }
    Ok(())
}

/// The CLIP provenance stamp for staleness (`media_status.clip_stamp`): the model id +
/// the OS version, so a model change OR an OS upgrade (which recompiles `.mlmodelc` and can
/// drift ANE output) re-embeds. `None` when no model is installed.
pub fn installed_stamp(data_dir: &Path) -> Option<String> {
    is_installed(data_dir).then(|| clip_stamp_for(&os_version()))
}

/// Build the stamp string from an OS-version component (extracted for testing).
fn clip_stamp_for(os: &str) -> String {
    format!("clip;model={CLIP_MODEL_ID};os={os}")
}

/// The OS version component of the stamp. On macOS the real `major.minor.patch` (an OS
/// upgrade recompiles `.mlmodelc` and can drift ANE output, so it must re-embed); a fixed
/// token elsewhere (CLIP only runs on macOS anyway).
fn os_version() -> String {
    #[cfg(target_os = "macos")]
    {
        use objc2_foundation::NSProcessInfo;
        let v = NSProcessInfo::processInfo().operatingSystemVersion();
        format!("{}.{}.{}", v.majorVersion, v.minorVersion, v.patchVersion)
    }
    #[cfg(not(target_os = "macos"))]
    {
        "non-macos".to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    /// A tiny valid zip containing one file, written to a temp path. Returns (path, sha).
    fn make_zip(dir: &Path, name: &str, body: &[u8]) -> (PathBuf, String) {
        let zip_path = dir.join(name);
        {
            let file = std::fs::File::create(&zip_path).unwrap();
            let mut w = zip::ZipWriter::new(file);
            w.start_file::<_, ()>(
                "clip-image.mlpackage/Manifest.json",
                zip::write::SimpleFileOptions::default(),
            )
            .unwrap();
            w.write_all(body).unwrap();
            w.finish().unwrap();
        }
        let sha = sha256_file(&zip_path).unwrap();
        (zip_path, sha)
    }

    #[test]
    fn sha256_hex_is_the_known_empty_hash() {
        // The SHA-256 of the empty input — a fixed, well-known vector.
        assert_eq!(
            sha256_hex(b""),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );
    }

    #[test]
    fn a_correct_checksum_verifies_and_a_wrong_one_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        let (zip_path, sha) = make_zip(dir.path(), "ok.zip", b"hello model");
        // Exact match ⇒ Ok.
        assert!(verify_checksum(&zip_path, &sha).is_ok());
        // A single flipped hex digit ⇒ ChecksumMismatch, never Ok.
        let mut wrong: Vec<char> = sha.chars().collect();
        wrong[0] = if wrong[0] == 'a' { 'b' } else { 'a' };
        let wrong: String = wrong.into_iter().collect();
        assert!(matches!(
            verify_checksum(&zip_path, &wrong),
            Err(InstallError::ChecksumMismatch { .. })
        ));
    }

    #[test]
    fn a_truncated_download_never_installs() {
        // Pre-fix intent: install must verify BEFORE unpacking, so a truncated archive
        // (whose bytes don't match the pinned hash) is rejected and NOTHING is extracted.
        let dir = tempfile::tempdir().unwrap();
        let (zip_path, full_sha) = make_zip(dir.path(), "full.zip", b"the complete model bytes");
        // Truncate the file on disk to simulate an interrupted download.
        let full = std::fs::read(&zip_path).unwrap();
        std::fs::write(&zip_path, &full[..full.len() / 2]).unwrap();
        let model_dir = dir.path().join("clip-model");
        let result = install_tower(&zip_path, &full_sha, &model_dir);
        assert!(result.is_err(), "a truncated archive must not install");
        assert!(
            !model_dir.join("clip-image.mlpackage").exists(),
            "nothing is unpacked when the checksum fails"
        );
    }

    #[test]
    fn install_tower_unpacks_a_verified_zip() {
        let dir = tempfile::tempdir().unwrap();
        let (zip_path, sha) = make_zip(dir.path(), "good.zip", b"model weights");
        let model_dir = dir.path().join("clip-model");
        install_tower(&zip_path, &sha, &model_dir).expect("verified zip installs");
        assert!(
            model_dir.join("clip-image.mlpackage/Manifest.json").exists(),
            "the bundle is unpacked after a matching checksum"
        );
    }

    #[test]
    fn a_placeholder_hash_refuses_to_install() {
        let dir = tempfile::tempdir().unwrap();
        let (zip_path, _sha) = make_zip(dir.path(), "x.zip", b"x");
        assert!(matches!(
            verify_checksum(&zip_path, PLACEHOLDER_SHA),
            Err(InstallError::NotConfigured)
        ));
    }

    #[test]
    fn the_stamp_carries_model_and_os() {
        let s = clip_stamp_for("15.1.0");
        assert!(s.contains(CLIP_MODEL_ID));
        assert!(s.contains("os=15.1.0"));
    }

    // ── M5a: is_installed is package-OR-compiled, and the reclaim/fallback fs helpers ──

    /// Make a fake tower directory (`.mlpackage` or `.mlmodelc`) with one file inside.
    fn make_dir(path: &Path) {
        std::fs::create_dir_all(path).unwrap();
        std::fs::write(path.join("weights.bin"), b"x").unwrap();
    }

    #[test]
    fn is_installed_accepts_either_the_package_or_the_compiled_model() {
        let dir = tempfile::tempdir().unwrap();
        let data_dir = dir.path();
        let model_dir = clip_model_dir(data_dir);
        assert!(!is_installed(data_dir), "nothing on disk ⇒ not installed");

        // Only the compiled `.mlmodelc` for BOTH towers (the post-reclaim steady state).
        for t in CLIP_TOWERS {
            make_dir(&compiled_path(&model_dir, t));
        }
        assert!(
            is_installed(data_dir),
            "compiled-only reads as installed (package reclaimed)"
        );

        // Dropping one compiled model (the OS-upgrade-invalidated fallback) flips it off.
        drop_compiled(&model_dir, &CLIP_TOWERS[0]).unwrap();
        assert!(!is_installed(data_dir), "a missing tower ⇒ not installed ⇒ re-download");

        // The freshly-downloaded `.mlpackage` (before any compile) also reads as installed.
        make_dir(&package_path(&model_dir, &CLIP_TOWERS[0]));
        assert!(is_installed(data_dir), "package-only also reads as installed");
    }

    #[test]
    fn reclaim_removes_the_package_only_after_a_compile_exists() {
        let dir = tempfile::tempdir().unwrap();
        let model_dir = clip_model_dir(dir.path());
        let tower = &CLIP_TOWERS[0];

        // Package present, no compiled model yet ⇒ reclaim is a no-op (never delete the
        // source before a verified compile).
        make_dir(&package_path(&model_dir, tower));
        assert!(
            !reclaim_source_package(&model_dir, tower).unwrap(),
            "no compile ⇒ keep source"
        );
        assert!(package_path(&model_dir, tower).is_dir(), "package still there");

        // Once the compiled model exists, reclaim deletes the source and keeps the compiled.
        make_dir(&compiled_path(&model_dir, tower));
        assert!(
            reclaim_source_package(&model_dir, tower).unwrap(),
            "compiled ⇒ source reclaimed"
        );
        assert!(!package_path(&model_dir, tower).is_dir(), "source `.mlpackage` gone");
        assert!(compiled_path(&model_dir, tower).is_dir(), "compiled `.mlmodelc` kept");

        // Idempotent: with the source already gone, a second reclaim is a no-op.
        assert!(
            !reclaim_source_package(&model_dir, tower).unwrap(),
            "nothing left to reclaim"
        );
    }
}
