//! Pure geometry rules for main-window state persistence.
//!
//! Everything here is plain data plus total functions: no Tauri types, no
//! locks, no I/O. That's deliberate. The Tauri-facing half (`mod.rs`) is hard
//! to test because it needs a real window and a real event loop; keeping the
//! rules that are easy to get wrong (maximize bookkeeping, off-screen
//! detection) in here means they're covered by ordinary unit tests.
//!
//! Ported and modified from `tauri-plugin-window-state` v2.4.1, used under MIT.
//! Copyright 2019-2023 Tauri Programme within The Commons Conservancy. The
//! on-disk schema is kept compatible with that plugin so upgrading users keep
//! their window position. See `DETAILS.md` for what we changed and why.

use serde::{Deserialize, Serialize};

/// Persisted geometry for one window, in **physical pixels**.
///
/// Field names and JSON shape match `tauri-plugin-window-state`'s
/// `.window-state.json`, so a file written by the plugin loads here unchanged.
/// The plugin's `decorated` field is intentionally absent: Cmdr's main window
/// is always decorated, so restoring a saved value could only ever break the
/// title bar. Serde ignores it on read and drops it on the next write.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(default)]
pub struct WindowGeometry {
    pub width: u32,
    pub height: u32,
    pub x: i32,
    pub y: i32,
    /// Position before the window was maximized. Maximizing moves the window
    /// to the monitor corner, which would otherwise overwrite the only record
    /// of where the user actually had it. See [`apply_move`].
    pub prev_x: i32,
    pub prev_y: i32,
    pub maximized: bool,
    pub visible: bool,
    pub fullscreen: bool,
}

impl Default for WindowGeometry {
    fn default() -> Self {
        Self {
            width: 0,
            height: 0,
            x: 0,
            y: 0,
            prev_x: 0,
            prev_y: 0,
            maximized: false,
            // A window we've never seen should be shown, not hidden forever.
            visible: true,
            fullscreen: false,
        }
    }
}

/// A monitor's bounds in the same physical-pixel coordinate space as
/// [`WindowGeometry`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MonitorRect {
    pub x: i32,
    pub y: i32,
    pub width: u32,
    pub height: u32,
}

impl MonitorRect {
    /// True if this monitor and the given window rect overlap by at least one
    /// pixel.
    ///
    /// **Deliberately different from upstream**, which tested whether any of
    /// the window's four corners lands inside the monitor. That misses a
    /// window larger than the monitor it sits on (all four corners hang off
    /// the edges, no corner is "inside"), and such a window would silently
    /// lose its position on restore. A rectangle-overlap test is both simpler
    /// and correct for that case.
    pub fn overlaps(&self, x: i32, y: i32, width: u32, height: u32) -> bool {
        let (m_left, m_top) = (self.x, self.y);
        let m_right = self.x.saturating_add_unsigned(self.width);
        let m_bottom = self.y.saturating_add_unsigned(self.height);

        let (w_left, w_top) = (x, y);
        let w_right = x.saturating_add_unsigned(width);
        let w_bottom = y.saturating_add_unsigned(height);

        w_left < m_right && w_right > m_left && w_top < m_bottom && w_bottom > m_top
    }
}

/// Whether a saved geometry still lands on one of the currently-connected
/// monitors.
///
/// When it doesn't (the monitor was unplugged, or the resolution shrank so the
/// old spot no longer exists) the caller must leave placement to the OS rather
/// than restoring a position the user can't reach.
pub fn is_on_any_monitor(geometry: &WindowGeometry, monitors: &[MonitorRect]) -> bool {
    let (x, y) = restore_position(geometry);
    monitors
        .iter()
        .any(|monitor| monitor.overlaps(x, y, geometry.width, geometry.height))
}

/// The position to restore the window to.
///
/// A window saved while maximized has `x`/`y` pointing at the monitor corner,
/// which is where maximizing put it, not where the user had it. Restoring that
/// would strand the window in the corner as soon as it's un-maximized, so the
/// pre-maximize position wins.
pub fn restore_position(geometry: &WindowGeometry) -> (i32, i32) {
    if geometry.maximized {
        (geometry.prev_x, geometry.prev_y)
    } else {
        (geometry.x, geometry.y)
    }
}

/// Records a window move, keeping the previous position for [`restore_position`].
pub fn apply_move(geometry: &mut WindowGeometry, x: i32, y: i32) {
    geometry.prev_x = geometry.x;
    geometry.prev_y = geometry.y;
    geometry.x = x;
    geometry.y = y;
}

