//! The row itself: an `NSView` that draws seven circles and a label, follows the pointer,
//! fires the circle's own menu item on a click, and gives VoiceOver one element per circle.
//!
//! Views in menu items get mouse events while the menu tracks, and no key events (Apple's
//! "Views in Menu Items" guide), which is why the row has no keyboard path. Finder's row
//! has none either.

use std::cell::{Cell, RefCell};

use objc2::rc::{Retained, Weak};
use objc2::runtime::AnyObject;
use objc2::{AnyThread, DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_app_kit::{
    NSAccessibility, NSAccessibilityButton, NSAccessibilityCheckBox, NSAccessibilityCheckBoxRole,
    NSAccessibilityElement, NSAccessibilityElementProtocol, NSAccessibilityGroupRole, NSAppearance,
    NSAppearanceCustomization, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSEvent, NSFont, NSFontAttributeName,
    NSForegroundColorAttributeName, NSLineCapStyle, NSLineJoinStyle, NSMenuItem, NSResponder, NSStringDrawing,
    NSTrackingArea, NSTrackingAreaOptions, NSView,
};
use objc2_foundation::{
    NSArray, NSDictionary, NSNumber, NSObject, NSObjectProtocol, NSPoint, NSRect, NSSize, NSString,
};

use super::model::{
    ColumnItem, GLYPH_STROKE, LABEL_BASELINE_Y, Point, RING_WIDTH, ROW_HEIGHT, Rgb, SWATCH_DIAMETER, glyph_for,
    glyph_strokes, menu_shows_images, ring_rgb, row_width, swatch_at, swatch_center, swatch_diameter, title_column_x,
};

/// What one circle shows and says, resolved when the menu was armed.
#[derive(Debug, Clone)]
pub(super) struct SwatchContent {
    /// The color's translated name: what VoiceOver reads for the circle.
    pub name: String,
    /// What the label line says while this circle is hovered and the color isn't applied
    /// (`Add "Green"`).
    pub add_label: String,
    /// The same while it is (`Remove "Green"`). Both are resolved up front, since the
    /// applied flags can land after the row is up ([`TagRowView::set_applied`]).
    pub remove_label: String,
    /// Whether every right-clicked row already carries this color.
    pub applied: bool,
    pub light: Rgb,
    pub dark: Rgb,
}

impl SwatchContent {
    /// What the label line says while this circle is hovered.
    fn hover_label(&self) -> &str {
        if self.applied {
            &self.remove_label
        } else {
            &self.add_label
        }
    }
}

/// Everything the row draws, in row order.
#[derive(Debug, Clone)]
pub(super) struct RowContent {
    pub swatches: Vec<SwatchContent>,
    /// What the label line says while no circle is hovered (`Tags`).
    pub idle_label: String,
}

pub(super) struct RowIvars {
    /// A cell because the applied flags can arrive after the row is up.
    content: RefCell<RowContent>,
    hovered: Cell<Option<usize>>,
    /// The seven menu items in row order, WEAK: each item retains its view, so a strong
    /// reference back would be a cycle. Emptied by [`TagRowView::disarm`], after which a
    /// click does nothing.
    items: RefCell<Vec<Weak<NSMenuItem>>>,
    /// One accessibility element per circle. The row owns them; each points back weakly.
    elements: RefCell<Vec<Retained<TagSwatchElement>>>,
}

define_class!(
    /// The tag row installed on the first tag item of the file context menu.
    #[unsafe(super(NSView, NSResponder, NSObject))]
    #[name = "CmdrTagRowView"]
    #[thread_kind = MainThreadOnly]
    #[ivars = RowIvars]
    pub(super) struct TagRowView;

    unsafe impl NSObjectProtocol for TagRowView {}

    impl TagRowView {
        /// Top-left origin, so the layout reads top to bottom the way `model.rs` writes it.
        #[unsafe(method(isFlipped))]
        fn is_flipped(&self) -> bool {
            true
        }

        #[unsafe(method(drawRect:))]
        fn draw_rect(&self, _dirty: NSRect) {
            self.draw();
        }

        #[unsafe(method(mouseEntered:))]
        fn mouse_entered(&self, event: &NSEvent) {
            self.follow_pointer(event);
        }

        #[unsafe(method(mouseMoved:))]
        fn mouse_moved(&self, event: &NSEvent) {
            self.follow_pointer(event);
        }

        /// A press-drag-release gesture through the menu moves the pointer as drags.
        #[unsafe(method(mouseDragged:))]
        fn mouse_dragged(&self, event: &NSEvent) {
            self.follow_pointer(event);
        }

        #[unsafe(method(mouseExited:))]
        fn mouse_exited(&self, _event: &NSEvent) {
            self.set_hovered(None);
        }

        /// Kept here rather than passed up the responder chain: the click is the row's.
        #[unsafe(method(mouseDown:))]
        fn mouse_down(&self, event: &NSEvent) {
            self.follow_pointer(event);
        }

        #[unsafe(method(mouseUp:))]
        fn mouse_up(&self, event: &NSEvent) {
            if let Some(index) = swatch_at(self.local_point(event), self.title_x()) {
                self.press(index);
            }
        }
    }
);

impl TagRowView {
    /// A row for `items`, the seven tag items in row order.
    pub(super) fn new(mtm: MainThreadMarker, content: RowContent, items: &[Retained<NSMenuItem>]) -> Retained<Self> {
        let font = NSFont::menuFontOfSize(0.0);
        let attributes = label_attributes(&font);
        // Both wordings of every circle: the applied flags may flip after the row is up,
        // and a row that then grew would need the menu laid out again.
        let widest_label = content
            .swatches
            .iter()
            .flat_map(|swatch| [swatch.add_label.as_str(), swatch.remove_label.as_str()])
            .chain([content.idle_label.as_str()])
            .map(|label| label_size(label, &attributes).width)
            .fold(0.0, f64::max);
        // Wide enough for the further-right title column, since images can still arrive
        // after the row is built (see `title_x`); the menu stretches the row to its own width.
        let frame = NSRect::new(
            NSPoint::new(0.0, 0.0),
            NSSize::new(row_width(title_column_x(true), widest_label), ROW_HEIGHT),
        );
        let this = Self::alloc(mtm).set_ivars(RowIvars {
            content: RefCell::new(content),
            hovered: Cell::new(None),
            items: RefCell::new(items.iter().map(Weak::from_retained).collect()),
            elements: RefCell::new(Vec::new()),
        });
        // SAFETY: `initWithFrame:` is `NSView`'s designated initializer, called once on this
        // fresh allocation, with a plain rectangle.
        let view: Retained<Self> = unsafe { msg_send![super(this), initWithFrame: frame] };
        // The menu sizes the row to its own width; the circles and label stay left-aligned.
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        view.track_the_pointer();
        view.expose_to_accessibility(mtm);
        view
    }

    /// Fires circle `index`'s menu item, exactly as a click on the plain item would, and
    /// closes the menu. Answers whether the item's action was sent.
    ///
    /// ❗ Reads the action and target off the item NOW and sends synchronously. The selector
    /// is muda's business (`fireMenuItemAction:` today, `customAction:` in newer muda), and
    /// on the muda we ship the item's ivar points into a `MenuChild` that is freed once
    /// `show_file_context_menu` returns, so a deferred send would read freed memory.
    pub(super) fn press(&self, index: usize) -> bool {
        let Some(item) = self.ivars().items.borrow().get(index).and_then(Weak::load) else {
            return false;
        };
        let (true, Some(action)) = (item.isEnabled(), item.action()) else {
            return false;
        };
        let target = item.target();
        let app = NSApplication::sharedApplication(self.mtm());
        // SAFETY: the selector and target are the item's own pair, read off it just now, and
        // the sender is the item, which is what AppKit passes when the item itself is clicked.
        // The send runs synchronously inside the menu's tracking loop, while everything the
        // action reads is still alive.
        let sent = unsafe { app.sendAction_to_from(action, target.as_deref(), Some(item.as_ref())) };
        if !sent {
            log::warn!(target: "menu", "The tag row's circle {index} found nobody to send its menu action to");
        }
        // SAFETY: `menu` is unretained only in that it doesn't outlive the item; it's used at
        // once, on the main thread, while the item (held above) keeps it attached.
        if let Some(menu) = unsafe { item.menu() } {
            menu.cancelTracking();
        }
        sent
    }

    /// Lets go of the menu items, so a stale click or accessibility press does nothing.
    pub(super) fn disarm(&self) {
        self.ivars().items.borrow_mut().clear();
    }

    /// Checks the circles whose colors every row carries, `applied` in row order, when the
    /// tag reads answer after the row is up. Redraws, and tells VoiceOver.
    pub(super) fn set_applied(&self, applied: &[bool]) {
        let ivars = self.ivars();
        for (swatch, &now) in ivars.content.borrow_mut().swatches.iter_mut().zip(applied) {
            swatch.applied = now;
        }
        for (element, &now) in ivars.elements.borrow().iter().zip(applied) {
            element.set_checked(now);
        }
        self.setNeedsDisplay(true);
    }

    /// Marks circle `index` as the one under the pointer, or none, and redraws on a change.
    pub(super) fn set_hovered(&self, hovered: Option<usize>) {
        if self.ivars().hovered.replace(hovered) != hovered {
            self.setNeedsDisplay(true);
        }
    }

    fn follow_pointer(&self, event: &NSEvent) {
        self.set_hovered(swatch_at(self.local_point(event), self.title_x()));
    }

    /// Where the menu's titles start right now, which the label and the first circle line
    /// up with.
    ///
    /// ❗ Asked at every use, never cached at install. `context_menu_icons.rs` puts images on
    /// Drive and provider rows from its own observer of the same tracking notification, and
    /// `NSNotificationCenter` promises no order between observers, so an answer taken when the
    /// row landed can miss the images that move every title 24 pt right.
    fn title_x(&self) -> f64 {
        // SAFETY: `menu` is unretained only in that it doesn't outlive the item; it's read at
        // once, on the main thread, while the item (held here) keeps it attached.
        let Some(menu) = self.enclosingMenuItem().and_then(|item| unsafe { item.menu() }) else {
            return title_column_x(false);
        };
        let run: Vec<Retained<NSMenuItem>> = self.ivars().items.borrow().iter().filter_map(Weak::load).collect();
        let items = (0..menu.numberOfItems())
            .filter_map(|index| menu.itemAtIndex(index))
            .map(|item| ColumnItem {
                in_tag_run: run
                    .iter()
                    .any(|tag_item| Retained::as_ptr(tag_item) == Retained::as_ptr(&item)),
                hidden: item.isHidden(),
                has_image: item.image().is_some(),
            });
        title_column_x(menu_shows_images(items))
    }

    fn local_point(&self, event: &NSEvent) -> Point {
        let local = self.convertPoint_fromView(event.locationInWindow(), None);
        Point { x: local.x, y: local.y }
    }

    /// One tracking area over the whole row, following it through resizes.
    fn track_the_pointer(&self) {
        let options = NSTrackingAreaOptions::MouseEnteredAndExited
            | NSTrackingAreaOptions::MouseMoved
            | NSTrackingAreaOptions::ActiveAlways
            | NSTrackingAreaOptions::InVisibleRect;
        // SAFETY: the owner is this view, which also holds the area, so the (unretained)
        // owner outlives it; there's no user info.
        let area = unsafe {
            NSTrackingArea::initWithRect_options_owner_userInfo(
                NSTrackingArea::alloc(),
                self.bounds(),
                options,
                Some(self.as_ref()),
                None,
            )
        };
        self.addTrackingArea(&area);
    }

    /// The row reads as a group named like its idle label, holding one checkbox per circle.
    fn expose_to_accessibility(&self, mtm: MainThreadMarker) {
        let ivars = self.ivars();
        let row = Weak::from(self);
        let content = ivars.content.borrow();
        let elements: Vec<Retained<TagSwatchElement>> = content
            .swatches
            .iter()
            .enumerate()
            .map(|(index, swatch)| TagSwatchElement::new(mtm, self, &row, index, swatch))
            .collect();
        // SAFETY: AppKit's own role constants, immortal statics read through their declared type.
        let group_role = unsafe { NSAccessibilityGroupRole };
        self.setAccessibilityElement(true);
        self.setAccessibilityRole(Some(group_role));
        self.setAccessibilityLabel(Some(&NSString::from_str(&content.idle_label)));
        drop(content);
        let children: Vec<&AnyObject> = elements.iter().map(|element| element.as_ref()).collect();
        // SAFETY: every child is an `NSAccessibilityElement` whose parent is this view, which
        // is what the children array must hold.
        unsafe { self.setAccessibilityChildren(Some(&NSArray::from_slice(&children))) };
        *ivars.elements.borrow_mut() = elements;
        self.place_accessibility_elements(self.title_x());
    }

    /// Puts each circle's accessibility frame where the circle is drawn. Refreshed on every
    /// draw, since the title column can move after the row lands (see `title_x`).
    fn place_accessibility_elements(&self, title_x: f64) {
        let radius = SWATCH_DIAMETER / 2.0;
        for (index, element) in self.ivars().elements.borrow().iter().enumerate() {
            let center = swatch_center(index, title_x);
            element.setAccessibilityFrameInParentSpace(NSRect::new(
                NSPoint::new(center.x - radius, center.y - radius),
                NSSize::new(SWATCH_DIAMETER, SWATCH_DIAMETER),
            ));
        }
    }

    fn draw(&self) {
        let ivars = self.ivars();
        let dark = is_dark(&self.effectiveAppearance());
        let hovered = ivars.hovered.get();
        let title_x = self.title_x();
        self.place_accessibility_elements(title_x);
        let content = ivars.content.borrow();
        for (index, swatch) in content.swatches.iter().enumerate() {
            let is_hovered = hovered == Some(index);
            let center = swatch_center(index, title_x);
            let diameter = swatch_diameter(is_hovered);
            let fill = if dark { swatch.dark } else { swatch.light };
            fill_disc(center, diameter, ring_rgb(fill));
            fill_disc(center, diameter - 2.0 * RING_WIDTH, fill);
            stroke_glyph(&glyph_strokes(glyph_for(swatch.applied, is_hovered), center, diameter));
        }
        let label = hovered
            .and_then(|index| content.swatches.get(index))
            .map_or(content.idle_label.as_str(), SwatchContent::hover_label);
        draw_label(label, title_x);
    }
}

pub(super) struct SwatchElementIvars {
    row: Weak<TagRowView>,
    index: usize,
}

define_class!(
    /// VoiceOver's view of one circle: a checkbox named after the color, checked when the
    /// color is applied, whose press does what a click does.
    #[unsafe(super(NSAccessibilityElement, NSObject))]
    #[name = "CmdrTagSwatchElement"]
    #[thread_kind = MainThreadOnly]
    #[ivars = SwatchElementIvars]
    pub(super) struct TagSwatchElement;

    unsafe impl NSObjectProtocol for TagSwatchElement {}

    unsafe impl NSAccessibilityElementProtocol for TagSwatchElement {}

    unsafe impl NSAccessibilityButton for TagSwatchElement {
        #[unsafe(method(accessibilityPerformPress))]
        fn accessibility_perform_press(&self) -> bool {
            let ivars = self.ivars();
            ivars.row.load().is_some_and(|row| row.press(ivars.index))
        }
    }

    unsafe impl NSAccessibilityCheckBox for TagSwatchElement {}
);

impl TagSwatchElement {
    fn new(
        mtm: MainThreadMarker,
        parent: &TagRowView,
        row: &Weak<TagRowView>,
        index: usize,
        swatch: &SwatchContent,
    ) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(SwatchElementIvars {
            row: row.clone(),
            index,
        });
        // SAFETY: `init` is `NSObject`'s initializer, which `NSAccessibilityElement` keeps as its
        // own; called once on this fresh allocation.
        let element: Retained<Self> = unsafe { msg_send![super(this), init] };
        // SAFETY: AppKit's own role constant, an immortal static read through its declared type.
        let checkbox_role = unsafe { NSAccessibilityCheckBoxRole };
        element.setAccessibilityRole(Some(checkbox_role));
        element.setAccessibilityLabel(Some(&NSString::from_str(&swatch.name)));
        element.set_checked(swatch.applied);
        // SAFETY: the parent is the view that owns this element and lists it as a child.
        unsafe { element.setAccessibilityParent(Some(parent.as_ref())) };
        element
    }

    fn set_checked(&self, checked: bool) {
        let value = NSNumber::new_i32(i32::from(checked));
        // SAFETY: a checkbox's value is an `NSNumber`, 1 when checked and 0 when not.
        unsafe { self.setAccessibilityValue(Some(value.as_ref())) };
    }
}

