//! The tag row's portable half: which circles, where they sit, what the pointer over one
//! means, and how to find their seven items in a tracking `NSMenu`.
//!
//! No AppKit, so every rule here runs in the Linux CI lane. `view.rs` draws what this
//! decides, and `loan.rs` finds the items and installs the view.
//!
//! Every length is in points, in the row view's own coordinates: origin top-left, since the
//! view is flipped. The geometry is Finder's, measured off @2x screenshots of its tag row
//! (dark mode, macOS 27.0, 2026-09-15).

/// An sRGB color, one byte per channel.
pub type Rgb = [u8; 3];

/// A point in the row view's coordinates.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Point {
    pub x: f64,
    pub y: f64,
}

/// One Finder tag color, as the row and the fallback bitmaps draw it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TagSwatch {
    /// Finder's color index (1 gray … 7 orange): what `tag-color:<index>` IDs and the tag
    /// xattr carry.
    pub color: u8,
    /// The catalog key of the color's translated name.
    pub name_key: &'static str,
    /// `--color-tag-*` in light mode (`apps/desktop/src/app.css`).
    pub light: Rgb,
    /// `--color-tag-*` in dark mode.
    pub dark: Rgb,
}

/// How many circles the row holds.
pub const SWATCH_COUNT: usize = 7;

/// The seven colors in Finder's row order, red to gray.
///
/// Cmdr's own tag tokens rather than Finder's palette, so a circle here matches the dot
/// the same tag draws in the file list. A hand-kept mirror of both `--color-tag-*` blocks in
/// `apps/desktop/src/app.css`, which points back here: change the two together.
pub const SWATCHES: [TagSwatch; SWATCH_COUNT] = [
    TagSwatch {
        color: 6,
        name_key: "menu.tag.red",
        light: [0xdf, 0x5b, 0x56],
        dark: [0xef, 0x6f, 0x6a],
    },
    TagSwatch {
        color: 7,
        name_key: "menu.tag.orange",
        light: [0xe0, 0x8a, 0x3c],
        dark: [0xf0, 0x9a, 0x4c],
    },
    TagSwatch {
        color: 5,
        name_key: "menu.tag.yellow",
        light: [0xe6, 0xb9, 0x3f],
        dark: [0xf0, 0xc5, 0x4e],
    },
    TagSwatch {
        color: 2,
        name_key: "menu.tag.green",
        light: [0x5a, 0xa8, 0x4f],
        dark: [0x6f, 0xc4, 0x63],
    },
    TagSwatch {
        color: 4,
        name_key: "menu.tag.blue",
        light: [0x4b, 0x8f, 0xe0],
        dark: [0x5e, 0xa0, 0xee],
    },
    TagSwatch {
        color: 3,
        name_key: "menu.tag.purple",
        light: [0xa8, 0x6f, 0xd0],
        dark: [0xbd, 0x86, 0xe0],
    },
    TagSwatch {
        color: 1,
        name_key: "menu.tag.gray",
        light: [0x9a, 0x9a, 0x9e],
        dark: [0xa8, 0xa8, 0xad],
    },
];

/// How much of the fill a circle's ring keeps: the ring is the fill darkened to this share.
///
/// Finder's ring measures about 80% of its fill's luminance.
pub const RING_SHADE: f64 = 0.78;

/// The ring color for a fill.
pub fn ring_rgb(fill: Rgb) -> Rgb {
    // `RING_SHADE` is below 1, so a shaded channel stays inside 0..=255.
    fill.map(|channel| (f64::from(channel) * RING_SHADE).round() as u8)
}

/// A circle's diameter at rest.
pub const SWATCH_DIAMETER: f64 = 14.0;

/// A hovered circle's diameter. Finder's grows from 14 to 20.
pub const HOVERED_DIAMETER: f64 = 20.0;

/// Center-to-center distance between neighbouring circles.
pub const SWATCH_PITCH: f64 = 24.0;

/// The ring's width.
pub const RING_WIDTH: f64 = 1.0;

/// The white glyph's stroke width.
pub const GLYPH_STROKE: f64 = 1.5;

/// How far the first circle's left edge sits right of the title column: Finder's circles
/// start a hair right of where its label's text starts.
pub const FIRST_SWATCH_OFFSET: f64 = 2.0;

