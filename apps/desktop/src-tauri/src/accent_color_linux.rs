//! Linux accent color reader.
//!
//! Reads the user's desktop accent color via `gsettings` (GNOME 47+).
//! Falls back to the Cmdr brand gold if gsettings is unavailable or
//! returns an unrecognized value.

use log::{debug, warn};

/// Brand fallback accent (mustard gold from getcmdr.com).
const FALLBACK_ACCENT_HEX: &str = "#d4a006";

/// Maps GNOME 47+ accent-color names to hex values.
/// These are the standard GNOME accent colors as of GNOME 47.
fn gnome_accent_name_to_hex(name: &str) -> Option<&'static str> {
    match name {
        "blue" => Some("#3584e4"),
        "teal" => Some("#2190a4"),
        "green" => Some("#3a944a"),
        "yellow" => Some("#c88800"),
        "orange" => Some("#ed5b00"),
        "red" => Some("#e62d42"),
        "pink" => Some("#d56199"),
        "purple" => Some("#9141ac"),
        "slate" => Some("#6f8396"),
        _ => None,
    }
}

/// Reads the GNOME accent color via `gsettings`.
fn read_accent_color() -> String {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output();

    match output {
        Ok(out) if out.status.success() => {
            // gsettings returns values like 'blue' (with quotes)
            let raw = String::from_utf8_lossy(&out.stdout);
            let name = raw.trim().trim_matches('\'');
            if let Some(hex) = gnome_accent_name_to_hex(name) {
                debug!("GNOME accent color: {name} -> {hex}");
                return hex.to_owned();
            }
            warn!("Unrecognized GNOME accent color '{name}', using fallback");
            FALLBACK_ACCENT_HEX.to_owned()
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr);
            debug!("gsettings failed (not GNOME?): {}", stderr.trim());
            FALLBACK_ACCENT_HEX.to_owned()
        }
        Err(e) => {
            debug!("gsettings not available: {e}");
            FALLBACK_ACCENT_HEX.to_owned()
        }
    }
}

/// Tauri command: returns the current Linux accent color as a hex string.
#[tauri::command]
pub fn get_accent_color() -> String {
    read_accent_color()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gnome_accent_names_resolve() {
        assert_eq!(gnome_accent_name_to_hex("blue"), Some("#3584e4"));
        assert_eq!(gnome_accent_name_to_hex("red"), Some("#e62d42"));
        assert_eq!(gnome_accent_name_to_hex("purple"), Some("#9141ac"));
    }

    #[test]
    fn unknown_accent_name_returns_none() {
        assert_eq!(gnome_accent_name_to_hex("neon"), None);
        assert_eq!(gnome_accent_name_to_hex(""), None);
    }

    #[test]
    fn read_accent_color_returns_hex() {
        let color = read_accent_color();
        assert!(color.starts_with('#'));
        assert!(color.len() == 7);
    }
}
