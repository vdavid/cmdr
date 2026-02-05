//! Extraction utilities for llama-server binary and shared libraries.

use std::fs;
use std::path::Path;
use tauri::{AppHandle, Manager, Runtime};

/// Binary filename for the llama-server executable.
pub const LLAMA_SERVER_BINARY: &str = "llama-server";

/// Bundled llama-server archive (included in app bundle).
const BUNDLED_LLAMA_ARCHIVE: &str = "resources/llama-server.tar.gz";

/// Path of the llama-server binary inside the tar.gz archive (version-prefixed directory).
const LLAMA_ARCHIVE_BINARY_SUFFIX: &str = "llama-server";

/// Required dylib for llama-server to function.
pub const REQUIRED_DYLIB: &str = "libllama.dylib";

/// Extracts llama-server and its dylibs from the bundled archive.
pub fn extract_bundled_llama_server<R: Runtime>(app: &AppHandle<R>, ai_dir: &Path) -> Result<(), String> {
    log::debug!("AI: extracting bundled llama-server runtime...");

    // Get path to bundled resource
    let resource_path = app
        .path()
        .resolve(BUNDLED_LLAMA_ARCHIVE, tauri::path::BaseDirectory::Resource)
        .map_err(|e| format!("Failed to resolve bundled archive path: {e}"))?;

    if !resource_path.exists() {
        return Err(format!("Bundled llama-server archive not found at: {resource_path:?}"));
    }

    let binary_path = ai_dir.join(LLAMA_SERVER_BINARY);
    extract_llama_server(&resource_path, &binary_path)?;

    // Set executable permissions
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let perms = fs::Permissions::from_mode(0o755);
        fs::set_permissions(&binary_path, perms).map_err(|e| format!("Failed to set permissions: {e}"))?;
    }

    log::debug!("AI: llama-server runtime extracted successfully");
    Ok(())
}

/// Extracts the llama-server binary and required shared libraries from the tar.gz archive.
fn extract_llama_server(archive_path: &Path, dest_path: &Path) -> Result<(), String> {
    use flate2::read::GzDecoder;
    use std::io::Read;
    use tar::{Archive, EntryType};

    let dest_dir = dest_path.parent().ok_or("Invalid destination path")?;
    let file = fs::File::open(archive_path).map_err(|e| format!("Failed to open archive: {e}"))?;
    let gz = GzDecoder::new(file);
    let mut archive = Archive::new(gz);

    let mut found_binary = false;
    let mut extracted_libs = Vec::new();
    let mut symlinks_to_create: Vec<(String, String)> = Vec::new();

    for entry in archive
        .entries()
        .map_err(|e| format!("Failed to read archive entries: {e}"))?
    {
        let mut entry = entry.map_err(|e| format!("Failed to read archive entry: {e}"))?;

        // Get entry type and file info before any operations
        let entry_type = entry.header().entry_type();

        // Get file name and convert to owned String to release the borrow on entry
        let file_name = {
            let path = entry.path().map_err(|e| format!("Failed to get entry path: {e}"))?;
            path.file_name().and_then(|n| n.to_str()).map(String::from)
        };

        let Some(file_name) = file_name else {
            continue;
        };

        // Handle symlinks (common for versioned dylibs like libfoo.dylib -> libfoo.0.dylib)
        if entry_type == EntryType::Symlink && file_name.ends_with(".dylib") {
            let link_target = entry
                .link_name()
                .map_err(|e| format!("Failed to get symlink target for {file_name}: {e}"))?
                .ok_or_else(|| format!("Symlink {file_name} has no target"))?;
            let target_name = link_target
                .file_name()
                .and_then(|n| n.to_str())
                .ok_or_else(|| format!("Invalid symlink target for {file_name}"))?
                .to_string();
            // Defer symlink creation until after all files are extracted
            symlinks_to_create.push((file_name, target_name));
            continue;
        }

        // Extract the llama-server binary
        if file_name == LLAMA_ARCHIVE_BINARY_SUFFIX {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|e| format!("Failed to extract llama-server: {e}"))?;
            fs::write(dest_path, &contents).map_err(|e| format!("Failed to write llama-server binary: {e}"))?;
            found_binary = true;
            log::debug!("AI extract: extracted llama-server binary");
        }
        // Extract all .dylib files (shared libraries required by llama-server)
        else if file_name.ends_with(".dylib") {
            let mut contents = Vec::new();
            entry
                .read_to_end(&mut contents)
                .map_err(|e| format!("Failed to extract {file_name}: {e}"))?;
            let lib_dest = dest_dir.join(&file_name);
            fs::write(&lib_dest, &contents).map_err(|e| format!("Failed to write {file_name}: {e}"))?;
            extracted_libs.push(file_name);
        }
    }

    if !found_binary {
        return Err(String::from("llama-server binary not found in downloaded archive"));
    }

    // Create symlinks after all regular files are extracted
    #[cfg(unix)]
    for (link_name, target_name) in &symlinks_to_create {
        let link_path = dest_dir.join(link_name);
        // Remove existing file/symlink if present (from previous extraction)
        let _ = fs::remove_file(&link_path);
        std::os::unix::fs::symlink(target_name, &link_path)
            .map_err(|e| format!("Failed to create symlink {link_name} -> {target_name}: {e}"))?;
        log::debug!("AI extract: created symlink {link_name} -> {target_name}");
    }

    log::debug!(
        "AI extract: extracted {} libraries, {} symlinks",
        extracted_libs.len(),
        symlinks_to_create.len()
    );
    Ok(())
}