/// The circles' center line, from the top of the row. Leaves room for a hovered circle, and
/// with the 11 pt separator above puts the circles 17.5 pt under its line, as in Finder.
pub const SWATCH_CENTER_Y: f64 = 12.0;

/// The label's baseline, from the top of the row: 30 pt under the circles' centers, as in
/// Finder.
pub const LABEL_BASELINE_Y: f64 = 41.5;

/// The row's height. With the 11 pt separator below, its line lands 42.5 pt under the
/// circles' centers, as in Finder.
pub const ROW_HEIGHT: f64 = 49.0;

// A hovered circle still clears its neighbours, and doesn't clip at the top of the row.
const _: () = assert!(HOVERED_DIAMETER / 2.0 + SWATCH_DIAMETER / 2.0 < SWATCH_PITCH);
const _: () = assert!(SWATCH_CENTER_Y >= HOVERED_DIAMETER / 2.0);

/// Room right of the widest content.
pub const TRAILING_PADDING: f64 = 14.0;

/// Where a menu's titles start, from its left edge, when no visible item has an image.
///
/// ⚠️ An estimate, to be tuned by eye in a real menu. AppKit exposes no title inset for menu
/// items, and a view in a menu item spans the whole row, so the row has to know this to line
/// its label and first circle up with the titles above and below it, the way Finder's does.
/// What IS measured: a one-item menu titled `M` is 44 pt wide, and each further `M` adds
/// 11.2 pt, so the two insets together are about 33 pt (macOS 27.0, `NSMenu.size` offscreen,
/// 2026-09-16).
pub const TITLE_COLUMN_X: f64 = 14.0;

/// How much further right titles start when some visible item has an image: an 18 pt image
/// widens a one-item menu by 24 pt (macOS 27.0, `NSMenu.size` offscreen, 2026-09-16).
pub const IMAGE_COLUMN_WIDTH: f64 = 24.0;

/// What the title column needs to know about one top-level item of the menu.
#[derive(Debug, Clone, Copy)]
pub struct ColumnItem {
    /// One of the seven tag items: the one carrying the row, or one of the six it hides.
    pub in_tag_run: bool,
    pub hidden: bool,
    pub has_image: bool,
}

/// Whether AppKit moves this menu's titles right to make room for images: some visible
/// item outside the tag run carries one.
///
/// The run doesn't count. Its first item's bitmap is cleared when the row lands, and the
/// other six are hidden, which AppKit already ignores; excluding them outright keeps the
/// answer right however the install is ordered.
pub fn menu_shows_images(items: impl IntoIterator<Item = ColumnItem>) -> bool {
    items
        .into_iter()
        .any(|item| !item.in_tag_run && !item.hidden && item.has_image)
}

/// Where this menu's titles start, which is where the row's label starts too.
pub fn title_column_x(menu_shows_images: bool) -> f64 {
    if menu_shows_images {
        TITLE_COLUMN_X + IMAGE_COLUMN_WIDTH
    } else {
        TITLE_COLUMN_X
    }
}

/// The center of circle `index`.
pub fn swatch_center(index: usize, title_x: f64) -> Point {
    Point {
        x: title_x + FIRST_SWATCH_OFFSET + SWATCH_DIAMETER / 2.0 + index as f64 * SWATCH_PITCH,
        y: SWATCH_CENTER_Y,
    }
}

/// A circle's diameter, grown while the pointer is over it.
pub fn swatch_diameter(hovered: bool) -> f64 {
    if hovered { HOVERED_DIAMETER } else { SWATCH_DIAMETER }
}

/// Which circle a point is over, if any.
///
/// Each circle owns a pitch-wide slot, so the gap between two circles belongs to one of
/// them and the label doesn't flicker back to `Tags` while the pointer crosses the row. The
/// slots stop above the label line, which isn't clickable.
pub fn swatch_at(point: Point, title_x: f64) -> Option<usize> {
    let half_pitch = SWATCH_PITCH / 2.0;
    if !(0.0..SWATCH_CENTER_Y + half_pitch).contains(&point.y) {
        return None;
    }
    (0..SWATCH_COUNT).find(|&index| {
        let center = swatch_center(index, title_x).x;
        (center - half_pitch..center + half_pitch).contains(&point.x)
    })
}

/// How wide the row needs to be to show every circle, hovered, and its widest label.
pub fn row_width(title_x: f64, widest_label: f64) -> f64 {
    let last_circle_right = swatch_center(SWATCH_COUNT - 1, title_x).x + HOVERED_DIAMETER / 2.0;
    title_x + widest_label.max(last_circle_right - title_x) + TRAILING_PADDING
}

