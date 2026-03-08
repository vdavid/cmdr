//! Copies bundled llama-server binary and shared libraries to the AI data directory.

use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};

/// Binary filename for the llama-server executable.
pub const LLAMA_SERVER_BINARY: &str = "llama-server";

/// Required dylib for llama-server to function.
pub const REQUIRED_DYLIB: &str = "libllama.dylib";

/// Bundled resource directory containing pre-extracted llama-server files.
const BUNDLED_AI_DIR: &str = "resources/ai";

/// Copies llama-server and its dylibs from the bundled resources to the AI data directory.
pub fn extract_bundled_llama_server<R: Runtime>(app: &AppHandle<R>, ai_dir: &Path) -> Result<(), String> {
    log::debug!("AI: copying bundled llama-server runtime to {:?}...", ai_dir);

    fs::create_dir_all(ai_dir).map_err(|e| format!("Failed to create AI directory: {e}"))?;

    let resource_dir = app
        .path()
        .resolve(BUNDLED_AI_DIR, tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve bundled AI resource path: {e}"))?;

    if !resource_dir.exists() {
        return Err(format!("Bundled AI resource directory not found at: {resource_dir:?}"));
    }

    let entries =
        fs::read_dir(&resource_dir).map_err(|e| format!("Failed to read bundled AI directory: {e}"))?;

    let mut copied_count = 0;
    for entry in entries {
        let entry = entry.map_err(|e| format!("Failed to read directory entry: {e}"))?;
        let src_path = entry.path();

        // Skip non-files (directories, the .version marker, etc.)
        let file_name = entry.file_name();
        let name = file_name.to_string_lossy();
        if name.starts_with('.') {
            continue;
        }

        let dest_path = ai_dir.join(&*name);

        // Handle symlinks: recreate them in the destination
        #[cfg(unix)]
        if src_path.symlink_metadata().is_ok_and(|m| m.file_type().is_symlink()) {
            let target =
                fs::read_link(&src_path).map_err(|e| format!("Failed to read symlink {name}: {e}"))?;
            let _ = fs::remove_file(&dest_path);
            std::os::unix::fs::symlink(&target, &dest_path)
                .map_err(|e| format!("Failed to create symlink {name}: {e}"))?;
            log::debug!("AI extract: symlink {} -> {}", name, target.display());
            continue;
        }

        // Copy regular file
        if !src_path.is_file() {
            continue;
        }

        fs::copy(&src_path, &dest_path).map_err(|e| format!("Failed to copy {name}: {e}"))?;

        // Ensure executable permissions
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let perms = fs::Permissions::from_mode(0o755);
            fs::set_permissions(&dest_path, perms).map_err(|e| format!("Failed to set permissions on {name}: {e}"))?;
        }

        copied_count += 1;
    }

    if copied_count == 0 {
        return Err(String::from("No files found in bundled AI resource directory"));
    }

    // Verify the binary was copied
    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    if !binary_path.exists() {
        return Err(String::from("llama-server binary not found in bundled resources"));
    }

    log::debug!("AI: copied {copied_count} files from bundled resources");
    Ok(())
}
