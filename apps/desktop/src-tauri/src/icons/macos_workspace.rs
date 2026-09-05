//! Asks macOS for a file's icon and renders it into an RGBA buffer.
//!
//! `NSWorkspace` hands back an `NSImage`, which is a resolution-independent recipe
//! rather than pixels. Getting pixels out means drawing it: allocate an
//! `NSBitmapImageRep` of the size we want, wrap it in an `NSGraphicsContext`, draw the
//! image into that context, and read the backing bytes.
//!
//! Gotcha/Why: every call builds its own bitmap rep and context instead of reusing one.
//! `fetch_icon_for_path` runs on rayon workers and on the dedicated 8 MB-stack threads
//! (see `CLAUDE.md`), so a shared drawing surface would be two threads compositing into
//! the same buffer. The allocation is a few hundred KB against a Launch Services round
//! trip, which is where the time actually goes.
//!
//! Every selector here predates the bundle's macOS floor by a decade or more, which is
//! the point of the module: the crate this replaced (`file_icon_provider`) reached
//! `UTType` on a code path Cmdr never called, and reaching it hard-linked
//! `UniformTypeIdentifiers.framework` into the binary. That framework arrived in macOS
//! 11, so dyld refused to launch Cmdr on Catalina at all. `desktop-macos-framework-floor`
//! now fails the build on a framework newer than the floor; the story is in
//! `docs/notes/system-requirements-and-es2025.md`.

use image::RgbaImage;
use log::{debug, error};
use objc2::AnyThread;
use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageRep, NSCompositingOperation, NSGraphicsContext, NSImage, NSWorkspace};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::path::Path;

/// Renders the OS icon for `path` at `size`×`size` pixels, or `None` if macOS has no
/// icon for it (a path that vanished mid-listing is the common case, so the miss is
/// logged at debug, not as a failure).
pub(super) fn render_file_icon(path: &Path, size: u16) -> Option<RgbaImage> {
    let file_path = path_to_nsstring(path)?;
    let image = NSWorkspace::sharedWorkspace().iconForFile(&file_path);
    let bitmap = create_bitmap_representation(size)?;
    let context = NSGraphicsContext::graphicsContextWithBitmapImageRep(&bitmap).or_else(|| {
        error!("Failed to create a graphics context for an icon bitmap");
        None
    })?;

    let side = u32::from(size);
    let pixels = draw_into_bitmap(&image, &context, &bitmap, side)?;
    RgbaImage::from_raw(side, side, pixels)
}

/// Allocates the RGBA backing store the icon gets drawn into: 8 bits per sample, four
/// samples per pixel, alpha on, non-planar, so `bitmapData` is one tightly-packed
/// `RGBA8` buffer `image::RgbaImage` can adopt as-is.
fn create_bitmap_representation(size: u16) -> Option<Retained<NSBitmapImageRep>> {
    let color_space_name = NSString::from_str("NSDeviceRGBColorSpace");
    let side = isize::try_from(size).ok()?;
    // SAFETY: a null `planes` pointer is the documented way to ask AppKit to allocate
    // the buffer itself, and the shape arguments describe that buffer consistently:
    // 4 samples per pixel × 8 bits per sample is the 32 bits per pixel declared, and
    // `side * 4` bytes per row is that width with no padding. An inconsistent set
    // returns nil rather than corrupting anything, which the `is_none` below reports.
    let bitmap = unsafe {
        NSBitmapImageRep::initWithBitmapDataPlanes_pixelsWide_pixelsHigh_bitsPerSample_samplesPerPixel_hasAlpha_isPlanar_colorSpaceName_bytesPerRow_bitsPerPixel(
            NSBitmapImageRep::alloc(),
            std::ptr::null_mut(),
            side,
            side,
            8,
            4,
            true,
            false,
            &color_space_name,
            side * 4,
            32,
        )
    };
    if bitmap.is_none() {
        error!("Failed to create an NSBitmapImageRep for an icon");
    }
    bitmap
}