/// Whether an appearance is a dark one (Dark Aqua or its high-contrast sibling).
///
/// Asked through `bestMatchFromAppearancesWithNames:`, AppKit's own way to fold an
/// appearance into the family it belongs to (macOS 10.14); `performAsCurrentDrawingAppearance:`
/// would need 11.
fn is_dark(appearance: &NSAppearance) -> bool {
    // SAFETY: AppKit's own appearance-name constants, immortal statics read through their
    // declared type.
    let (aqua, dark_aqua) = unsafe { (NSAppearanceNameAqua, NSAppearanceNameDarkAqua) };
    let names = NSArray::from_slice(&[aqua, dark_aqua]);
    appearance
        .bestMatchFromAppearancesWithNames(&names)
        .is_some_and(|name| &*name == dark_aqua)
}

fn color(rgb: Rgb) -> Retained<NSColor> {
    let channel = |byte: u8| f64::from(byte) / 255.0;
    NSColor::colorWithSRGBRed_green_blue_alpha(channel(rgb[0]), channel(rgb[1]), channel(rgb[2]), 1.0)
}

fn fill_disc(center: Point, diameter: f64, rgb: Rgb) {
    let radius = diameter / 2.0;
    let rect = NSRect::new(
        NSPoint::new(center.x - radius, center.y - radius),
        NSSize::new(diameter, diameter),
    );
    color(rgb).setFill();
    NSBezierPath::bezierPathWithOvalInRect(rect).fill();
}

