use serde::Deserialize;
use std::path::{Path, PathBuf};

#[derive(Deserialize)]
pub struct BenchConfig {
    pub targets: Vec<Target>,
}

#[derive(Deserialize, Clone)]
pub struct Target {
    pub name: String,
    pub host: String,
    pub share: String,
    #[serde(default)]
    pub username: Option<String>,
    #[serde(default)]
    pub password: Option<String>,
    #[serde(default)]
    pub guest: bool,
    pub native_mount: String,
    /// Override max chunk size in KB. Pi's Samba hangs on 1MB despite negotiating 8MB.
    #[serde(default)]
    pub max_chunk_kb: Option<u32>,
}

impl Target {
    /// Base path on the OS-mounted share for native benchmark files.
    pub fn native_test_base(&self) -> PathBuf {
        PathBuf::from(&self.native_mount).join("_test-native")
    }

    /// Base SMB path (relative to share root) for direct benchmark files.
    pub fn direct_test_base() -> &'static str {
        "_test-direct"
    }
}

impl BenchConfig {
    pub fn load(path: &Path) -> Result<Self, String> {
        let content = std::fs::read_to_string(path)
            .map_err(|e| format!("Can't read {}: {e}", path.display()))?;
        let config: BenchConfig =
            toml::from_str(&content).map_err(|e| format!("Invalid TOML: {e}"))?;

        // Validate targets
        for t in &config.targets {
            if !t.guest && (t.username.is_none() || t.password.is_none()) {
                eprintln!(
                    "Warning: Target '{}' needs username+password (or set guest = true). It will be skipped.",
                    t.name
                );
            }
            let mount = Path::new(&t.native_mount);
            if !mount.exists() {
                eprintln!(
                    "Warning: Native mount '{}' for target '{}' doesn't exist. Mount it first: open smb://{}/ in Finder. It will be skipped.",
                    t.native_mount, t.name, t.host
                );
            }
        }

        Ok(config)
    }
}
