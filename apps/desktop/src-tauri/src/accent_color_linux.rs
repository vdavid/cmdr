//! Linux accent color reader.
//!
//! Reads the user's desktop accent color via the XDG Desktop Portal D-Bus API
//! (works on GNOME 47+ and KDE Plasma 5.23+), falling back to `gsettings`
//! (older GNOME), then to the Cmdr brand gold.
//!
//! Also observes the portal's `SettingChanged` signal for live accent color
//! updates, matching the macOS `NSSystemColorsDidChangeNotification` behavior.

use std::time::Duration;

use log::{debug, info, warn};
use tauri::{AppHandle, Runtime};
use tauri_specta::Event as _;
use zbus::zvariant::{OwnedValue, Value};

use crate::system_events::AccentColorChanged;

/// Brand fallback accent (mustard gold from getcmdr.com).
const FALLBACK_ACCENT_HEX: &str = "#d4a006";

const PORTAL_DEST: &str = "org.freedesktop.portal.Desktop";
const PORTAL_PATH: &str = "/org/freedesktop/portal/desktop";
const PORTAL_IFACE: &str = "org.freedesktop.portal.Settings";
const APPEARANCE_NS: &str = "org.freedesktop.appearance";
const ACCENT_KEY: &str = "accent-color";

/// Hard cap on each probe. A healthy local session-bus / gsettings responds in milliseconds;
/// anything slower than this means a misconfigured environment (orphan socket with no daemon,
/// stalled subprocess) and we should fall through to the next tier instead of blocking app
/// startup. Without this cap, `zbus::Connection::session()` can hang indefinitely on a
/// half-configured D-Bus (observed at 120 s+ on the GitHub Actions Ubuntu runner).
const PROBE_TIMEOUT: Duration = Duration::from_millis(500);

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
            if fields.len() == 3
                && let (Value::F64(r), Value::F64(g), Value::F64(b)) = (&fields[0], &fields[1], &fields[2])
            {
                return Some((*r, *g, *b));
            }
            None
        }
        _ => None,
    }
}

/// Reads accent color via XDG Desktop Portal D-Bus (GNOME 47+, KDE Plasma 5.23+).
/// Bounded by `PROBE_TIMEOUT` so a stalled session bus can't hang the caller.
async fn read_accent_color_portal() -> Option<String> {
    let probe = async {
        let conn = zbus::Connection::session().await.ok()?;
        let proxy = zbus::Proxy::new(&conn, PORTAL_DEST, PORTAL_PATH, PORTAL_IFACE)
            .await
            .ok()?;
        let reply: OwnedValue = proxy.call("ReadOne", &(APPEARANCE_NS, ACCENT_KEY)).await.ok()?;
        let (r, g, b) = extract_rgb(&reply)?;
        Some((r, g, b))
    };
    let (r, g, b) = tokio::time::timeout(PROBE_TIMEOUT, probe).await.ok().flatten()?;
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
/// Bounded by `PROBE_TIMEOUT` so a hung gsettings subprocess can't block startup.
async fn read_accent_color_gsettings() -> Option<String> {
    let probe = tokio::process::Command::new("gsettings")
        .args(["get", "org.gnome.desktop.interface", "accent-color"])
        .output();
    let output = tokio::time::timeout(PROBE_TIMEOUT, probe).await.ok()?.ok()?;

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
/// Each tier is wrapped in `PROBE_TIMEOUT` (500 ms) so a half-configured session
/// bus or a stalled `gsettings` subprocess can't wedge the caller — at worst we
/// pay ~1 s total in the pathological case before settling on the brand fallback.
async fn read_accent_color() -> String {
    if let Some(hex) = read_accent_color_portal().await {
        return hex;
    }
    debug!("XDG Portal accent color not available, trying gsettings");

    if let Some(hex) = read_accent_color_gsettings().await {
        return hex;
    }
    debug!("gsettings accent color not available, using Cmdr brand gold");

    FALLBACK_ACCENT_HEX.to_owned()
}

/// Tauri command: returns the current Linux accent color as a hex string.
#[tauri::command]
#[specta::specta]
pub async fn get_accent_color() -> String {
    read_accent_color().await
}

/// Starts observing XDG Portal `SettingChanged` signal for live accent color updates.
/// Emits `accent-color-changed` events to the frontend, matching macOS behavior.
pub fn observe_accent_color_changes<R: Runtime>(app_handle: AppHandle<R>) {
    tauri::async_runtime::spawn(async move {
        let initial = read_accent_color().await;
        debug!("Linux accent color: {initial}");
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

        if namespace == APPEARANCE_NS
            && key == ACCENT_KEY
            && let Some((r, g, b)) = extract_rgb(&value)
        {
            let hex = rgb_floats_to_hex(r, g, b);
            info!("Accent color changed: {hex}");
            if let Err(e) = (AccentColorChanged { hex }).emit(&app_handle) {
                warn!("Failed to emit accent-color-changed: {e}");
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

    /// Verifies the two contracts that matter:
    ///   1. `read_accent_color` always returns within the combined `PROBE_TIMEOUT`
    ///      budget (`portal` + `gsettings` ≤ 2 × 500 ms = 1 s), so a flaky
    ///      session-bus or hung subprocess can never block app startup.
    ///   2. The result is a valid `#rrggbb` hex string, regardless of which
    ///      tier produced it (portal / gsettings / brand fallback).
    ///
    /// Replaces the older `read_accent_color_returns_valid_hex`, which had the
    /// same shape assertions but **no timeout assertion** and called the
    /// unbounded blocking zbus connect. That hung CI for 120 s when the
    /// GitHub Actions Ubuntu runner shipped an orphan session-bus socket
    /// (path set, no daemon serving). The new timeout in production code makes
    /// the function honest, and this test pins it down so the regression can't
    /// come back.
    ///
    /// Asserting a `<2 s` wall-clock with a `500 ms` budget gives plenty of
    /// slack for slow CI runners / heavy parallelism while still catching the
    /// "blocking forever" regression — anything close to 2 s means a probe
    /// stopped honoring its timeout.
    #[tokio::test]
    async fn read_accent_color_is_bounded_and_returns_valid_hex() {
        let start = std::time::Instant::now();
        let color = read_accent_color().await;
        let elapsed = start.elapsed();

        assert!(
            elapsed < Duration::from_secs(2),
            "read_accent_color took {elapsed:?}, expected < 2 s (PROBE_TIMEOUT={PROBE_TIMEOUT:?})",
        );
        assert!(color.starts_with('#'), "expected #rrggbb, got {color}");
        assert_eq!(color.len(), 7, "expected #rrggbb (7 chars), got {color}");
        for c in color.chars().skip(1) {
            assert!(c.is_ascii_hexdigit(), "invalid hex digit in {color}");
        }
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
