//! The code signature an item carries, read with the Security framework, plus one
//! bounded check that its main executable matches it.
//!
//! Who signed it comes from code requirements evaluated against the signature
//! alone (`kSecCSBasicValidateOnly`: nothing hashed, no network), so no
//! certificate name is ever parsed to decide a signer kind. The org is the leaf
//! certificate's own summary, an item-reported string.
//!
//! The check (`kSecCSDoNotValidateResources | kSecCSCheckAllArchitectures`)
//! hashes the main executable's pages, every architecture, and nothing else.
//! Measured on macOS 27.0.1, 2026-11-13, warm page cache, Swift spike over the same
//! calls: Slack and Xcode under 1 ms, IntelliJ 8 ms, a 401 MB thin executable
//! (Warp) 915 ms. Without `kSecCSCheckAllArchitectures` a byte flipped in a
//! universal binary's x86_64 slice passed on an arm64 Mac; with it, the same copy
//! fails `errSecCSSignatureFailed`. A check past its budget is
//! `NotChecked(TimedOut)`, ❌ never a pass.

use std::ffi::{c_int, c_void};
use std::str::FromStr;
use std::sync::mpsc;
use std::time::Duration;

use core_foundation::array::{CFArray, CFArrayRef};
use core_foundation::base::{CFType, TCFType};
use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
use core_foundation::number::CFNumber;
use core_foundation::string::{CFString, CFStringRef};
use core_foundation::url::CFURL;
use security_framework::certificate::SecCertificate;
use security_framework::os::macos::code_signing::{Flags, SecRequirement, SecStaticCode};
use security_framework_sys::code_signing::SecStaticCodeCheckValidity;

use cmdr_about::fact::{AboutTarget, Fact, UnknownWhy};
use cmdr_about::payload::{ExecutableCheck, NotCheckedWhy, Signature, Signer, StapledTicket};
use cmdr_about::text::ItemText;

/// How long the executable check may take. Inside the signature row's own
/// budget, with room for the signing-information read before it.
pub(crate) const CHECK_BUDGET: Duration = Duration::from_millis(600);

/// `errSecCSUnsigned`: the code carries no signature.
const UNSIGNED: i32 = -67062;
/// `errSecCSSignatureFailed`: the bytes don't match what the signature covers.
const SIGNATURE_FAILED: i32 = -67061;
/// `kSecCSSigningInformation`: include the certificates and team id.
const SIGNING_INFORMATION: u32 = 1 << 1;
/// `kSecCodeSignatureAdhoc` in the signature's flags.
const ADHOC: u32 = 0x0002;

// SAFETY: the Security framework's documented C signature and constants
// (`Security/SecCode.h`), declared exactly as `eject/holders/facts.rs` declares it
// (a second spelling trips `clashing_extern_declarations`).
// `SecCodeCopySigningInformation` accepts a static code object and returns a +1
// dictionary on success; the keys are framework-owned constant CFStrings.
#[link(name = "Security", kind = "framework")]
unsafe extern "C" {
    fn SecCodeCopySigningInformation(code: *const c_void, flags: u32, information: *mut CFDictionaryRef) -> c_int;
    static kSecCodeInfoTeamIdentifier: CFStringRef;
    static kSecCodeInfoCertificates: CFStringRef;
    static kSecCodeInfoFlags: CFStringRef;
}

/// Who a requirement names, strongest claim first. Apple's own: `anchor apple`.
/// The Mac App Store's signing leaf, and Developer ID's intermediate plus leaf,
/// by the OIDs Apple documents in "Code Signing Requirement Language".
const SIGNERS: [(&str, SignerKind); 3] = [
    ("anchor apple", SignerKind::Apple),
    (
        "anchor apple generic and certificate leaf[field.1.2.840.113635.100.6.1.9] exists",
        SignerKind::AppStore,
    ),
    (
        "anchor apple generic and certificate 1[field.1.2.840.113635.100.6.2.6] exists \
         and certificate leaf[field.1.2.840.113635.100.6.1.13] exists",
        SignerKind::DeveloperId,
    ),
];

