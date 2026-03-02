//! Linux permission helpers.
//!
//! Linux doesn't have macOS-style Full Disk Access. Standard Unix
//! permissions govern file access, so `check_full_disk_access` always
//! returns `true`. System settings openers attempt `xdg-open` for the
//! appropriate settings panels.

/// Always returns `true` on Linux (no app sandboxing).
#[tauri::command]
pub fn check_full_disk_access() -> bool {
    true
}

/// Opens the system privacy/security settings if a desktop environment is available.
#[tauri::command]
pub fn open_privacy_settings() -> Result<(), String> {
    // GNOME: gnome-control-center privacy
    // KDE: systemsettings (no direct privacy section URL)
    // Fallback: xdg-open is unlikely to have a privacy URI, so return an error.
    Err("Privacy settings aren't needed on Linux — file access is governed by standard Unix permissions, not app sandboxing.".to_string())
}

/// Opens the system appearance settings via the desktop environment.
/// Detects the DE from `$XDG_CURRENT_DESKTOP` and launches the appropriate settings app.
/// When the env var is empty (common when launching via SSH), tries commands in order.
///
/// Some settings apps (notably `gnome-control-center`) refuse to launch unless
/// `XDG_CURRENT_DESKTOP` is set. We pass the expected value to the child process
/// so it works even from SSH sessions where the variable isn't inherited.
///
/// Panel name note: the `background` panel contains style, accent color, and wallpaper
/// settings on both Ubuntu and vanilla GNOME.
#[tauri::command]
pub fn open_appearance_settings() -> Result<(), String> {
    let desktop = std::env::var("XDG_CURRENT_DESKTOP").unwrap_or_default().to_uppercase();
    log::debug!("open_appearance_settings: XDG_CURRENT_DESKTOP='{desktop}'");

    // (command, args, XDG_CURRENT_DESKTOP value the command expects)
    let candidates: &[(&str, &[&str], &str)] = if desktop.contains("GNOME") {
        &[("gnome-control-center", &["background"] as &[&str], "GNOME")]
    } else if desktop.contains("KDE") {
        &[("systemsettings", &["kcm_lookandfeel"] as &[&str], "KDE")]
    } else if desktop.contains("XFCE") {
        &[("xfce4-appearance-settings", &[] as &[&str], "XFCE")]
    } else {
        // Unknown or empty DE (common via SSH) — try all in order
        &[
            ("gnome-control-center", &["background"] as &[&str], "GNOME"),
            ("systemsettings", &["kcm_lookandfeel"], "KDE"),
            ("xfce4-appearance-settings", &[], "XFCE"),
        ]
    };

    for (cmd, args, de_value) in candidates {
        log::debug!("open_appearance_settings: trying {cmd} {}", args.join(" "));
        match std::process::Command::new(cmd)
            .args(*args)
            .env("XDG_CURRENT_DESKTOP", de_value)
            .spawn()
        {
            Ok(_) => return Ok(()),
            Err(e) => log::debug!("open_appearance_settings: {cmd} failed: {e}"),
        }
    }

    Err("Could not open appearance settings. No supported settings app found.".to_string())
}
