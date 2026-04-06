//! Linux-specific file icon resolution using XDG icon themes.
//!
//! Uses `freedesktop-icons` (pure Rust) to look up icons in the active theme
//! (Adwaita, Papirus, Yaru, etc.) and `mime_guess` for extension-to-MIME mapping.
//! Thread-safe — works with rayon parallelism unlike GTK-based icon providers.

use image::DynamicImage;
use log::{debug, warn};
use std::sync::LazyLock;

/// Current GTK icon theme name, detected once at startup.
static ICON_THEME: LazyLock<String> = LazyLock::new(|| {
    let theme = freedesktop_icons::default_theme_gtk().unwrap_or_else(|| "Adwaita".to_string());
    debug!("XDG icon theme detected: {theme}");
    theme
});

/// Resolves an icon for the given icon ID using the XDG icon theme.
///
/// Supports icon IDs: `"dir"`, `"symlink-dir"`, `"file"`, `"symlink-file"`,
/// `"symlink"`, and `"ext:<extension>"` (for example, `"ext:png"`).
pub fn get_icon_for_id(icon_id: &str, size: u16) -> Option<DynamicImage> {
    let icon_names = icon_names_for_id(icon_id);
    debug!("Icon lookup: {icon_id} -> names: {icon_names:?}");
    lookup_first_icon(&icon_names, size)
}

/// Maps an icon ID to a list of FreeDesktop icon names to try (in priority order).
fn icon_names_for_id(icon_id: &str) -> Vec<String> {
    match icon_id {
        "dir" | "symlink-dir" => vec!["folder".to_string()],
        "file" | "symlink-file" | "symlink" => vec!["text-x-generic".to_string()],
        _ if icon_id.starts_with("ext:") => {
            let ext = &icon_id[4..];
            mime_to_icon_names(ext)
        }
        _ => vec!["text-x-generic".to_string()],
    }
}

/// Converts a file extension to a prioritized list of FreeDesktop icon names.
///
/// Example: "png" -> ["image-png", "image-x-generic", "text-x-generic"]
fn mime_to_icon_names(ext: &str) -> Vec<String> {
    let mut names = Vec::with_capacity(3);

    if let Some(mime) = mime_guess::from_ext(ext).first() {
        // Primary: exact MIME icon (image/png -> image-png)
        let exact = format!("{}-{}", mime.type_(), mime.subtype());
        names.push(exact);

        // Fallback: generic type icon (image/png -> image-x-generic)
        let generic = format!("{}-x-generic", mime.type_());
        if !names.contains(&generic) {
            names.push(generic);
        }
    }

    // Final fallback
    let fallback = "text-x-generic".to_string();
    if !names.contains(&fallback) {
        names.push(fallback);
    }

    names
}

/// Tries each icon name in order, returning the first one found in the theme.
fn lookup_first_icon(names: &[String], size: u16) -> Option<DynamicImage> {
    let theme = &*ICON_THEME;

    for name in names {
        let found = freedesktop_icons::lookup(name)
            .with_size(size)
            .with_theme(theme)
            .with_cache()
            .find();

        match &found {
            Some(path) => {
                let ext = path.extension().and_then(|e| e.to_str()).unwrap_or("?");
                debug!("  {name}: found {path:?} (format: {ext})");

                // Skip SVGs — we don't have an SVG renderer and adding resvg would be heavy
                if ext == "svg" {
                    debug!("  {name}: skipped (SVG)");
                    continue;
                }

                match image::open(path) {
                    Ok(img) => return Some(img),
                    Err(e) => warn!("  {name}: failed to load {path:?}: {e}"),
                }
            }
            None => {
                debug!("  {name}: not found in theme '{theme}'");
            }
        }
    }

    None
}
