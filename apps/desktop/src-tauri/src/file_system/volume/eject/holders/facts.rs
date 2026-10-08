//! What KIND of thing each named holder is, which is what picks the sentence a refusal
//! says.
//!
//! "Something still has files open there" leaves a person guessing. "Photos still has
//! files open there", "macOS still has files open there", and "a disk image stored
//! there is still open" each say what to do next, and they're different actions,
//! so the kind has to be a decision rather than a guess. ❌ Never from a process name, a
//! path prefix, or a message: [`classify`] reads typed signals only, and it's pure.
//!
//! **The order is the design**, first rule that answers wins:
//!
//! 1. Cmdr's own process, or one it started → [`HolderKind::Cmdr`] (our bug, not theirs).
//! 2. An app this process or an ancestor belongs to → [`HolderKind::App`]. Walking
//!    ancestors is what names Warp for the `zsh` a person `cd`'d into the drive.
//! 3. A disk image stored ON the drive → [`HolderKind::DiskImage`]: closing apps can't
//!    free that one, the image has to be ejected first.
//! 4. The app macOS holds RESPONSIBLE for it → [`HolderKind::App`], which is what turns a
//!    `com.apple.WebKit.WebContent` helper into the browser a person can quit.
//! 5. An executable that lives on the drive being ejected → [`HolderKind::Tool`], ❗ with
//!    no code-signing query: see the safety rule below.
//! 6. An Apple platform binary → [`HolderKind::System`]; anything else →
//!    [`HolderKind::Tool`]. A signature nothing could read stays
//!    [`HolderKind::Unclassified`], since "tool" would be a guess.
//!
//! ❗ **Run a code-signing query only when the executable is known to be off every target
//! mount.** `SecCodeCopyGuestWithAttributes` reads the binary, which puts Cmdr itself in
//! the kernel's holder list for the very volume it's letting go of: measured at 4.5 s,
//! and a Whole unmount in that window named the prober as the dissenter. Rule 5 is that
//! guard as much as it is a classification, and rule 4's fallback carries its own copy of
//! it for the responsible process. An unreadable target or executable stays
//! [`HolderKind::Unclassified`].
//!
//! ❗ **The facts run after every path's walk, on the same abandonable thread and inside
//! the same budget.** A holder the budget cuts short keeps its name and stays
//! `Unclassified`: a named pid with no kind still words better than nothing, and dropping
//! it would repeat the "couldn't tell means nothing there" collapse this module exists to
//! avoid.

use std::path::{Path, PathBuf};
use std::time::Instant;

use super::{HolderKind, VolumeHolder};

mod nested_images;

#[cfg(test)]
mod tests;

/// What an app-shaped holder is called.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct AppFacts {
    /// The app's own display name, as macOS shows it in the Dock.
    pub(super) name: String,
    /// Its bundle id, when one answered.
    pub(super) bundle_id: Option<String>,
}

/// Everything the facts stage learned about one holder, gathered in rule order and
/// stopping at the first rule that answered.
///
/// ❗ A field left at its default means "no rule needed it", ❌ never "we asked and the
/// answer was no". That's what keeps [`classify`] pure while the gathering stays lazy:
/// a holder rule 1 answered for carries none of the later signals, and classifying it
/// again from this struct gives the same answer.
// DEFAULT-OK: every field's zero value means "no rule needed this signal", which is the
// lazy-gathering contract above, ❌ never "we asked the disk and the answer was no". The
// one field that could be mistaken for a negative answer, `platform_binary`, is an
// `Option` for exactly that reason.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub(super) struct ProcessFacts {
    /// Cmdr's own process, or one it started.
    pub(super) is_cmdr: bool,
    /// The app this process or an ancestor belongs to.
    pub(super) app: Option<AppFacts>,
    /// The disk image it serves, stored on the drive being ejected.
    pub(super) disk_image: Option<String>,
    /// The app macOS holds responsible for it.
    pub(super) responsible: Option<AppFacts>,
    /// Its executable lives on the drive being ejected.
    pub(super) executable_on_target: bool,
    /// Whether it's an Apple platform binary. ❗ `None` is "nothing could tell", which
    /// includes "we never asked because rule 5 already answered".
    pub(super) platform_binary: Option<bool>,
}

/// What the facts make of a holder: the kind, plus the better name and bundle id when
/// something answered with one.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Classified {
    /// The name to show instead of the executable's, when an app or an image answered.
    pub(super) name: Option<String>,
    /// The bundle id, when an app answered.
    pub(super) bundle_id: Option<String>,
    /// What kind of holder it is.
    pub(super) kind: HolderKind,
}