#[derive(Clone, Copy)]
enum SignerKind {
    Apple,
    AppStore,
    DeveloperId,
}

pub(crate) fn signature(path: &str, target: AboutTarget, extension: Option<&str>) -> Fact<Signature> {
    objc2::rc::autoreleasepool(|_| read(path, target, extension))
}

fn read(path: &str, target: AboutTarget, extension: Option<&str>) -> Fact<Signature> {
    let is_bundle = matches!(target, AboutTarget::AppBundle | AboutTarget::Package);
    let is_disk_image = extension.is_some_and(|extension| extension.eq_ignore_ascii_case("dmg"));
    let ticket = stapled_ticket(path, is_bundle, is_disk_image);
    let Some(code) = static_code(path, is_bundle) else {
        return Fact::unknown(UnknownWhy::Unparseable);
    };
    match basic_validity(&code) {
        Ok(()) => {}
        Err(UNSIGNED) => {
            return Fact::Known(Signature::read(
                Signer::Unsigned,
                ticket,
                ExecutableCheck::NotChecked(NotCheckedWhy::NothingToCheck),
            ));
        }
        // A signature that doesn't hold together: Cmdr can't say whose it is.
        Err(_) => return Fact::unknown(UnknownWhy::Unparseable),
    }
    let info = signing_information(&code);
    let signer = signer_of(&code, info.as_ref());
    let checked = if is_disk_image {
        // A disk image's signature covers the whole image: hashing it is no
        // bounded check.
        ExecutableCheck::NotChecked(NotCheckedWhy::NotSupported)
    } else {
        check_executable(path, is_bundle)
    };
    Fact::Known(Signature::read(signer, ticket, checked))
}

fn static_code(path: &str, is_directory: bool) -> Option<SecStaticCode> {
    let url = CFURL::from_path(path, is_directory)?;
    SecStaticCode::from_path(&url, Flags::NONE).ok()
}

/// The signature's own integrity, nothing hashed: `Err(UNSIGNED)` for none.
fn basic_validity(code: &SecStaticCode) -> Result<(), i32> {
    let flags = (Flags::BASIC_VALIDATE_ONLY | Flags::NO_NETWORK_ACCESS).bits();
    // SAFETY: `code` is a live static code object for the call; a null requirement
    // asks for the signature's own validity alone.
    let status = unsafe { SecStaticCodeCheckValidity(code.as_concrete_TypeRef(), flags, std::ptr::null_mut()) };
    if status == 0 { Ok(()) } else { Err(status) }
}

fn signing_information(code: &SecStaticCode) -> Option<CFDictionary<CFString, CFType>> {
    let mut info: CFDictionaryRef = std::ptr::null();
    // SAFETY: `code` is live; on success `info` holds a +1 dictionary, which the
    // create rule below takes ownership of.
    let status = unsafe {
        SecCodeCopySigningInformation(
            code.as_concrete_TypeRef().cast_const().cast(),
            SIGNING_INFORMATION,
            &mut info,
        )
    };
    if status != 0 || info.is_null() {
        return None;
    }
    // SAFETY: a non-null +1 CFDictionary from the call above, keyed by CFStrings.
    Some(unsafe { CFDictionary::wrap_under_create_rule(info) })
}

fn signer_of(code: &SecStaticCode, info: Option<&CFDictionary<CFString, CFType>>) -> Signer {
    let flags = info
        .and_then(|info| {
            // SAFETY: a framework-owned constant CFString, live for the process.
            info.find(unsafe { CFString::wrap_under_get_rule(kSecCodeInfoFlags) })
                .and_then(|value| value.downcast::<CFNumber>())
                .and_then(|number| number.to_i64())
        })
        .unwrap_or(0);
    if u32::try_from(flags).is_ok_and(|flags| flags & ADHOC != 0) {
        return Signer::AdHoc;
    }
    let requirement_flags = Flags::BASIC_VALIDATE_ONLY | Flags::NO_NETWORK_ACCESS;
    let kind = SIGNERS.iter().find_map(|(text, kind)| {
        let requirement = SecRequirement::from_str(text).ok()?;
        code.check_validity(requirement_flags, &requirement)
            .ok()
            .map(|()| *kind)
    });
    match kind {
        Some(SignerKind::Apple) => Signer::Apple,
        Some(SignerKind::AppStore) => Signer::AppStore,
        Some(SignerKind::DeveloperId) => {
            let team = info.and_then(team_id).unwrap_or_default();
            let summary = info.and_then(leaf_summary).unwrap_or_default();
            Signer::DeveloperId {
                org: ItemText::new(developer_org(&summary, &team)),
                team: ItemText::new(&team),
            }
        }
        None => Signer::Unrecognized,
    }
}