/// The white mark inside a circle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Glyph {
    /// A plain circle: the color isn't applied and the pointer isn't on it.
    None,
    /// Applied, at rest.
    Check,
    /// The click would add the color.
    Plus,
    /// The click would remove the color.
    Cross,
}

/// The glyph a circle shows: at rest, whether the color is applied; hovered, what a click
/// would do.
pub fn glyph_for(applied: bool, hovered: bool) -> Glyph {
    match (applied, hovered) {
        (false, false) => Glyph::None,
        (true, false) => Glyph::Check,
        (false, true) => Glyph::Plus,
        (true, true) => Glyph::Cross,
    }
}

/// The glyph's strokes, each a polyline, for a circle at `center` with `diameter`.
///
/// Shapes are fractions of the diameter, so a glyph scales with its circle: Finder's check
/// sits in a 14 pt circle, its plus and cross in the 20 pt hovered one.
pub fn glyph_strokes(glyph: Glyph, center: Point, diameter: f64) -> Vec<Vec<Point>> {
    let at = |dx: f64, dy: f64| Point {
        x: center.x + dx * diameter,
        y: center.y + dy * diameter,
    };
    match glyph {
        Glyph::None => Vec::new(),
        Glyph::Check => vec![vec![at(-0.19, 0.07), at(-0.05, 0.23), at(0.22, -0.18)]],
        Glyph::Plus => {
            const ARM: f64 = 0.17;
            vec![vec![at(-ARM, 0.0), at(ARM, 0.0)], vec![at(0.0, -ARM), at(0.0, ARM)]]
        }
        Glyph::Cross => {
            const ARM: f64 = 0.15;
            vec![vec![at(-ARM, -ARM), at(ARM, ARM)], vec![at(-ARM, ARM), at(ARM, -ARM)]]
        }
    }
}

/// The label line's catalog key while nothing is hovered.
pub const ROW_LABEL_KEY: &str = "menu.tag.rowLabel";

/// The label line's catalog key while a circle is hovered: what a click on it does.
///
/// Exactly the toggle `file_system::tags_io::toggle_color` performs: "applied" means every
/// right-clicked row carries the color, and then the click removes it; otherwise it adds it.
pub fn hover_label_key(applied: bool) -> &'static str {
    if applied {
        "menu.tag.removeNamed"
    } else {
        "menu.tag.addNamed"
    }
}