/// What kind of holder `facts` describes, and what to call it.
///
/// Pure, and the whole decision: every signal was gathered before this ran, so the rule
/// order lives in one readable place and tests table-drive it without a real process.
pub(super) fn classify(facts: &ProcessFacts) -> Classified {
    let named = |app: &AppFacts, kind| Classified {
        name: Some(app.name.clone()),
        bundle_id: app.bundle_id.clone(),
        kind,
    };
    if facts.is_cmdr {
        return Classified {
            name: None,
            bundle_id: None,
            kind: HolderKind::Cmdr,
        };
    }
    if let Some(app) = &facts.app {
        return named(app, HolderKind::App);
    }
    if let Some(image) = &facts.disk_image {
        return Classified {
            name: Some(image.clone()),
            bundle_id: None,
            kind: HolderKind::DiskImage,
        };
    }
    if let Some(responsible) = &facts.responsible {
        return named(responsible, HolderKind::App);
    }
    let kind = if facts.executable_on_target {
        HolderKind::Tool
    } else {
        match facts.platform_binary {
            Some(true) => HolderKind::System,
            Some(false) => HolderKind::Tool,
            // Nothing could read the signature, so any kind here would be invented.
            None => HolderKind::Unclassified,
        }
    };
    Classified {
        name: None,
        bundle_id: None,
        kind,
    }
}

/// Puts what the facts found onto the holder, keeping the executable name when nothing
/// better answered.
pub(super) fn rename(holder: &mut VolumeHolder, what: &Classified) {
    if let Some(name) = &what.name {
        holder.name.clone_from(name);
    }
    holder.bundle_id.clone_from(&what.bundle_id);
    holder.kind = what.kind;
}

/// Classifies each of `pids`, handing every answer to `record` as it lands.
///
/// ❗ Runs on the holder scan's own abandonable thread, after every path has been walked,
/// so it knows the device of EVERY mount of the teardown (rule 5 needs all of them: a
/// process holding one partition may run from a sibling, and a code-signing query there
/// is just as wrong). Stops at `deadline`, leaving the holders it never reached named and
/// `Unclassified`.
pub(super) fn name_the_kinds(
    pids: &[u32],
    paths: &[PathBuf],
    deadline: Instant,
    mut record: impl FnMut(u32, Classified),
) {
    if pids.is_empty() {
        return;
    }
    let ours = std::process::id();
    let about = Surroundings::new(target_devices(paths));
    for (done, pid) in pids.iter().enumerate() {
        if Instant::now() >= deadline {
            log::info!(
                target: "eject",
                "The holder scan ran out of budget after naming what kind {done} of {} holders are; the rest stay unclassified",
                pids.len()
            );
            return;
        }
        // Every AppKit and Core Foundation object below is autoreleased, and this thread
        // has no run loop to drain a pool of its own.
        record(*pid, platform::within_pool(|| classify(&gather(*pid, ours, &about))));
    }
}

/// The devices of the mounts being torn down, for rule 5's "is this executable on the
/// drive" test.
///
/// ❗ A second `stat` of each mount root, after the walk's own. It's cheap on a healthy
/// mount, and it happens only once every path has already answered, so a root that hangs
/// here costs the KINDS and never the names. ❌ Never `f_fsid`: on the boot volume it
/// names the sealed system snapshot rather than the mount (§ "Spike results" 8). `None`
/// means at least one root couldn't be identified, ❌ never that the known roots are the
/// whole drive.
fn target_devices(paths: &[PathBuf]) -> Option<Vec<u64>> {
    paths.iter().map(|path| super::root_device(path)).collect()
}

/// What the facts stage knows about the drive as a whole, shared by every holder it
/// classifies.
pub(super) struct Surroundings {
    /// The device of every mount being torn down, or `None` when any was unreadable.
    targets: Option<Vec<u64>>,
    /// The attached disk images, read at most once per scan and only when a holder gets
    /// as far as rule 3. `None` inside means nothing could be read.
    images: std::cell::OnceCell<Option<Vec<nested_images::AttachedImage>>>,
}

impl Surroundings {
    /// The drive, by the device of each of its mounts.
    pub(super) fn new(targets: Option<Vec<u64>>) -> Self {
        Self {
            targets,
            images: std::cell::OnceCell::new(),
        }
    }