fn team_id(info: &CFDictionary<CFString, CFType>) -> Option<String> {
    // SAFETY: a framework-owned constant CFString, live for the process.
    let key = unsafe { CFString::wrap_under_get_rule(kSecCodeInfoTeamIdentifier) };
    info.find(key)?.downcast::<CFString>().map(|team| team.to_string())
}

fn leaf_summary(info: &CFDictionary<CFString, CFType>) -> Option<String> {
    // SAFETY: a framework-owned constant CFString, live for the process.
    let key = unsafe { CFString::wrap_under_get_rule(kSecCodeInfoCertificates) };
    let value = info.find(key)?;
    if !value.instance_of::<CFArray<CFType>>() {
        return None;
    }
    // SAFETY: checked to be a CFArray just above; the get rule retains it for the
    // wrapper's life. Its elements are SecCertificates, leaf first.
    let certificates: CFArray<SecCertificate> =
        unsafe { CFArray::wrap_under_get_rule(value.as_CFTypeRef() as CFArrayRef) };
    let leaf = certificates.get(0)?;
    Some(leaf.subject_summary())
}

/// The organization a Developer ID leaf names. Its summary is Apple's fixed shape,
/// `Developer ID Application: <org> (<team>)`; anything else stays whole, as the
/// certificate said it.
fn developer_org<'a>(summary: &'a str, team: &str) -> &'a str {
    let without_prefix = summary
        .split_once(": ")
        .filter(|(prefix, _)| prefix.starts_with("Developer ID"))
        .map_or(summary, |(_, rest)| rest);
    let team_suffix = format!(" ({team})");
    without_prefix
        .strip_suffix(team_suffix.as_str())
        .unwrap_or(without_prefix)
}

/// A bundle's stapled ticket is `Contents/CodeResources` (Slack, verified
/// 2026-11-13: `codesign -dvvv` says "Notarization Ticket=stapled" and the file
/// is there). A bare executable can't carry one. A disk image keeps it inside its
/// signature, which Cmdr doesn't read.
fn stapled_ticket(path: &str, is_bundle: bool, is_disk_image: bool) -> StapledTicket {
    if is_disk_image {
        return StapledTicket::NotRead;
    }
    if !is_bundle {
        return StapledTicket::Absent;
    }
    let ticket = std::path::Path::new(path).join("Contents/CodeResources");
    match std::fs::symlink_metadata(ticket) {
        Ok(metadata) if metadata.is_file() => StapledTicket::Present,
        _ => StapledTicket::Absent,
    }
}

/// Compares the main executable against its signature on its own pool job, and
/// waits for it at most [`CHECK_BUDGET`]. A check still hashing then is left to
/// finish on its thread, its answer dropped.
fn check_executable(path: &str, is_bundle: bool) -> ExecutableCheck {
    let (answer, waiting) = mpsc::sync_channel(1);
    let path = path.to_string();
    crate::about::pool::submit(Box::new(move || {
        crate::file_system::framework_pool::note_activity("SecStaticCodeCheckValidity", "signature");
        let outcome = objc2::rc::autoreleasepool(|_| executable_matches(&path, is_bundle));
        let _ = answer.send(outcome);
    }));
    match waiting.recv_timeout(CHECK_BUDGET) {
        Ok(outcome) => outcome,
        Err(_) => ExecutableCheck::NotChecked(NotCheckedWhy::TimedOut),
    }
}

