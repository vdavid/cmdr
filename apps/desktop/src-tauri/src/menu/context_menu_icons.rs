//! Every image on the file context menu (macOS): SF Symbols on Cmdr's own items, each File
//! Provider's logo on its actions, the app icons in "Open with", each service's own icon in
//! "Share", and the tag items' fallback circles.
//!
//! The menu BAR gets its icons at build time (`macos_appkit.rs`'s `MENU_BAR_ICONS`),
//! because Tauri hands out the installed bar's `NSMenu`. A context menu's it does not
//! (muda's `ns_menu()` sits behind Tauri's sealed `ContextMenuBase`), so this reaches
//! the items through `NSMenuDidBeginTrackingNotification`, exactly as
//! `services_context.rs` does and for exactly the same reason. AppKit posts it before
//! the menu is laid out, which is why an image set here still gets its gutter. The
//! submenus ("Open with", "Share") already hang off the root `NSMenu` by then, so they're
//! reached from the root's notification and done before either one opens.
//!
//! ## Why not `IconMenuItem`, which needs no AppKit at all
//!
//! Because the image it sets never reaches `set_menu_item_image`, the one door that opts
//! an item into staying visible on macOS 27, and there's no `NSMenuItem` to opt in
//! afterwards. Linked against the macOS 27 SDK, every `IconMenuItem` image draws nothing.
//! It also can't render a template image (muda hands `NSImage` a PNG and never calls
//! `setTemplate:`), so a monochrome glyph would vanish in one appearance anyway. Clippy
//! refuses `IconMenuItem` crate-wide (`clippy.toml`), so nobody reaches for it by habit.
//!
//! So the menu is built from plain items, and [`image_runs`] says which image each one
//! carries. Images are keyed by item ID and resolved to live titles at arm time, the house
//! rule for crossing into AppKit.
//!
//! ## Why runs, not single titles
//!
//! A title is only a title. Two apps or two share extensions can carry the same name, and
//! the header line carries the bare filename, so a folder named `Mail` would take the Mail
//! service's icon. Each group of items is matched as ONE contiguous run of titles inside
//! the menu that holds it ([`find_title_run`]), which a stray lookalike can't satisfy, and
//! duplicates inside a run pair up by position.

use std::cell::{Cell, RefCell};

use objc2::rc::Retained;
use objc2::{AnyThread, MainThreadMarker};
use objc2_app_kit::{NSImage, NSMenu, NSMenuItem};
use objc2_foundation::{NSCopying, NSData, NSNotification, NSSize};
use tauri::Runtime;
use tauri::menu::{Menu, MenuItem, MenuItemKind, Submenu};

use super::file_context_menu::FileContextInfo;
use super::file_provider_items::file_provider_action_id;
use super::macos_appkit::{
    find_ns_submenu, menu_item_text, observe_menu_tracking, plain_title, set_menu_item_image, sf_symbol_image,
    tracking_menu,
};
use super::open_with::{OPEN_WITH_ID_PREFIX, OPEN_WITH_SUBMENU_ID};
use super::provider_logos::{ProviderLogo, logo_for_provider};
use super::share_submenu::{SHARE_SUBMENU_ID, share_service_id};
use super::tag_row::SWATCHES;
use super::{DRIVE_ASK_GEMINI_ID, DRIVE_COPY_LINK_ID, DRIVE_OPEN_ID, TAG_COLOR_ID_PREFIX};
use crate::file_system::file_provider_actions::ProviderOffer;
use crate::file_system::open_with::AppIcon;
use crate::file_system::open_with::OpenWithChoices;

/// `(menu item ID, SF Symbol name)` for the file context menu.
///
/// Everything here is an ID, never a label: a title is user-facing text that
/// translation moves, and an icon that stops matching disappears without a sound.
/// Items with no entry show no icon, which is the norm: icons mark the actions worth
/// spotting at a glance, not every line.
///
/// The three Drive items are the whole list today. `link` is the same symbol the menu
/// bar's `Copy path` carries, on purpose: the same concept gets the same glyph, which is
/// already how `Copy` shares `document.on.document` across two menus. `sparkles` is what
/// Apple and Google both spell AI with, so `Ask Gemini` reads as one at a glance.
/// Provider actions (`file_provider_items.rs`) get no symbol: their labels are the
/// provider's, and a glyph Cmdr picked would claim to know what each one does. They get
/// their provider's logo instead ([`image_runs`]), which only says whose action it is.
const FILE_CONTEXT_ICONS: &[(&str, &str)] = &[
    (DRIVE_OPEN_ID, "arrow.up.forward.app"),
    (DRIVE_COPY_LINK_ID, "link"),
    (DRIVE_ASK_GEMINI_ID, "sparkles"),
];