    /// The disk image `pid` serves, when it's stored on the drive being ejected.
    fn image_named(&self, pid: u32) -> Option<String> {
        let targets = self.targets.as_ref()?;
        if targets.is_empty() {
            return None;
        }
        let images = self.images.get_or_init(nested_images::attached).as_ref()?;
        nested_images::stored_on(images, pid, targets, device_of).map(|image| image.name.clone())
    }

    /// Whether `pid`'s executable lives on the drive being ejected, or `None` when
    /// either side of the comparison couldn't be identified.
    ///
    /// ❗ Rule 5, and the guard rule 6 and rule 4's fallback both lean on.
    fn owns_executable(&self, pid: u32) -> Option<bool> {
        let targets = self.targets.as_ref()?;
        if targets.is_empty() {
            return Some(false);
        }
        let executable = platform::executable_path(pid)?;
        let device = device_of(&executable)?;
        Some(targets.contains(&device))
    }

    /// The app macOS holds responsible for `pid`, when there is one.
    ///
    /// A responsible process with no `NSRunningApplication` (Google Drive's is one) still
    /// names an app, through the display name its signature seals — but ❌ only when its
    /// executable is known to be off the drive, because that read is the code-signing
    /// query rule 5 exists to prevent.
    fn responsible_app(&self, pid: u32) -> Option<AppFacts> {
        let responsible = platform::responsible_pid(pid)?;
        if let Some(app) = platform::running_app(responsible) {
            return Some(app);
        }
        if self.owns_executable(responsible) != Some(false) {
            return None;
        }
        Some(AppFacts {
            name: platform::signing_display_name(responsible)?,
            bundle_id: None,
        })
    }
}

/// The device a path sits on, from its own `lstat`.
fn device_of(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).ok().map(|metadata| metadata.dev())
}

/// Everything the rules need about `pid`, gathered in rule order.
///
/// `ours` is Cmdr's own process id as a parameter: a test's holder descends from the test
/// process, and reading that as Cmdr would hide every rule below the first one.
pub(super) fn gather(pid: u32, ours: u32, about: &Surroundings) -> ProcessFacts {
    let mut facts = ProcessFacts::default();
    let lineage = platform::lineage(pid);
    facts.is_cmdr = lineage.contains(&ours);
    if facts.is_cmdr {
        return facts;
    }
    facts.app = lineage.iter().find_map(|relative| platform::running_app(*relative));
    if facts.app.is_some() {
        return facts;
    }
    facts.disk_image = about.image_named(pid);
    if facts.disk_image.is_some() {
        return facts;
    }
    facts.responsible = about.responsible_app(pid);
    if facts.responsible.is_some() {
        return facts;
    }
    match about.owns_executable(pid) {
        Some(true) => facts.executable_on_target = true,
        Some(false) => facts.platform_binary = platform::is_platform_binary(pid),
        None => {}
    }
    facts
}

#[cfg(target_os = "macos")]
mod platform {
    //! The macOS signals behind the rules: LaunchServices, the process tree, the
    //! responsible-process attribution, and code signing.

    use std::ffi::{c_int, c_void};
    use std::path::PathBuf;

    use core_foundation::base::{CFType, TCFType};
    use core_foundation::dictionary::{CFDictionary, CFDictionaryRef};
    use core_foundation::string::{CFString, CFStringRef};
    use objc2_app_kit::{NSApplicationActivationPolicy, NSRunningApplication};
    use security_framework::os::macos::code_signing::{Flags, GuestAttributes, SecCode};

    use super::AppFacts;

    /// How far up the process tree an app is looked for. A shell under a terminal is two
    /// levels; eight leaves room for a helper under a helper without letting a strange
    /// parent chain spin.
    const MAX_ANCESTORS: usize = 8;

    // `SecCodeCopySigningInformation` (`Security/SecCode.h:528`) and the two keys it
    // answers, neither of which `security-framework` binds. Both keys are in the
    // "generic" set, so the default flags are enough to get them.
    #[link(name = "Security", kind = "framework")]
    unsafe extern "C" {
        fn SecCodeCopySigningInformation(code: *const c_void, flags: u32, information: *mut CFDictionaryRef) -> c_int;

        /// Present only when the code was signed as part of an OS release, which is what
        /// "this is macOS itself" means. ❗ A platform binary is ❌ NOT the same as "an
        /// Apple app a person can quit": `/bin/sleep` and Calculator are platform
        /// binaries too, which is why rules 2 and 4 run first.
        static kSecCodeInfoPlatformIdentifier: CFStringRef;

        /// The `Info.plist` the signature seals, which is where a helper with no
        /// `NSRunningApplication` keeps its display name.
        static kSecCodeInfoPList: CFStringRef;
    }