fn executable_matches(path: &str, is_bundle: bool) -> ExecutableCheck {
    let Some(code) = static_code(path, is_bundle) else {
        return ExecutableCheck::NotChecked(NotCheckedWhy::CheckUnavailable);
    };
    let flags = (Flags::DO_NOT_VALIDATE_RESOURCES | Flags::CHECK_ALL_ARCHITECTURES | Flags::NO_NETWORK_ACCESS).bits();
    // SAFETY: `code` is a live static code object for the call; a null requirement
    // checks the code against its own signature.
    let status = unsafe { SecStaticCodeCheckValidity(code.as_concrete_TypeRef(), flags, std::ptr::null_mut()) };
    match status {
        0 => ExecutableCheck::ExecutableMatches,
        SIGNATURE_FAILED => ExecutableCheck::ExecutableDiffers,
        _ => ExecutableCheck::NotChecked(NotCheckedWhy::CheckUnavailable),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_developer_id_leaf_names_its_org_without_the_prefix_or_team() {
        assert_eq!(
            developer_org(
                "Developer ID Application: SLACK TECHNOLOGIES L.L.C. (BQR82RBBHL)",
                "BQR82RBBHL"
            ),
            "SLACK TECHNOLOGIES L.L.C."
        );
        assert_eq!(
            developer_org("Something else: Org (OTHERTEAM)", "BQR82RBBHL"),
            "Something else: Org (OTHERTEAM)",
            "a summary of another shape stays whole"
        );
    }

    #[test]
    fn a_system_tool_carries_apples_signature_and_its_executable_matches() {
        let Fact::Known(signature) = signature("/bin/ls", AboutTarget::File, None) else {
            panic!("/bin/ls is signed");
        };
        assert_eq!(signature.signer, Signer::Apple);
        assert_eq!(
            signature.ticket,
            StapledTicket::DoesNotApply,
            "Apple's own tools aren't notarized, so a missing ticket is no news"
        );
        assert_eq!(signature.checked, ExecutableCheck::ExecutableMatches);
    }

    #[test]
    fn a_system_app_carries_apples_signature() {
        let Fact::Known(signature) = signature(
            "/System/Applications/Calculator.app",
            AboutTarget::AppBundle,
            Some("app"),
        ) else {
            panic!("Calculator is signed");
        };
        assert_eq!(signature.signer, Signer::Apple);
        assert_eq!(signature.ticket, StapledTicket::DoesNotApply);
        assert_eq!(signature.checked, ExecutableCheck::ExecutableMatches);
    }

    #[test]
    fn a_plain_file_has_no_code_to_sign() {
        let dir = crate::test_support::TestDir::new("about-signature");
        let file = dir.as_ref().join("notes.txt");
        std::fs::write(&file, "hello").expect("write the fixture");
        let fact = signature(&file.to_string_lossy(), AboutTarget::File, Some("txt"));
        assert!(
            matches!(
                fact,
                Fact::Known(Signature {
                    signer: Signer::Unsigned,
                    ..
                }) | Fact::Unknown(_)
            ),
            "a text file is no signed code: {fact:?}"
        );
    }

    /// A copied system tool with one byte changed inside its code: the signature it
    /// carries still names Apple, and the check says the executable doesn't match.
    #[test]
    fn a_tampered_executable_is_caught_by_the_bounded_check() {
        let dir = crate::test_support::TestDir::new("about-signature-tampered");
        let copy = dir.as_ref().join("ls");
        let mut bytes = std::fs::read("/bin/ls").expect("read /bin/ls");
        // Past the fat header and every slice's first page, well inside the code.
        let at = 0x4000 + 0x200;
        bytes[at] ^= 0xFF;
        std::fs::write(&copy, bytes).expect("write the copy");
        let Fact::Known(signature) = signature(&copy.to_string_lossy(), AboutTarget::File, None) else {
            panic!("the copy still carries its signature");
        };
        assert_eq!(signature.checked, ExecutableCheck::ExecutableDiffers);
    }
}
