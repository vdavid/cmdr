//! macOS-specific icon fetching that bypasses the Launch Services icon cache.
//!
//! This module provides fresh icons by:
//! 1. Getting the UTI for a file extension
//! 2. Finding the default app for that UTI
//! 3. Extracting the document icon directly from the app's bundle
//!
//! This ensures we always show the current file association, even if the user
//! just changed it in Finder's "Get Info" → "Open with" → "Change All".

use core_foundation::array::CFArray;
use core_foundation::base::TCFType;
use core_foundation::string::CFString;
use core_foundation::url::CFURL;
use core_services::{
    LSCopyApplicationURLsForBundleIdentifier, LSCopyDefaultRoleHandlerForContentType,
    UTTypeCreatePreferredIdentifierForTag, kLSRolesAll, kUTTagClassFilenameExtension,
};
use image::DynamicImage;
use plist::Value;
use std::path::{Path, PathBuf};

/// Gets the UTI (Uniform Type Identifier) for a file extension.
fn get_uti_for_extension(ext: &str) -> Option<CFString> {
    let tag = CFString::new(ext);

    unsafe {
        let uti_ref = UTTypeCreatePreferredIdentifierForTag(
            kUTTagClassFilenameExtension,
            tag.as_concrete_TypeRef(),
            std::ptr::null(),
        );

        if uti_ref.is_null() {
            return None;
        }

        Some(CFString::wrap_under_create_rule(uti_ref))
    }
}

/// Gets the default application bundle ID for a content type (UTI).
fn get_default_app_bundle_id(uti: &CFString) -> Option<CFString> {
    unsafe {
        let bundle_id_ref = LSCopyDefaultRoleHandlerForContentType(uti.as_concrete_TypeRef(), kLSRolesAll);

        if bundle_id_ref.is_null() {
            return None;
        }

        Some(CFString::wrap_under_create_rule(bundle_id_ref))
    }
}

/// Gets the application URL for a bundle identifier.
fn get_app_url_for_bundle_id(bundle_id: &CFString) -> Option<PathBuf> {
    unsafe {
        let urls_ref = LSCopyApplicationURLsForBundleIdentifier(bundle_id.as_concrete_TypeRef(), std::ptr::null_mut());

        if urls_ref.is_null() {
            return None;
        }

        let urls: CFArray<CFURL> = CFArray::wrap_under_create_rule(urls_ref);

        if urls.is_empty() {
            return None;
        }

        // Get the first URL (primary app location)
        let url = urls.get(0)?;

        // Convert CFURL to PathBuf via string
        let path_str = url.get_string().to_string();
        // Remove "file://" prefix and decode URL encoding
        let path = path_str.strip_prefix("file://").unwrap_or(&path_str);
        let decoded = urlencoding::decode(path).ok()?;
        Some(PathBuf::from(decoded.into_owned()))
    }
}

/// Reads the app's Info.plist and finds the document icon for the given UTI.
fn get_document_icon_name_from_bundle(app_path: &Path, uti: &str) -> Option<String> {
    let plist_path = app_path.join("Contents/Info.plist");
    let plist_data = std::fs::read(&plist_path).ok()?;
    let plist: Value = plist::from_bytes(&plist_data).ok()?;

    // Look in CFBundleDocumentTypes for a matching UTI
    if let Some(doc_types) = plist.as_dictionary()?.get("CFBundleDocumentTypes")
        && let Some(doc_types_arr) = doc_types.as_array()
    {
        for doc_type in doc_types_arr {
            if let Some(doc_dict) = doc_type.as_dictionary() {
                // Check if this document type handles our UTI
                if let Some(content_types) = doc_dict.get("LSItemContentTypes")
                    && let Some(types_arr) = content_types.as_array()
                {
                    for t in types_arr {
                        if let Some(type_str) = t.as_string()
                            && type_str.eq_ignore_ascii_case(uti)
                        {
                            // Found matching document type, get its icon
                            if let Some(icon) = doc_dict.get("CFBundleTypeIconFile") {
                                return icon.as_string().map(String::from);
                            }
                        }
                    }
                }
            }
        }
    }

    // No document-specific icon found.
    // Check if we should fall back to the app's main icon (configurable).
    if !crate::config::USE_APP_ICONS_AS_DOCUMENT_ICONS {
        // Return None to fall back to temp file approach → Finder-style document icons
        return None;
    }

    // Fallback to the app's main icon - this is desirable because:
    // 1. It clearly shows which app will open this file type
    // 2. It updates immediately when the user changes file associations
    // 3. It's more informative than a generic document icon
    plist
        .as_dictionary()?
        .get("CFBundleIconFile")
        .and_then(|v| v.as_string())
        .map(String::from)
}