    /// `responsibility_get_pid_responsible_for_pid`, resolved at runtime.
    ///
    /// Private (`libsystem_kernel`), so ❌ never linked against: a macOS that stops
    /// exporting it costs the rule, never the launch. It's what LaunchServices itself
    /// uses to attribute a helper to the app that asked for it (2–8 µs per call,
    /// § "Spike results" 8).
    type ResponsibleFor = unsafe extern "C-unwind" fn(pid: libc::pid_t) -> libc::pid_t;

    /// Runs `work` with an autorelease pool around it.
    pub(super) fn within_pool<T>(work: impl FnOnce() -> T) -> T {
        objc2::rc::autoreleasepool(|_| work())
    }

    /// `pid` and its ancestors, nearest first, stopping at launchd or after
    /// [`MAX_ANCESTORS`] steps.
    pub(super) fn lineage(pid: u32) -> Vec<u32> {
        let mut chain = vec![pid];
        let mut current = pid;
        for _ in 0..MAX_ANCESTORS {
            let Some(parent) = parent_of(current) else { break };
            // launchd is nobody's answer, and a process that claims itself as its parent
            // would spin.
            if parent <= 1 || parent == current || chain.contains(&parent) {
                break;
            }
            chain.push(parent);
            current = parent;
        }
        chain
    }

    /// `pid`'s parent, `None` once the process is gone or isn't ours to see.
    fn parent_of(pid: u32) -> Option<u32> {
        let pid = c_int::try_from(pid).ok()?;
        // SAFETY: a `proc_bsdshortinfo` is a C struct of integers and fixed char arrays,
        // for which all-zero bytes are a valid value; the call below overwrites it.
        let mut info: libc::proc_bsdshortinfo = unsafe { std::mem::zeroed() };
        let size = c_int::try_from(size_of::<libc::proc_bsdshortinfo>()).ok()?;
        // SAFETY: `info` is a live, correctly typed buffer of exactly `size` bytes, which
        // is what `PROC_PIDT_SHORTBSDINFO` fills. Nothing unwinds across the boundary.
        let written = unsafe {
            libc::proc_pidinfo(
                pid,
                libc::PROC_PIDT_SHORTBSDINFO,
                0,
                std::ptr::from_mut(&mut info).cast::<c_void>(),
                size,
            )
        };
        (written == size).then_some(info.pbsi_ppid)
    }

    /// The app `pid` belongs to, `None` for a process LaunchServices doesn't track as
    /// one.
    ///
    /// Any activation policy but prohibited counts: a menu-bar-only app is still
    /// something a person can find and quit, and several of the helpers that hold a drive
    /// belong to one.
    pub(super) fn running_app(pid: u32) -> Option<AppFacts> {
        let pid = libc::pid_t::try_from(pid).ok()?;
        let app = NSRunningApplication::runningApplicationWithProcessIdentifier(pid)?;
        if app.activationPolicy() == NSApplicationActivationPolicy::Prohibited {
            return None;
        }
        Some(AppFacts {
            name: app.localizedName()?.to_string(),
            bundle_id: app.bundleIdentifier().map(|id| id.to_string()),
        })
    }

    /// The process macOS holds responsible for `pid`, `None` when the symbol is missing
    /// or the kernel wouldn't say.
    pub(super) fn responsible_pid(pid: u32) -> Option<u32> {
        static SYMBOL: std::sync::OnceLock<Option<ResponsibleFor>> = std::sync::OnceLock::new();
        let responsible_for = (*SYMBOL.get_or_init(|| {
            // SAFETY: `RTLD_DEFAULT` searches the images already loaded, and the name is a
            // NUL-terminated C string literal. A missing symbol answers null, checked below.
            let symbol =
                unsafe { libc::dlsym(libc::RTLD_DEFAULT, c"responsibility_get_pid_responsible_for_pid".as_ptr()) };
            if symbol.is_null() {
                log::debug!(target: "eject", "This macOS doesn't export responsibility_get_pid_responsible_for_pid, so a helper can't name the app behind it");
                return None;
            }
            // SAFETY: the symbol resolved is that function, whose signature is
            // `ResponsibleFor`'s. A pointer to a function and a function pointer have the
            // same size and representation on every platform Cmdr builds for.
            Some(unsafe { std::mem::transmute::<*mut c_void, ResponsibleFor>(symbol) })
        }))?;
        let pid = libc::pid_t::try_from(pid).ok()?;
        // SAFETY: the resolved symbol takes a pid by value and answers one; it touches
        // no memory of ours.
        let responsible = unsafe { responsible_for(pid) };
        u32::try_from(responsible).ok()
    }

