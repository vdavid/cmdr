//! Linux accent color reader.
//!
//! Reads the user's desktop accent color via the XDG Desktop Portal D-Bus API
//! (works on GNOME 47+ and KDE Plasma 5.23+), falling back to `gsettings`
//! (older GNOME), then to the Cmdr brand gold.
//!
//! Also observes the portal's `SettingChanged` signal for live accent color
//! updates, matching the macOS `NSSystemColorsDidChangeNotification` behavior.

use log::{debug, info, warn};
use tauri::{AppHandle, Emitter, Runtime};
use zbus::zvariant::{OwnedValue, Value};

/// Brand fallback accent (mustard gold from getcmdr.com).
const FALLBACK_ACCENT_HEX: &str = "#d4a006";

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_IFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE_NS: &str = "org.freedesktop.appearance";
const ACCENT_KEY: &str = "accent-color";

/// Converts sRGB floats in [0, 1] to a `#rrggbb` hex string.
fn rgb_floats_to_hex(r: f64, g: f64, b: f64) -> String {
    let r8 = (r.clamp(0.0, 1.0) * 255.0).round() as u8;
    let g8 = (g.clamp(0.0, 1.0) * 255.0).round() as u8;
    let b8 = (b.clamp(0.0, 1.0) * 255.0).round() as u8;
    format!("#{r8:02x}{g8:02x}{b8:02x}")
}

/// Extracts (r, g, b) floats from a D-Bus variant value.
/// The portal wraps the color in nested variants: `Variant(Variant((r, g, b)))`.
fn extract_rgb(value: &Value<'_>) -> Option<(f64, f64, f64)> {
    // Unwrap up to two levels of Variant nesting
    let inner = match value {
        Value::Value(v) => match v.as_ref() {
            Value::Value(v2) => v2.as_ref(),
            other => other,
        },
        other => other,
    };

    match inner {
        Value::Structure(s) => {
            let fields = s.fields();
            if fields.len() == 3 {
                if let (Value::F64(r), Value::F64(g), Value::F64(b)) = (&fields[0], &fields[1], &fields[2]) {
                    return Some((*r, *g, *b));
                }
            }
            None
        }
        _ => None,
    }
}

/// Reads accent color via XDG Desktop Portal D-Bus (GNOME 47+, KDE Plasma 5.23+).
fn read_accent_color_portal() -> Option<String> {
    let conn = zbus::blocking::Connection::session().ok()?;
    let proxy = zbus::blocking::Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, PORTAL_IFACE).ok()?;

    let reply: OwnedValue = proxy.call("ReadOne", &(APPEARANCE_NS, ACCENT_KEY)).ok()?;

    let (r, g, b) = extract_rgb(&reply)?;
    let hex = rgb_floats_to_hex(r, g, b);
    debug!("XDG Portal accent color: ({r:.3}, {g:.3}, {b:.3}) -> {hex}");
    Some(hex)
}

/// Maps GNOME 47+ accent-color names to hex values.
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

/// Reads accent color via `gsettings` (older GNOME without portal support).
fn read_accent_color_gsettings() -> Option<String> {
    let output = std::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output()
        .ok()?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        debug!("gsettings failed (not GNOME?): {}", stderr.trim());
        return None;
    }

    let raw = String::from_utf8_lossy(&output.stdout);
    let name = raw.trim().trim_matches('\'');
    if let Some(hex) = gnome_accent_name_to_hex(name) {
        debug!("GNOME accent color: {name} -> {hex}");
        return Some(hex.to_owned());
    }
    warn!("Unrecognized GNOME accent color '{name}'");
    None
}

/// Reads accent color with fallback chain: XDG Portal → gsettings → brand gold.
fn read_accent_color() -> String {
    if let Some(hex) = read_accent_color_portal() {
        return hex;
    }
    debug!("XDG Portal accent color not available, trying gsettings");

    if let Some(hex) = read_accent_color_gsettings() {
        return hex;
    }
    debug!("gsettings accent color not available, using Cmdr brand gold");

    FALLBACK_ACCENT_HEX.to_owned()
}