/// Where the seven tag items start among a menu's top-level item titles.
///
/// `armed` holds the seven live titles in row order. The match has to be the whole run,
/// contiguous and in order, because a single title can collide: the header line carries
/// the bare filename, so a folder named `Red` has a `Red` item above the real one.
pub fn find_tag_run<S: AsRef<str>>(titles: &[S], armed: &[String]) -> Option<usize> {
    if armed.len() != SWATCH_COUNT {
        return None;
    }
    titles
        .windows(SWATCH_COUNT)
        .position(|window| window.iter().zip(armed).all(|(title, armed)| title.as_ref() == armed))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn point(x: f64, y: f64) -> Point {
        Point { x, y }
    }

    #[test]
    fn swatches_run_in_finders_order() {
        let colors: Vec<u8> = SWATCHES.iter().map(|swatch| swatch.color).collect();
        assert_eq!(colors, [6, 7, 5, 2, 4, 3, 1]);
    }

    /// Each Finder color appears exactly once, and colorless (0) never: the IDs, the cached
    /// bitmaps, and `applied_tag_colors` are all indexed by it.
    #[test]
    fn every_finder_color_has_exactly_one_swatch() {
        let mut colors: Vec<u8> = SWATCHES.iter().map(|swatch| swatch.color).collect();
        colors.sort_unstable();
        assert_eq!(colors, [1, 2, 3, 4, 5, 6, 7]);
    }

    #[test]
    fn the_ring_is_darker_than_its_fill() {
        for swatch in SWATCHES {
            for fill in [swatch.light, swatch.dark] {
                let ring = ring_rgb(fill);
                assert!(
                    ring.iter().zip(fill).all(|(ring, fill)| *ring < fill),
                    "{fill:?} → {ring:?}"
                );
            }
        }
        assert_eq!(ring_rgb([100, 200, 255]), [78, 156, 199]);
    }

    #[test]
    fn titles_start_further_right_when_the_menu_shows_images() {
        assert_eq!(title_column_x(false), TITLE_COLUMN_X);
        assert_eq!(title_column_x(true), TITLE_COLUMN_X + IMAGE_COLUMN_WIDTH);
    }

    fn column_item(in_tag_run: bool, hidden: bool, has_image: bool) -> ColumnItem {
        ColumnItem {
            in_tag_run,
            hidden,
            has_image,
        }
    }

    /// A Drive row's SF Symbol lands from another observer on the same notification, maybe
    /// after the row installed; asking at draw time sees it either way.
    #[test]
    fn a_visible_image_outside_the_run_moves_the_titles() {
        let plain = column_item(false, false, false);
        let drive_row = column_item(false, false, true);
        assert!(menu_shows_images([plain, drive_row, plain]));
    }

    #[test]
    fn the_tag_run_and_hidden_items_dont_move_the_titles() {
        let plain = column_item(false, false, false);
        let tag_item_with_bitmap = column_item(true, false, true);
        let hidden_tag_item = column_item(true, true, true);
        let hidden_other = column_item(false, true, true);
        assert!(!menu_shows_images([
            plain,
            tag_item_with_bitmap,
            hidden_tag_item,
            hidden_other
        ]));
        assert!(!menu_shows_images([]));
    }

    #[test]
    fn circles_sit_on_one_line_a_pitch_apart_starting_just_right_of_the_title_column() {
        let first = swatch_center(0, 30.0);
        assert_eq!(first.x - SWATCH_DIAMETER / 2.0, 30.0 + FIRST_SWATCH_OFFSET);
        assert_eq!(first.y, SWATCH_CENTER_Y);
        for index in 1..SWATCH_COUNT {
            let center = swatch_center(index, 30.0);
            assert_eq!(center.x - swatch_center(index - 1, 30.0).x, SWATCH_PITCH);
            assert_eq!(center.y, SWATCH_CENTER_Y);
        }
    }

    /// Finder's hovered circle is 20 pt against 14 at rest. That it still fits the row is a
    /// compile-time assertion beside the constants.
    #[test]
    fn a_hovered_circle_grows() {
        assert_eq!(swatch_diameter(false), SWATCH_DIAMETER);
        assert_eq!(swatch_diameter(true), HOVERED_DIAMETER);
    }

    #[test]
    fn a_point_on_a_circle_hits_that_circle() {
        for index in 0..SWATCH_COUNT {
            let center = swatch_center(index, 30.0);
            assert_eq!(swatch_at(center, 30.0), Some(index));
            let edge = point(
                center.x + HOVERED_DIAMETER / 2.0 - 0.5,
                center.y - HOVERED_DIAMETER / 2.0 + 0.5,
            );
            assert_eq!(swatch_at(edge, 30.0), Some(index), "the corner of a hovered circle");
        }
    }

    /// The gap between two circles belongs to one of them, so the pointer never flickers
    /// between a label and `Tags` while it crosses the row.
    #[test]
    fn the_gap_between_circles_belongs_to_a_neighbour() {
        let left = swatch_center(2, 30.0);
        let gap = point(left.x + SWATCH_PITCH / 2.0 - 0.01, left.y);
        assert_eq!(swatch_at(gap, 30.0), Some(2));
        let gap = point(left.x + SWATCH_PITCH / 2.0 + 0.01, left.y);
        assert_eq!(swatch_at(gap, 30.0), Some(3));
    }

    #[test]
    fn nothing_is_hit_beside_the_row_or_on_the_label_line() {
        let first = swatch_center(0, 30.0);
        let last = swatch_center(SWATCH_COUNT - 1, 30.0);
        assert_eq!(swatch_at(point(first.x - SWATCH_PITCH, first.y), 30.0), None);
        assert_eq!(swatch_at(point(last.x + SWATCH_PITCH, last.y), 30.0), None);
        assert_eq!(swatch_at(point(first.x, LABEL_BASELINE_Y), 30.0), None);
        assert_eq!(swatch_at(point(first.x, LABEL_BASELINE_Y - 8.0), 30.0), None);
        assert_eq!(swatch_at(point(first.x, -1.0), 30.0), None);
    }

    #[test]
    fn the_row_is_wide_enough_for_the_hovered_last_circle_and_the_widest_label() {
        let last = swatch_center(SWATCH_COUNT - 1, 30.0);
        let short_label = row_width(30.0, 20.0);
        assert!(short_label >= last.x + HOVERED_DIAMETER / 2.0 + TRAILING_PADDING);
        let long_label = row_width(30.0, 400.0);
        assert_eq!(long_label, 30.0 + 400.0 + TRAILING_PADDING);
    }

    #[test]
    fn the_glyph_says_what_a_click_would_do() {
        assert_eq!(glyph_for(false, false), Glyph::None);
        assert_eq!(glyph_for(true, false), Glyph::Check);
        assert_eq!(glyph_for(false, true), Glyph::Plus);
        assert_eq!(glyph_for(true, true), Glyph::Cross);
    }

    #[test]
    fn a_plain_circle_has_no_strokes_and_every_mark_stays_inside_its_circle() {
        let center = point(50.0, 12.0);
        assert!(glyph_strokes(Glyph::None, center, SWATCH_DIAMETER).is_empty());
        for (glyph, diameter) in [
            (Glyph::Check, SWATCH_DIAMETER),
            (Glyph::Plus, HOVERED_DIAMETER),
            (Glyph::Cross, HOVERED_DIAMETER),
        ] {
            let strokes = glyph_strokes(glyph, center, diameter);
            assert!(!strokes.is_empty(), "{glyph:?} draws nothing");
            let inner_radius = diameter / 2.0 - RING_WIDTH - GLYPH_STROKE / 2.0;
            for stroke in &strokes {
                assert!(stroke.len() >= 2, "{glyph:?} has a one-point stroke");
                for p in stroke {
                    let distance = ((p.x - center.x).powi(2) + (p.y - center.y).powi(2)).sqrt();
                    assert!(distance < inner_radius, "{glyph:?} pokes out of its circle at {p:?}");
                }
            }
        }
    }

    #[test]
    fn the_hover_label_offers_the_opposite_of_the_current_state() {
        assert_eq!(hover_label_key(false), "menu.tag.addNamed");
        assert_eq!(hover_label_key(true), "menu.tag.removeNamed");
        assert_eq!(ROW_LABEL_KEY, "menu.tag.rowLabel");
    }

    fn armed() -> Vec<String> {
        ["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"]
            .map(String::from)
            .to_vec()
    }

    fn menu_titles(before: &[&str], after: &[&str]) -> Vec<String> {
        before
            .iter()
            .copied()
            .chain(["Red", "Orange", "Yellow", "Green", "Blue", "Purple", "Gray"])
            .chain(after.iter().copied())
            .map(String::from)
            .collect()
    }

    #[test]
    fn the_run_is_found_where_the_seven_items_sit() {
        let titles = menu_titles(&["photo.jpg", "", "Open", "", "Select", ""], &["", "Copy…"]);
        assert_eq!(find_tag_run(&titles, &armed()), Some(6));
    }

    /// The header carries the bare filename, so a folder named `Red` puts a `Red` item
    /// above the real run. A first-title match would install the row on the header.
    #[test]
    fn a_folder_named_like_a_color_doesnt_capture_the_row() {
        let titles = menu_titles(&["Red", "", "Select", ""], &["", "Copy…"]);
        assert_eq!(find_tag_run(&titles, &armed()), Some(4));
        let titles = menu_titles(&["Orange", "Red", "Orange"], &[]);
        assert_eq!(find_tag_run(&titles, &armed()), Some(3));
    }

    #[test]
    fn a_broken_or_partial_run_is_no_run() {
        let broken: Vec<&str> = vec!["Red", "Orange", "Yellow", "", "Green", "Blue", "Purple", "Gray"];
        assert_eq!(find_tag_run(&broken, &armed()), None);
        let partial: Vec<&str> = vec!["Red", "Orange", "Yellow", "Green", "Blue", "Purple"];
        assert_eq!(find_tag_run(&partial, &armed()), None);
        let reordered: Vec<&str> = vec!["Orange", "Red", "Yellow", "Green", "Blue", "Purple", "Gray"];
        assert_eq!(find_tag_run(&reordered, &armed()), None);
        assert_eq!(find_tag_run::<&str>(&[], &armed()), None);
    }

    #[test]
    fn nothing_armed_matches_nothing() {
        let titles = menu_titles(&[], &[]);
        assert_eq!(find_tag_run(&titles, &[]), None);
    }
}