    /// The display name `pid`'s signature seals, `None` when it has no `Info.plist` or
    /// no name in it.
    ///
    /// ❗ Reads the executable. Every caller has to have ruled out an executable on the
    /// drive being ejected first.
    pub(super) fn signing_display_name(pid: u32) -> Option<String> {
        let information = signing_information(pid)?;
        // SAFETY: the key is Security's own `extern "C"` global, read by value.
        let plist = value(&information, unsafe { kSecCodeInfoPList })?.downcast::<CFDictionary>()?;
        ["CFBundleDisplayName", "CFBundleName"].into_iter().find_map(|key| {
            let key = CFString::new(key);
            Some(
                value(&plist, key.as_concrete_TypeRef())?
                    .downcast::<CFString>()?
                    .to_string(),
            )
        })
    }

    /// Whether `pid` runs an Apple platform binary, `None` when its signature couldn't be
    /// read at all.
    ///
    /// ❗ Reads the executable, same rule as [`signing_display_name`].
    pub(super) fn is_platform_binary(pid: u32) -> Option<bool> {
        let information = signing_information(pid)?;
        // SAFETY: the key is Security's own `extern "C"` global, read by value.
        Some(value(&information, unsafe { kSecCodeInfoPlatformIdentifier }).is_some())
    }

    /// What `pid`'s signature says about it, `None` when the kernel has no code object
    /// for it or Security wouldn't answer.
    fn signing_information(pid: u32) -> Option<CFDictionary> {
        let pid = libc::pid_t::try_from(pid).ok()?;
        let mut attributes = GuestAttributes::new();
        attributes.set_pid(pid);
        // The code signing root of trust is the kernel, which is what `None` asks for.
        let code = SecCode::copy_guest_with_attribues(None, &attributes, Flags::NONE).ok()?;
        let mut information: CFDictionaryRef = std::ptr::null();
        // SAFETY: `code` is a live `SecCodeRef`, which this call accepts in place of a
        // `SecStaticCodeRef` (`SecCode.h:352`), and `information` is a valid out pointer
        // the call only writes a retained dictionary into. Nothing unwinds across the
        // boundary.
        let read =
            unsafe { SecCodeCopySigningInformation(code.as_CFTypeRef(), 0, std::ptr::from_mut(&mut information)) };
        if read != 0 || information.is_null() {
            return None;
        }
        // SAFETY: the dictionary came back under the Create rule, which the wrapper
        // balances when it drops.
        Some(unsafe { CFDictionary::wrap_under_create_rule(information) })
    }

    /// One value out of a Core Foundation dictionary, retained for the caller.
    fn value(dictionary: &CFDictionary, key: CFStringRef) -> Option<CFType> {
        let found = dictionary.find(key.cast::<c_void>())?;
        // SAFETY: the dictionary owns the value (the Get rule), and it outlives this
        // borrow; `wrap_under_get_rule` takes a reference of its own.
        Some(unsafe { CFType::wrap_under_get_rule(*found) })
    }

    /// The executable behind `pid`, `None` when the process has gone or isn't visible.
    /// The walk's own `proc_pidpath` wrapper, so one reading serves both.
    pub(super) fn executable_path(pid: u32) -> Option<PathBuf> {
        super::super::scan::executable_path(pid).map(PathBuf::from)
    }
}

#[cfg(not(target_os = "macos"))]
mod platform {
    //! Linux has no holder walk, so nothing ever reaches these. They exist so the rules
    //! above have ONE shape on every platform: a fact nobody could gather reads as
    //! "couldn't tell", which is what the kinds already mean.

    use std::path::PathBuf;

    use super::AppFacts;

    pub(super) fn within_pool<T>(work: impl FnOnce() -> T) -> T {
        work()
    }

    pub(super) fn lineage(pid: u32) -> Vec<u32> {
        vec![pid]
    }

    pub(super) fn running_app(_pid: u32) -> Option<AppFacts> {
        None
    }

    pub(super) fn responsible_pid(_pid: u32) -> Option<u32> {
        None
    }

    pub(super) fn signing_display_name(_pid: u32) -> Option<String> {
        None
    }

    pub(super) fn is_platform_binary(_pid: u32) -> Option<bool> {
        None
    }

    pub(super) fn executable_path(_pid: u32) -> Option<PathBuf> {
        None
    }
}