/// Loads an ICNS icon file and converts it to a DynamicImage.
/// Uses the `icns` crate which properly parses macOS icon format.
fn load_icns_icon(icon_path: &Path) -> Option<DynamicImage> {
    let file = std::fs::File::open(icon_path).ok()?;
    let icon_family = icns::IconFamily::read(file).ok()?;

    // Get the largest available icon (prefer 256x256 or 512x512)
    let icon_types = [
        icns::IconType::RGBA32_512x512,
        icns::IconType::RGBA32_256x256,
        icns::IconType::RGBA32_128x128,
        icns::IconType::RGBA32_64x64,
        icns::IconType::RGBA32_32x32,
    ];

    for icon_type in icon_types {
        if let Ok(icon_image) = icon_family.get_icon_with_type(icon_type) {
            // Convert icns::Image to image::DynamicImage
            let width = icon_image.width();
            let height = icon_image.height();
            let pixels = icon_image.into_data();

            if let Some(img) = image::RgbaImage::from_raw(width, height, pixels.to_vec()) {
                return Some(DynamicImage::ImageRgba8(img));
            }
        }
    }

    None
}

/// Fetches the icon for a file extension directly from the default app's bundle.
/// This bypasses the Launch Services icon cache.
pub fn fetch_fresh_icon_for_extension(ext: &str) -> Option<DynamicImage> {
    // 1. Get UTI for extension
    let uti = get_uti_for_extension(ext)?;
    let uti_str = uti.to_string();

    // 2. Get default app bundle ID for this UTI
    let bundle_id = get_default_app_bundle_id(&uti)?;

    // 3. Get app URL from bundle ID
    let app_path = get_app_url_for_bundle_id(&bundle_id)?;

    // 4. Find the document icon name in the app's Info.plist
    let icon_name = get_document_icon_name_from_bundle(&app_path, &uti_str)?;

    // 5. Build the icon path (in Resources folder)
    // Icon name might or might not have .icns extension
    let icon_filename = if icon_name.ends_with(".icns") {
        icon_name
    } else {
        format!("{}.icns", icon_name)
    };
    let icon_path = app_path.join("Contents/Resources").join(&icon_filename);

    // 6. Load and return the icon
    load_icns_icon(&icon_path)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_uti_for_extension() {
        let uti = get_uti_for_extension("txt");
        assert!(uti.is_some());
        let uti_str = uti.unwrap().to_string();
        assert!(uti_str.contains("text") || uti_str.contains("txt") || !uti_str.is_empty());
    }

    #[test]
    fn test_get_default_app_for_txt() {
        if let Some(uti) = get_uti_for_extension("txt") {
            let bundle_id = get_default_app_bundle_id(&uti);
            // Most systems have a default text editor, but this test shouldn't fail if not
            if let Some(bid) = bundle_id {
                println!("Default app for .txt: {bid}");
            }
        }
    }

    #[test]
    fn test_fetch_fresh_icon() {
        // Try a common extension
        let icon = fetch_fresh_icon_for_extension("pdf");
        // This might fail if no PDF reader is installed, which is fine
        if let Some(img) = icon {
            println!("Got PDF icon: {}x{}", img.width(), img.height());
        }
    }

    #[test]
    fn test_mp4_debug() {
        // Debug: Check what UTI we get for mp4
        let uti = get_uti_for_extension("mp4");
        println!("UTI for mp4: {:?}", uti.as_ref().map(|u| u.to_string()));

        if let Some(uti) = uti {
            let bundle_id = get_default_app_bundle_id(&uti);
            println!(
                "Default app bundle ID for mp4: {:?}",
                bundle_id.as_ref().map(|b| b.to_string())
            );

            if let Some(bid) = bundle_id {
                let app_url = get_app_url_for_bundle_id(&bid);
                println!("App URL for bundle ID: {:?}", app_url);

                if let Some(app_path) = app_url {
                    let icon_name = get_document_icon_name_from_bundle(&app_path, &uti.to_string());
                    println!("Document icon name: {:?}", icon_name);
                }
            }
        }
    }
}