/// The side of every non-symbol image except the tag circles, in points: the box the SF
/// Symbols beside them take.
///
/// At the menu's 13 pt font, `link` is 17 × 17, `sparkles` 15 × 17, and
/// `arrow.up.forward.app` 15 × 14 (verified on macOS 26.6, `NSImage.size` from a Swift
/// probe, 2026-09-12). A share service's own image is 16 × 16 too (macOS 26.6.2,
/// `image.size` on all nine services offered for a text file, 2026-09-09).
const IMAGE_SIDE_POINTS: f64 = 16.0;

/// What one item shows. No AppKit in here, so which item gets what stays unit-testable;
/// [`render`] turns it into an `NSImage` at arm time.
#[derive(Clone)]
enum ItemImage {
    /// An SF Symbol, which AppKit tints for light, dark, and highlight.
    Symbol(&'static str),
    /// A provider's colored logo, from its SVG.
    Logo(&'static ProviderLogo),
    /// An app's own icon, read from its bundle off the main thread (`load_app_icon`).
    AppIcon(AppIcon),
    /// A Finder tag's circle, with the check composited in when every row carries it.
    TagCircle { color: u8, applied: bool },
    /// The icon of the service at this index in the live Share offer, macOS's own image.
    ShareService(usize),
}

/// Which menu a run's items sit in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum RunHost {
    /// The context menu itself.
    Menu,
    /// The submenu with this ID, directly inside the context menu.
    Submenu(&'static str),
}

/// A group of items that sit next to each other, matched as one.
///
/// Items with no image still belong to the run (an app whose bundle has no readable icon),
/// because contiguity is what the match leans on.
#[derive(Clone)]
struct ImageRun {
    host: RunHost,
    /// `(menu item ID, what it shows)`, in menu order.
    items: Vec<(String, Option<ItemImage>)>,
}

/// Every image the file context menu for `info` can carry, grouped into runs.
///
/// Which of these the menu actually built is the arm step's question: the Drive items exist
/// only for a Drive row, "Open with" only for a file, "Share" only when macOS offers a
/// service, and an offer's lines only when File Provider vouched for them. A run whose items
/// the menu doesn't have is dropped whole.
///
/// A fact still pending adds no run here: its items get their images when it lands
/// (`context_menu_live.rs` calls the `land_*` functions below). The tag circles are the one
/// run that's always there, unchecked until the tag reads answer.
fn image_runs(info: &FileContextInfo) -> Vec<ImageRun> {
    let symbols = FILE_CONTEXT_ICONS.iter().map(|&(id, symbol)| ImageRun {
        host: RunHost::Menu,
        items: vec![(id.to_string(), Some(ItemImage::Symbol(symbol)))],
    });
    let logos = info
        .file_provider_offer
        .ready()
        .and_then(Option::as_ref)
        .and_then(logo_run);
    let tag_circles = tag_run(info.applied_tag_colors.ready().unwrap_or(&[false; 8]));
    let open_with = info.open_with.ready().and_then(open_with_run);
    let share = info
        .share_services
        .ready()
        .and_then(|services| share_run(services.len()));
    symbols
        .chain(std::iter::once(tag_circles))
        .chain(logos)
        .chain(open_with)
        .chain(share)
        .collect()
}

/// The provider's logo on each of its lines, when Cmdr knows the provider.
fn logo_run(offer: &ProviderOffer) -> Option<ImageRun> {
    let logo = logo_for_provider(&offer.provider_id)?;
    (!offer.actions.is_empty()).then(|| ImageRun {
        host: RunHost::Menu,
        items: (0..offer.actions.len())
            .map(|index| (file_provider_action_id(index), Some(ItemImage::Logo(logo))))
            .collect(),
    })
}

/// The seven tag circles, checked where every row carries the color.
fn tag_run(applied_tag_colors: &[bool; 8]) -> ImageRun {
    ImageRun {
        host: RunHost::Menu,
        items: SWATCHES
            .iter()
            .map(|swatch| {
                let applied = applied_tag_colors[usize::from(swatch.color)];
                let circle = ItemImage::TagCircle {
                    color: swatch.color,
                    applied,
                };
                (format!("{TAG_COLOR_ID_PREFIX}{}", swatch.color), Some(circle))
            })
            .collect(),
    }
}

/// Each candidate app's own icon in "Open with".
fn open_with_run(choices: &OpenWithChoices) -> Option<ImageRun> {
    (!choices.candidates.is_empty()).then(|| ImageRun {
        host: RunHost::Submenu(OPEN_WITH_SUBMENU_ID),
        items: choices
            .candidates
            .iter()
            .map(|app| {
                let id = format!("{OPEN_WITH_ID_PREFIX}{}", app.bundle_id);
                (id, app.icon.clone().map(ItemImage::AppIcon))
            })
            .collect(),
    })
}

/// Each offered service's own icon in `Share`.
fn share_run(count: usize) -> Option<ImageRun> {
    (count > 0).then(|| ImageRun {
        host: RunHost::Submenu(SHARE_SUBMENU_ID),
        items: (0..count)
            .map(|index| (share_service_id(index), Some(ItemImage::ShareService(index))))
            .collect(),
    })
}

/// Where `run` starts as one contiguous stretch of `titles`, or `None` when it doesn't.
///
/// Titles compare without the display-accelerator run `display_accelerators.rs` may have
/// put after a TAB. An empty run, or one holding an empty title, matches nothing: a
/// separator's title is empty, and a match on it would be a match on nothing.
pub(super) fn find_title_run<S: AsRef<str>>(titles: &[S], run: &[String]) -> Option<usize> {
    if run.is_empty() || run.iter().any(String::is_empty) {
        return None;
    }
    titles.windows(run.len()).position(|window| {
        window
            .iter()
            .zip(run)
            .all(|(title, want)| plain_title(title.as_ref()) == want)
    })
}

/// A run ready for the tracking menu: live titles, and the images already made.
#[derive(Clone)]
struct ArmedRun {
    /// The live title of the submenu the run sits in, or `None` for the context menu itself.
    submenu_title: Option<String>,
    titles: Vec<String>,
    images: Vec<Option<Retained<NSImage>>>,
}

/// Images armed for the next context menu to open.
///
/// A title rather than an ID, because AppKit has never heard of a Tauri menu ID: the
/// title is read off the live Tauri item at arm time, which is the house rule for
/// crossing that boundary and what keeps the match working once labels are translated.
pub struct IconLoan(MainThreadMarker);

impl Drop for IconLoan {
    fn drop(&mut self) {
        ARMED.with(|slot| slot.borrow_mut().clear());
    }
}

thread_local! {
    /// The runs for the menu currently going up. Main-thread-only by construction: every
    /// reader runs from the menu thread, and it holds `NSImage`s.
    static ARMED: RefCell<Vec<ArmedRun>> = const { RefCell::new(Vec::new()) };
    /// Whether the tracking observer is registered. Registering it lazily keeps the
    /// cost with the first right-click instead of every launch.
    static OBSERVING: Cell<bool> = const { Cell::new(false) };
}

/// Arms every image the file context menu `menu` carries, per [`image_runs`]. The menu must
/// be about to `popup()`.
///
/// ❗ Hold the returned guard until `popup()` returns, the way `ServicesLoan` is held:
/// `popup()` runs AppKit's tracking loop, so the menu only starts tracking inside it.
/// Dropping the guard early would disarm before the images ever landed.
///
/// Answers `None` when there is nothing to do: off the main thread, or the menu built none
/// of the items that carry an image.
pub fn lend_context_menu_icons<R: Runtime>(menu: &Menu<R>, info: &FileContextInfo) -> Option<IconLoan> {
    let Some(mtm) = MainThreadMarker::new() else {
        log::warn!(target: "menu", "Not on the main thread; the context menu's items show no images");
        return None;
    };
    let armed: Vec<ArmedRun> = image_runs(info)
        .into_iter()
        .filter_map(|run| arm(mtm, menu, run))
        .collect();
    if armed.is_empty() {
        return None;
    }
    ensure_observing(mtm);
    ARMED.with(|slot| *slot.borrow_mut() = armed);
    Some(IconLoan(mtm))
}

/// Resolves a run's IDs to the titles AppKit will show, and makes its images.
///
/// `None` when the menu didn't build the run, or built only part of it: a partial run can't
/// be matched as one, and guessing which part is there would be the single-title match this
/// replaced.
fn arm<R: Runtime>(mtm: MainThreadMarker, menu: &Menu<R>, run: ImageRun) -> Option<ArmedRun> {
    let (submenu_title, titles) = match run.host {
        RunHost::Menu => (None, live_titles(&run, |id| menu.get(id))?),
        RunHost::Submenu(submenu_id) => {
            let submenu = menu.get(submenu_id)?.as_submenu()?.clone();
            let title = submenu.text().ok()?;
            (Some(title), live_titles(&run, |id| submenu.get(id))?)
        }
    };
    Some(armed(mtm, &run, submenu_title, titles))
}

/// A run with its live titles, its images made.
fn armed(mtm: MainThreadMarker, run: &ImageRun, submenu_title: Option<String>, titles: Vec<String>) -> ArmedRun {
    let images = run
        .items
        .iter()
        .map(|(_, image)| image.as_ref().and_then(|image| render(mtm, image)))
        .collect();
    ArmedRun {
        submenu_title,
        titles,
        images,
    }
}

// The `land_*` functions put a LATE fact's images on a menu that's already open
// (`context_menu_live.rs`). ❗ They read titles off the item and submenu handles they're
// given, ❌ never off the context `Menu`, which muda holds borrowed until `popup()` returns.

/// The app icons of a late "Open with" list, on the submenu `fill_open_with_submenu` filled.
pub(super) fn land_open_with_icons<R: Runtime>(
    mtm: MainThreadMarker,
    root: &NSMenu,
    submenu: &Submenu<R>,
    choices: &OpenWithChoices,
) {
    if let Some(run) = open_with_run(choices) {
        land_in_submenu(mtm, root, submenu, &run);
    }
}

/// The service icons of a late `Share` offer. The offer must be armed first
/// (`share::arm_offer`), since that's where each icon comes from.
pub(super) fn land_share_icons<R: Runtime>(mtm: MainThreadMarker, root: &NSMenu, submenu: &Submenu<R>, count: usize) {
    if let Some(run) = share_run(count) {
        land_in_submenu(mtm, root, submenu, &run);
    }
}

/// The provider's logo on each of `items`, the offer's lines, once they carry its labels.
pub(super) fn land_provider_logos<R: Runtime>(
    mtm: MainThreadMarker,
    root: &NSMenu,
    items: &[MenuItem<R>],
    offer: &ProviderOffer,
) {
    if let Some(mut run) = logo_run(offer) {
        run.items.truncate(items.len());
        land_on_items(mtm, root, items, &run);
    }
}

/// The tag circles once the tag reads answered, checked where every row carries the color.
/// Shows only where the tag row didn't install; the row draws its own.
pub(super) fn land_tag_circles<R: Runtime>(
    mtm: MainThreadMarker,
    root: &NSMenu,
    items: &[MenuItem<R>],
    applied_tag_colors: &[bool; 8],
) {
    land_on_items(mtm, root, items, &tag_run(applied_tag_colors));
}

fn land_in_submenu<R: Runtime>(mtm: MainThreadMarker, root: &NSMenu, submenu: &Submenu<R>, run: &ImageRun) {
    let Some(title) = submenu.text().ok() else {
        return;
    };
    let Some(titles) = live_titles(run, |id| submenu.get(id)) else {
        return;
    };
    let Some(ns_submenu) = find_ns_submenu(root, &title) else {
        return;
    };
    apply(&ns_submenu, &armed(mtm, run, Some(title), titles));
}

fn land_on_items<R: Runtime>(mtm: MainThreadMarker, root: &NSMenu, items: &[MenuItem<R>], run: &ImageRun) {
    let Some(titles) = items.iter().map(|item| item.text().ok()).collect::<Option<Vec<_>>>() else {
        return;
    };
    if titles.len() != run.items.len() {
        return;
    }
    apply(root, &armed(mtm, run, None, titles));
}

/// The live title of every item in `run`, or `None` if any of them isn't built.
fn live_titles<R: Runtime>(run: &ImageRun, get: impl Fn(&str) -> Option<MenuItemKind<R>>) -> Option<Vec<String>> {
    run.items
        .iter()
        .map(|(id, _)| menu_item_text(&get(id.as_str())?))
        .collect()
}

/// Registers the tracking observer, once per process.
fn ensure_observing(mtm: MainThreadMarker) {
    if OBSERVING.replace(true) {
        return;
    }
    observe_menu_tracking(mtm, "put images on the right-click menu", |mtm, note| {
        on_menu_did_begin_tracking(mtm, note);
    });
}

/// Puts the armed images on the tracking menu's items.
///
/// Fires for every menu the app tracks (the menu bar included), so it does nothing
/// unless images are armed. It acts on a ROOT menu only: the runs in "Open with" and
/// "Share" are reached through the root, and a submenu posting its own notification when it
/// opens would otherwise be matched against the root's runs.
fn on_menu_did_begin_tracking(mtm: MainThreadMarker, notification: &NSNotification) {
    let armed = ARMED.with(|slot| slot.borrow().clone());
    if armed.is_empty() {
        return;
    }
    let Some(menu) = tracking_menu(mtm, notification) else {
        return;
    };
    // SAFETY: `supermenu` is unsafe only because the reference is unretained; it is only
    // tested for presence here, inside this synchronous main-thread call.
    if unsafe { menu.supermenu() }.is_some() {
        return;
    }
    for run in &armed {
        match &run.submenu_title {
            None => apply(&menu, run),
            Some(title) => {
                if let Some(submenu) = find_ns_submenu(&menu, title) {
                    apply(&submenu, run);
                }
            }
        }
    }
}

/// Sets a run's images on its items in `menu`, if `menu` holds the whole run.
///
/// A title that isn't there costs the run its images and nothing else, which is the same
/// bargain the menu bar's pass makes.
fn apply(menu: &NSMenu, run: &ArmedRun) {
    let items: Vec<Retained<NSMenuItem>> = (0..menu.numberOfItems())
        .filter_map(|index| menu.itemAtIndex(index))
        .collect();
    let titles: Vec<String> = items.iter().map(|item| item.title().to_string()).collect();
    let Some(start) = find_title_run(&titles, &run.titles) else {
        log::debug!(target: "menu", "No run of {} items titled {:?} here, so they show no images", run.titles.len(), run.titles.first());
        return;
    };
    for (item, image) in items[start..].iter().zip(&run.images) {
        let Some(image) = image else {
            continue;
        };
        // A view draws this item (the tag row), and AppKit would still reserve the image
        // column for its image, pushing every title in the menu right. `tag_row` clears the
        // image when it installs; this keeps it cleared whichever observer runs first.
        if item.view().is_some() {
            continue;
        }
        set_menu_item_image(item, image);
    }
}

/// Makes the `NSImage` an item shows. `None` costs that item its image and nothing else.
fn render(mtm: MainThreadMarker, image: &ItemImage) -> Option<Retained<NSImage>> {
    match image {
        ItemImage::Symbol(symbol) => sf_symbol_image(symbol),
        ItemImage::Logo(logo) => logo_image(logo),
        ItemImage::AppIcon(icon) => {
            let png = crate::icons::rgba_to_png(&icon.rgba, icon.width, icon.height)?;
            data_image(&png, IMAGE_SIDE_POINTS)
        }
        ItemImage::TagCircle { color, applied } => data_image(
            super::tag_icons::tag_circle_png(*color, *applied)?,
            super::tag_icons::SIDE_POINTS,
        ),
        ItemImage::ShareService(index) => {
            // A copy, because sizing it would resize macOS's own image wherever else it's shown.
            let image = crate::file_system::share::offered_image(mtm, *index)?.copy();
            image.setSize(NSSize::new(IMAGE_SIDE_POINTS, IMAGE_SIDE_POINTS));
            Some(image)
        }
    }
}

/// A provider's logo, sized like the SF Symbols beside it.
///
/// No version gate, unlike `sf_symbol_image`: `initWithData:` is as old as `NSImage`, so an
/// OS whose image loader can't read SVG answers nil rather than raising, and nil costs the
/// logo and nothing else. Every logo loads as `_NSSVGImageRep` on macOS 26.6 (Swift probe,
/// 2026-09-12); older releases are unverified.
fn logo_image(logo: &ProviderLogo) -> Option<Retained<NSImage>> {
    let image = data_image(logo.svg, IMAGE_SIDE_POINTS);
    if image.is_none() {
        log::debug!(target: "menu", "NSImage can't read {}'s SVG logo here, so its actions show none", logo.provider);
    }
    image
}

/// An `NSImage` read from encoded bytes (SVG or PNG), sized `side` × `side` points. Every
/// image fed here is square, so sizing it to a square can't skew one.
fn data_image(bytes: &[u8], side: f64) -> Option<Retained<NSImage>> {
    let data = NSData::with_bytes(bytes);
    let image = NSImage::initWithData(NSImage::alloc(), &data)?;
    image.setSize(NSSize::new(side, side));
    Some(image)
}

#[cfg(test)]
#[path = "context_menu_icons_test.rs"]
mod context_menu_icons_test;