fn stroke_glyph(strokes: &[Vec<Point>]) {
    if strokes.is_empty() {
        return;
    }
    let path = NSBezierPath::bezierPath();
    for stroke in strokes {
        let mut points = stroke.iter().map(|p| NSPoint::new(p.x, p.y));
        if let Some(first) = points.next() {
            path.moveToPoint(first);
            points.for_each(|p| path.lineToPoint(p));
        }
    }
    path.setLineWidth(GLYPH_STROKE);
    path.setLineCapStyle(NSLineCapStyle::Butt);
    path.setLineJoinStyle(NSLineJoinStyle::Round);
    NSColor::whiteColor().setStroke();
    path.stroke();
}

/// The menu font in the secondary label color, which AppKit resolves for the appearance the
/// row is drawing in.
fn label_attributes(font: &NSFont) -> Retained<NSDictionary<NSString, AnyObject>> {
    let color = NSColor::secondaryLabelColor();
    // SAFETY: both are AppKit's own attribute-name constants, immortal statics read through
    // the bindings' declared type.
    let keys: [&NSString; 2] = unsafe { [NSFontAttributeName, NSForegroundColorAttributeName] };
    let values: [&AnyObject; 2] = [font.as_ref(), color.as_ref()];
    NSDictionary::from_slices(&keys, &values)
}

fn label_size(text: &str, attributes: &NSDictionary<NSString, AnyObject>) -> NSSize {
    // SAFETY: the dictionary maps two real attribute keys to the classes they require
    // (`NSFont`, `NSColor`).
    unsafe { NSString::from_str(text).sizeWithAttributes(Some(attributes)) }
}

fn draw_label(text: &str, title_x: f64) {
    let font = NSFont::menuFontOfSize(0.0);
    let attributes = label_attributes(&font);
    // In a flipped view the point is the top of the line, which sits one ascender above
    // the baseline.
    let origin = NSPoint::new(title_x, LABEL_BASELINE_Y - font.ascender());
    // SAFETY: as in `label_size`, the attributes are a font and a color under their own keys.
    unsafe { NSString::from_str(text).drawAtPoint_withAttributes(origin, Some(&attributes)) };
}