/// Tauri command: returns the current Linux accent color as a hex string.
#[tauri::command]
pub fn get_accent_color() -> String {
    read_accent_color()
}

/// Starts observing XDG Portal `SettingChanged` signal for live accent color updates.
/// Emits `accent-color-changed` events to the frontend, matching macOS behavior.
pub fn observe_accent_color_changes<R: Runtime>(app_handle: AppHandle<R>) {
    let initial = read_accent_color();
    debug!("Linux accent color: {initial}");

    tauri::async_runtime::spawn(async move {
        if let Err(e) = watch_portal_signal(app_handle).await {
            debug!("Portal accent color watcher not available: {e}");
        }
    });
}

/// Subscribes to the portal's `SettingChanged` D-Bus signal and emits Tauri events.
async fn watch_portal_signal<R: Runtime>(app_handle: AppHandle<R>) -> zbus::Result<()> {
    let conn = zbus::Connection::session().await?;
    let proxy = zbus::Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, PORTAL_IFACE).await?;

    use futures_util::StreamExt;
    let mut signals = proxy.receive_signal("SettingChanged").await?;

    while let Some(signal) = signals.next().await {
        let body = signal.body();
        let Ok((namespace, key, value)) = body.deserialize::<(String, String, OwnedValue)>() else {
            continue;
        };

        if namespace == APPEARANCE_NS && key == ACCENT_KEY {
            if let Some((r, g, b)) = extract_rgb(&value) {
                let hex = rgb_floats_to_hex(r, g, b);
                info!("Accent color changed: {hex}");
                if let Err(e) = app_handle.emit("accent-color-changed", &hex) {
                    warn!("Failed to emit accent-color-changed: {e}");
                }
            }
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rgb_floats_basic_colors() {
        assert_eq!(rgb_floats_to_hex(1.0, 0.0, 0.0), "#ff0000");
        assert_eq!(rgb_floats_to_hex(0.0, 1.0, 0.0), "#00ff00");
        assert_eq!(rgb_floats_to_hex(0.0, 0.0, 1.0), "#0000ff");
        assert_eq!(rgb_floats_to_hex(0.0, 0.0, 0.0), "#000000");
        assert_eq!(rgb_floats_to_hex(1.0, 1.0, 1.0), "#ffffff");
    }

    #[test]
    fn rgb_floats_clamps_out_of_range() {
        assert_eq!(rgb_floats_to_hex(-0.5, 1.5, 0.5), "#00ff80");
    }

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
    fn read_accent_color_returns_valid_hex() {
        let color = read_accent_color();
        assert!(color.starts_with('#'));
        assert_eq!(color.len(), 7);
    }

    #[test]
    fn extract_rgb_from_nested_variants() {
        // Simulate portal response: Variant(Variant((0.5, 0.5, 0.5)))
        let structure = zbus::zvariant::StructureBuilder::new()
            .add_field(0.5_f64)
            .add_field(0.5_f64)
            .add_field(0.5_f64)
            .build()
            .unwrap();
        let inner = Value::Structure(structure);
        let wrapped = Value::Value(Box::new(Value::Value(Box::new(inner))));

        let result = extract_rgb(&wrapped);
        assert_eq!(result, Some((0.5, 0.5, 0.5)));
    }

    #[test]
    fn extract_rgb_wrong_field_count_returns_none() {
        let structure = zbus::zvariant::StructureBuilder::new()
            .add_field(0.5_f64)
            .add_field(0.5_f64)
            .build()
            .unwrap();
        assert_eq!(extract_rgb(&Value::Structure(structure)), None);
    }

    #[test]
    fn extract_rgb_wrong_type_returns_none() {
        let structure = zbus::zvariant::StructureBuilder::new()
            .add_field("not a float")
            .add_field(0.5_f64)
            .add_field(0.5_f64)
            .build()
            .unwrap();
        assert_eq!(extract_rgb(&Value::Structure(structure)), None);
    }
}