/// Draws `image` into `context` at `side`×`side` and returns the bitmap's bytes.
///
/// `NSCompositingOperation::Copy` (rather than `SourceOver`) is what keeps a transparent
/// icon transparent: the bitmap starts as uninitialized memory, so blending over it
/// would mix whatever was there into the edges.
fn draw_into_bitmap(
    image: &NSImage,
    context: &NSGraphicsContext,
    bitmap: &NSBitmapImageRep,
    side: u32,
) -> Option<Vec<u8>> {
    let reported = image.size();
    if reported.width < 1.0 || reported.height < 1.0 {
        debug!("macOS returned a zero-sized icon image");
        return None;
    }

    let side_f = f64::from(side);
    let desired = NSSize {
        width: side_f,
        height: side_f,
    };

    // `setCurrentContext` makes `context` the destination for the draw that follows, and
    // `restoreGraphicsState` puts back whatever this thread had before, so the swap
    // never outlives this function.
    context.saveGraphicsState();
    NSGraphicsContext::setCurrentContext(Some(context));
    image.setSize(desired);
    image.drawAtPoint_fromRect_operation_fraction(
        NSPoint::ZERO,
        NSRect::new(NSPoint::ZERO, desired),
        NSCompositingOperation::Copy,
        1.0,
    );
    context.flushGraphics();
    context.restoreGraphicsState();

    let bytes = usize::try_from(bitmap.bytesPerPlane()).ok()?;
    // SAFETY: `bitmapData` points at the buffer `bitmap` owns, and `bytesPerPlane` is
    // that buffer's own length, reported by the same object. `bitmap` is borrowed for
    // this whole function, so the pointer stays live across the read, and `to_vec`
    // copies out before the borrow ends. The rep is non-planar (see
    // `create_bitmap_representation`), so there is exactly one plane to read.
    Some(unsafe { std::slice::from_raw_parts(bitmap.bitmapData(), bytes) }.to_vec())
}

/// Canonicalizes `path` and hands it over as an `NSString`.
///
/// The canonicalize doubles as the existence check (`iconForFile:` answers a generic
/// document icon for a path that isn't there, which would cache a wrong icon under a
/// real id) and resolves symlinks, so a link shows what it points at. The icon-id
/// scheme wants exactly that: `symlink-file` / `symlink-dir` are separate ids resolved
/// from their own sample paths, never through here.
fn path_to_nsstring(path: &Path) -> Option<Retained<NSString>> {
    let canonical = match path.canonicalize() {
        Ok(canonical) => canonical,
        Err(error) => {
            debug!("No icon for '{}': {error}", path.display());
            return None;
        }
    };
    let Some(text) = canonical.to_str() else {
        error!("Path '{}' is not valid UTF-8, so it gets no icon", canonical.display());
        return None;
    };
    Some(NSString::from_str(text))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `/etc/hosts` exists on every macOS and always has an icon, so it's the same
    /// stand-in `get_sample_path_for_icon_id` uses for the generic `file` id.
    const ALWAYS_PRESENT: &str = "/etc/hosts";

    #[test]
    fn renders_a_real_icon_at_the_requested_size() {
        let icon = render_file_icon(Path::new(ALWAYS_PRESENT), 32).expect("macOS has an icon for /etc/hosts");
        assert_eq!(icon.dimensions(), (32, 32));

        // The bitmap starts as uninitialized memory, so "we drew something" has to be
        // asserted on the pixels, not on the buffer's length: a render that silently
        // drew nothing would still hand back 32×32×4 plausible-looking bytes. A real
        // document icon is mostly opaque in the middle and transparent at the corners.
        assert!(icon.get_pixel(16, 16)[3] > 0, "the middle of a document icon is opaque");
        assert_eq!(
            icon.get_pixel(0, 0)[3],
            0,
            "the corner outside the page shape stays transparent, which is what `NSCompositingOperation::Copy` buys: blending over the uninitialized bitmap instead would leave garbage there",
        );
    }

    #[test]
    fn a_size_of_zero_renders_nothing_rather_than_dividing_by_it() {
        assert!(render_file_icon(Path::new(ALWAYS_PRESENT), 0).is_none());
    }

    #[test]
    fn a_path_that_is_not_there_gets_no_icon() {
        // Guards the canonicalize: `iconForFile:` answers a generic document icon for a
        // missing path, which would get cached under an id that means something else.
        assert!(render_file_icon(Path::new("/nope/not/a/real/path"), 32).is_none());
    }
}