/// Records a window resize. Returns `false` and leaves `geometry` untouched
/// for a degenerate size.
///
/// A zero dimension is never a real user-visible window; macOS reports one
/// transiently while minimizing or while a space transition is in flight.
/// Persisting it would restore an invisible window on next launch.
pub fn apply_resize(geometry: &mut WindowGeometry, width: u32, height: u32) -> bool {
    if width == 0 || height == 0 {
        return false;
    }
    geometry.width = width;
    geometry.height = height;
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    fn geometry() -> WindowGeometry {
        WindowGeometry {
            width: 800,
            height: 600,
            x: 100,
            y: 200,
            ..Default::default()
        }
    }

    fn monitor() -> MonitorRect {
        MonitorRect {
            x: 0,
            y: 0,
            width: 1920,
            height: 1080,
        }
    }

    #[test]
    fn default_window_is_visible() {
        // A never-seen window must not be restored hidden: nothing would ever show it.
        assert!(WindowGeometry::default().visible);
    }

    #[test]
    fn move_keeps_the_previous_position() {
        let mut g = geometry();
        apply_move(&mut g, 300, 400);
        assert_eq!((g.x, g.y), (300, 400));
        assert_eq!((g.prev_x, g.prev_y), (100, 200));
    }

    #[test]
    fn restore_uses_live_position_when_not_maximized() {
        let g = geometry();
        assert_eq!(restore_position(&g), (100, 200));
    }

    #[test]
    fn restore_uses_pre_maximize_position_when_maximized() {
        // Maximizing moves the window to the monitor corner (0,0 here), so the
        // only record of where the user had it is prev_x/prev_y.
        let mut g = geometry();
        apply_move(&mut g, 0, 0);
        g.maximized = true;
        assert_eq!(restore_position(&g), (100, 200));
    }

    #[test]
    fn resize_rejects_a_zero_dimension() {
        let mut g = geometry();
        assert!(!apply_resize(&mut g, 0, 600));
        assert!(!apply_resize(&mut g, 800, 0));
        assert_eq!((g.width, g.height), (800, 600));
    }

    #[test]
    fn resize_accepts_a_real_size() {
        let mut g = geometry();
        assert!(apply_resize(&mut g, 1024, 768));
        assert_eq!((g.width, g.height), (1024, 768));
    }

    #[test]
    fn window_fully_inside_a_monitor_overlaps_it() {
        assert!(monitor().overlaps(100, 200, 800, 600));
    }

    #[test]
    fn window_entirely_off_screen_does_not_overlap() {
        assert!(!monitor().overlaps(3000, 200, 800, 600));
        assert!(!monitor().overlaps(-900, 200, 800, 600));
    }

    #[test]
    fn window_straddling_an_edge_overlaps() {
        // Half off the right edge: still reachable, so still a valid position.
        assert!(monitor().overlaps(1900, 200, 800, 600));
    }

    #[test]
    fn window_touching_an_edge_exactly_does_not_overlap() {
        // Right edge is exclusive: a window starting at x=1920 is on the *next*
        // monitor, not this one.
        assert!(!monitor().overlaps(1920, 200, 800, 600));
    }

    #[test]
    fn window_larger_than_its_monitor_still_overlaps() {
        // Pre-fix this would have failed: upstream tested the four corners, and
        // a window covering the whole monitor has none of them inside it. The
        // user would silently lose their position on restore.
        let small = MonitorRect {
            x: 0,
            y: 0,
            width: 1280,
            height: 800,
        };
        assert!(small.overlaps(-100, -100, 1600, 1000));
    }

    #[test]
    fn saved_position_on_a_disconnected_monitor_is_rejected() {
        // Saved on a second monitor to the right, which is now unplugged.
        let g = WindowGeometry {
            x: 2100,
            y: 300,
            ..geometry()
        };
        assert!(!is_on_any_monitor(&g, &[monitor()]));
    }

    #[test]
    fn saved_position_on_a_still_connected_monitor_is_accepted() {
        let second = MonitorRect {
            x: 1920,
            y: 0,
            width: 1920,
            height: 1080,
        };
        let g = WindowGeometry {
            x: 2100,
            y: 300,
            ..geometry()
        };
        assert!(is_on_any_monitor(&g, &[monitor(), second]));
    }

    #[test]
    fn maximized_window_is_checked_against_its_pre_maximize_position() {
        // x/y say monitor 2's corner, but prev_* is where it'll actually land.
        let g = WindowGeometry {
            x: 1920,
            y: 0,
            prev_x: 2100,
            prev_y: 300,
            maximized: true,
            ..geometry()
        };
        assert!(!is_on_any_monitor(&g, &[monitor()]));
    }

    #[test]
    fn round_trips_through_json() {
        let g = WindowGeometry {
            width: 1024,
            height: 768,
            x: 10,
            y: 20,
            prev_x: 30,
            prev_y: 40,
            maximized: true,
            visible: true,
            fullscreen: false,
        };
        let json = serde_json::to_string(&g).expect("serializing geometry can't fail");
        assert_eq!(serde_json::from_str::<WindowGeometry>(&json).expect("round trip"), g);
    }

    #[test]
    fn loads_a_file_written_by_the_old_plugin() {
        // Upgrading users must keep their window position, so the plugin's
        // `decorated` field has to be ignored rather than rejected.
        let plugin_json = r#"{
            "width": 2474, "height": 1218,
            "x": 2590, "y": 372,
            "prev_x": 2126, "prev_y": 372,
            "maximized": false, "visible": true,
            "decorated": true, "fullscreen": false
        }"#;
        let g: WindowGeometry = serde_json::from_str(plugin_json).expect("plugin file must load");
        assert_eq!((g.width, g.height), (2474, 1218));
        assert_eq!((g.x, g.y), (2590, 372));
        assert_eq!((g.prev_x, g.prev_y), (2126, 372));
    }

    #[test]
    fn missing_fields_fall_back_to_defaults() {
        let g: WindowGeometry =
            serde_json::from_str(r#"{"width": 900, "height": 700}"#).expect("partial file must load");
        assert_eq!((g.width, g.height), (900, 700));
        assert!(g.visible);
        assert!(!g.maximized);
    }
}
