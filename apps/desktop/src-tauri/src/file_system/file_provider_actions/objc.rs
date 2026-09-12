//! The private half: FileProvider.framework's host side, driven the way Finder drives it.
//!
//! - `+[FPProviderDomain providerDomainsWithError:]` lists every domain with its storage
//!   roots and its extension bundle (2.4 ms for four domains).
//! - `-[FPItemManager fetchItemForURL:completionHandler:]` resolves a path to an `FPItem`.
//! - `+[FPProviderDomain fetchProviderDomainWithID:completionHandler:]` brings the domain's
//!   live `domainUserInfo`, which rules read (Dropbox mirrors its own "Right-click
//!   actions" settings there).
//! - `NSPredicate` evaluates each declared rule against `{fileproviderItems,
//!   domainUserInfo}`, which matched `fileproviderctl evaluate` exactly on a Dropbox file,
//!   a Dropbox folder, and a streamed Drive folder.
//! - A click builds an `FPVendorDefinedActionOperation` and hands it to
//!   `-[FPItemManager scheduleAction:]`; the provider's extension does the rest, usually
//!   in a window of its own.
//!
//! Verified on macOS 26.6.2 with Dropbox 270.3.3261, Google Drive for desktop 130.0, and
//! MacDroid 2.10, from an unsigned, unentitled process, 2026-09-12. The nearer-looking doors
//! are shut: `FPItemManager.operationForAction:items:` builds operations for system actions
//! only, and FileProviderUI's `FPUIActionController` wants a UI extension no provider ships.
//!
//! **Private API, so everything fails closed.** The framework, every class, and every
//! selector are looked up at runtime, every call runs inside `objc2::exception::catch`,
//! and every wait is bounded. A macOS or provider update that moves any of it takes the
//! menu group away; it never crashes.
//!
//! **Every block handed to File Provider carries its type signature**
//! (`RcBlock::with_encoding`, ❌ never `RcBlock::new`). File Provider wraps a completion
//! handler with `__FPMakeAsyncCompletionBlock`, which forwards through `_Block_signature`;
//! a signature-less block becomes a nil handler inside File Provider, and the reply then
//! crashes the app with `EXC_BAD_ACCESS` at `0x10`. Verified with lldb on Darwin 25.6.0,
//! 2026-09-11.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::CStr;
use std::panic::AssertUnwindSafe;
use std::path::{Path, PathBuf};
use std::rc::Rc;
use std::sync::mpsc;
use std::sync::{Arc, LazyLock, Mutex, OnceLock};
use std::time::Instant;

use block2::{ManualBlockEncoding, RcBlock};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject};
use objc2::{Message, msg_send, sel};
use objc2_foundation::{NSArray, NSBundle, NSDictionary, NSError, NSPredicate, NSString, NSURL, ns_string};

use super::LOG_TARGET;
use super::declarations::Declarations;
use crate::ignore_poison::IgnorePoison;

const FILE_PROVIDER_FRAMEWORK: &CStr = c"/System/Library/Frameworks/FileProvider.framework/FileProvider";

/// A Foundation object carried from File Provider's reply queue to the thread waiting on it.
pub(super) struct CrossThread<T: ?Sized + Message>(pub(super) Retained<T>);

// SAFETY: it only ever carries what File Provider hands a completion block here, an
// `FPItem` or a domain's `domainUserInfo` dictionary. Both are immutable snapshots decoded
// from the XPC reply that nothing mutates afterwards, so reading one on the waiting thread
// races nothing, and `Retained`'s retain and release are atomic.
unsafe impl<T: ?Sized + Message> Send for CrossThread<T> {}

/// The type signature of every completion handler used here, `(id, NSError *) -> void`.
struct ObjectOrErrorReply;

// SAFETY: `v24@?0@8@16` is a void block taking two object pointers, exactly the
// `(*mut AnyObject, *mut NSError) -> ()` this encoding is declared for.
unsafe impl ManualBlockEncoding for ObjectOrErrorReply {
    type Arguments = (*mut AnyObject, *mut NSError);
    type Return = ();
    const ENCODING_CSTR: &'static CStr = c"v24@?0@8@16";
}

type Reply = RcBlock<dyn Fn(*mut AnyObject, *mut NSError)>;

/// A completion handler, with its signature, that hands `handler` the object or nil.
fn reply_block(handler: impl Fn(Option<&AnyObject>) + 'static) -> Reply {
    RcBlock::with_encoding::<_, _, _, ObjectOrErrorReply>(move |object: *mut AnyObject, _error: *mut NSError| {
        // SAFETY: File Provider passes nil or an object it keeps alive for the block's call.
        handler(unsafe { object.as_ref() });
    })
}

/// Loads FileProvider.framework once. Cmdr doesn't link it, and its private classes only
/// resolve by name once it's loaded.
fn framework_loaded() -> bool {
    static LOADED: OnceLock<bool> = OnceLock::new();
    *LOADED.get_or_init(|| {
        // SAFETY: `dlopen` is thread-safe and the `c""` literal is NUL-terminated. The handle
        // is never closed on purpose: the classes it registers have to outlive every call here.
        let handle = unsafe { libc::dlopen(FILE_PROVIDER_FRAMEWORK.as_ptr(), libc::RTLD_NOW) };
        !handle.is_null()
    })
}

/// `FPItemManager.defaultManager`, when the class still has every selector used here.
fn item_manager() -> Option<Retained<AnyObject>> {
    if !framework_loaded() {
        return None;
    }
    let class = AnyClass::get(c"FPItemManager")?;
    let shaped = class.metaclass().responds_to(sel!(defaultManager))
        && class.responds_to(sel!(fetchItemForURL:completionHandler:))
        && class.responds_to(sel!(scheduleAction:));
    if !shaped {
        return None;
    }
    // SAFETY: `+defaultManager` exists on the class (checked above), takes no arguments, and
    // returns an object; a nil answer becomes `None`.
    unsafe { msg_send![class, defaultManager] }
}

/// `FPProviderDomain`, when the framework loaded and the class still answers `selector`.
fn provider_domain_class(selector: objc2::runtime::Sel) -> Option<&'static AnyClass> {
    if !framework_loaded() {
        return None;
    }
    let class = AnyClass::get(c"FPProviderDomain")?;
    class.metaclass().responds_to(selector).then_some(class)
}

/// One File Provider domain, as the registry needs it.
pub(super) struct ListedDomain {
    pub id: String,
    pub provider_id: String,
    /// Where the domain's files live, as File Provider reports them (`storageURLs`).
    pub roots: Vec<PathBuf>,
    /// The extension bundle whose `Info.plist` declares the actions.
    pub extension: PathBuf,
}

/// Every domain `fileproviderd` knows. A synchronous XPC call, so ❌ never on the menu path:
/// the registry refresh runs it on a thread of its own.
pub(super) fn list_domains() -> Option<Vec<ListedDomain>> {
    let class = provider_domain_class(sel!(providerDomainsWithError:))?;
    let listed = objc2::exception::catch(AssertUnwindSafe(|| {
        // SAFETY: `+providerDomainsWithError:` exists on the class (checked above). Its one
        // argument is an optional error out-pointer, null here, and it returns an array or nil.
        let domains: Option<Retained<NSArray>> =
            unsafe { msg_send![class, providerDomainsWithError: std::ptr::null_mut::<*mut AnyObject>()] };
        domains.map(|domains| domains.iter().filter_map(|domain| listed_domain(&domain)).collect())
    }));
    match listed {
        Ok(domains) => domains,
        Err(exception) => {
            log::debug!(target: LOG_TARGET, "listing File Provider domains threw: {exception:?}");
            None
        }
    }
}

fn listed_domain(domain: &AnyObject) -> Option<ListedDomain> {
    let class = domain.class();
    let shaped = [
        sel!(identifier),
        sel!(providerID),
        sel!(storageURLs),
        sel!(extensionBundleURL),
    ]
    .into_iter()
    .all(|selector| class.responds_to(selector));
    if !shaped {
        return None;
    }
    // SAFETY: `identifier` exists on the domain's class (checked above) and returns an object or nil.
    let id: Option<Retained<AnyObject>> = unsafe { msg_send![domain, identifier] };
    // SAFETY: `providerID` exists on the domain's class (checked above) and returns an object or nil.
    let provider_id: Option<Retained<AnyObject>> = unsafe { msg_send![domain, providerID] };
    // SAFETY: `storageURLs` exists on the domain's class (checked above) and returns an object or nil.
    let roots: Option<Retained<AnyObject>> = unsafe { msg_send![domain, storageURLs] };
    // SAFETY: `extensionBundleURL` exists on the domain's class (checked above) and returns an object or nil.
    let extension: Option<Retained<AnyObject>> = unsafe { msg_send![domain, extensionBundleURL] };
    let roots = roots
        .as_deref()
        .and_then(|roots| roots.downcast_ref::<NSArray>())
        .map(|urls| urls.iter().filter_map(|url| path_of(&url)).collect())
        .unwrap_or_default();
    Some(ListedDomain {
        id: string_of(id.as_deref())?,
        provider_id: string_of(provider_id.as_deref())?,
        roots,
        extension: extension.as_deref().and_then(path_of)?,
    })
}

/// One resolved row: the item itself for the rules, plus its address for a click.
pub(super) struct FetchedItem {
    pub object: CrossThread<AnyObject>,
    pub domain_id: String,
    pub identifier: String,
}

/// Resolves every path to a File Provider item, concurrently, by `deadline`. `None` unless
/// every one of them resolves in time: a provider's actions apply to the whole selection
/// or to none of it.
pub(super) fn fetch_items(paths: &[PathBuf], deadline: Instant) -> Option<Vec<FetchedItem>> {
    let manager = item_manager()?;
    let (tx, rx) = mpsc::channel::<(usize, Option<FetchedItem>)>();
    // Ours stay alive until the wait ends. File Provider keeps a copy of its own once the
    // block has a signature; holding ours too costs nothing and leaves no doubt.
    let mut replies = Vec::with_capacity(paths.len());
    for (index, path) in paths.iter().enumerate() {
        let tx = tx.clone();
        let reply = reply_block(move |item| {
            let fetched = item.and_then(|item| {
                objc2::exception::catch(AssertUnwindSafe(|| read_item(item)))
                    .ok()
                    .flatten()
            });
            // A failed send means the caller stopped waiting (its deadline), which it already handles.
            let _ = tx.send((index, fetched));
        });
        let url = NSURL::fileURLWithPath(&NSString::from_str(path.to_str()?));
        let started = objc2::exception::catch(AssertUnwindSafe(|| {
            // SAFETY: `fetchItemForURL:completionHandler:` exists on the manager's class (checked
            // in `item_manager`). `url` is a live `NSURL`, and the block is the handler's
            // `(FPItem *, NSError *)` shape, signature included.
            let () = unsafe { msg_send![&*manager, fetchItemForURL: &*url, completionHandler: &*reply] };
        }));
        replies.push(reply);
        if let Err(exception) = started {
            log::debug!(target: LOG_TARGET, "File Provider item fetch threw: {exception:?}");
            return None;
        }
    }
    drop(tx);

    let mut fetched: Vec<Option<FetchedItem>> = std::iter::repeat_with(|| None).take(paths.len()).collect();
    for _ in paths {
        let remaining = deadline.checked_duration_since(Instant::now())?;
        let (index, item) = rx.recv_timeout(remaining).ok()?;
        *fetched.get_mut(index)? = Some(item?);
    }
    drop(replies);
    fetched.into_iter().collect()
}

/// Reads the address off a live `FPItem`. Called inside `exception::catch`.
fn read_item(item: &AnyObject) -> Option<FetchedItem> {
    let class = item.class();
    if !class.responds_to(sel!(providerDomainID)) || !class.responds_to(sel!(itemIdentifier)) {
        return None;
    }
    // SAFETY: `providerDomainID` exists on the item's class (checked above) and returns an object or nil.
    let domain: Option<Retained<AnyObject>> = unsafe { msg_send![item, providerDomainID] };
    // SAFETY: `itemIdentifier` exists on the item's class (checked above) and returns an object or nil.
    let identifier: Option<Retained<AnyObject>> = unsafe { msg_send![item, itemIdentifier] };
    Some(FetchedItem {
        domain_id: string_of(domain.as_deref())?,
        identifier: string_of(identifier.as_deref())?,
        object: CrossThread(item.retain()),
    })
}

/// The domain's live `domainUserInfo`, by `deadline`. `None` when the domain didn't answer,
/// which leaves the group out: rules read it, and guessing it would show the wrong actions.
pub(super) fn fetch_domain_user_info(domain_id: &str, deadline: Instant) -> Option<CrossThread<AnyObject>> {
    let class = provider_domain_class(sel!(fetchProviderDomainWithID:completionHandler:))?;
    let (tx, rx) = mpsc::channel::<Option<CrossThread<AnyObject>>>();
    let reply = reply_block(move |domain| {
        let info = domain.and_then(|domain| {
            objc2::exception::catch(AssertUnwindSafe(|| user_info_of(domain)))
                .ok()
                .flatten()
        });
        // A failed send means the caller stopped waiting (its deadline), which it already handles.
        let _ = tx.send(info);
    });
    let id = NSString::from_str(domain_id);
    let started = objc2::exception::catch(AssertUnwindSafe(|| {
        // SAFETY: `+fetchProviderDomainWithID:completionHandler:` exists on the class (checked
        // above). `id` is a live string, and the block is the handler's
        // `(FPProviderDomain *, NSError *)` shape, signature included.
        let () = unsafe { msg_send![class, fetchProviderDomainWithID: &*id, completionHandler: &*reply] };
    }));
    if let Err(exception) = started {
        log::debug!(target: LOG_TARGET, "File Provider domain fetch threw: {exception:?}");
        return None;
    }
    let remaining = deadline.checked_duration_since(Instant::now())?;
    let info = rx.recv_timeout(remaining).ok().flatten();
    drop(reply);
    info
}

/// A domain's `domainUserInfo`, or an empty dictionary when it carries none: every key
/// then reads as missing, the same as Finder sees it.
fn user_info_of(domain: &AnyObject) -> Option<CrossThread<AnyObject>> {
    if !domain.class().responds_to(sel!(domainUserInfo)) {
        return None;
    }
    // SAFETY: `domainUserInfo` exists on the domain's class (checked above) and returns a
    // dictionary or nil.
    let info: Option<Retained<AnyObject>> = unsafe { msg_send![domain, domainUserInfo] };
    let info =
        info.unwrap_or_else(|| Retained::into_super(Retained::into_super(NSDictionary::<AnyObject, AnyObject>::new())));
    Some(CrossThread(info))
}

type ParsedRules = Rc<Vec<Option<Retained<NSPredicate>>>>;

thread_local! {
    /// Parsed rules per extension bundle, version, and declaration count, so a right-click
    /// parses a provider's rules once rather than every time (Dropbox's 25 cost most of the
    /// 9–47 ms a first evaluation took). Per thread because `NSPredicate` isn't `Send`; the
    /// menu always evaluates on the main thread.
    static PARSED_RULES: RefCell<HashMap<(PathBuf, String, usize), ParsedRules>> = RefCell::new(HashMap::new());
}

/// The positions of the declarations whose rule holds for `items` in their domain.
///
/// A rule that fails to parse or throws while evaluating (a key path these items don't
/// have) is that rule's miss, never the menu's.
pub(super) fn matching_actions(
    extension: &Path,
    declarations: &Declarations,
    items: &[FetchedItem],
    domain_user_info: &AnyObject,
) -> Vec<usize> {
    let rules = parsed_rules(extension, declarations);
    let objects: Vec<&AnyObject> = items.iter().map(|item| &*item.object.0).collect();
    let evaluated = objc2::exception::catch(AssertUnwindSafe(|| {
        let array = NSArray::from_slice(&objects);
        let array: &AnyObject = &array;
        let context = NSDictionary::<NSString, AnyObject>::from_slices(
            &[ns_string!("fileproviderItems"), ns_string!("domainUserInfo")],
            &[array, domain_user_info],
        );
        let context: &AnyObject = &context;
        rules
            .iter()
            .enumerate()
            .filter_map(|(index, rule)| {
                let rule = rule.as_ref()?;
                let matched = objc2::exception::catch(AssertUnwindSafe(|| {
                    // SAFETY: `evaluateWithObject:` accepts any object, and the context is a live
                    // dictionary of the two keys the rules read.
                    unsafe { rule.evaluateWithObject(Some(context)) }
                }));
                matches!(matched, Ok(true)).then_some(index)
            })
            .collect()
    }));
    evaluated.unwrap_or_default()
}

fn parsed_rules(extension: &Path, declarations: &Declarations) -> ParsedRules {
    let key = (
        extension.to_path_buf(),
        declarations.version.clone(),
        declarations.actions.len(),
    );
    PARSED_RULES.with(|cache| {
        cache
            .borrow_mut()
            .entry(key)
            .or_insert_with(|| {
                Rc::new(
                    declarations
                        .actions
                        .iter()
                        .map(|action| parse_rule(&action.rule))
                        .collect(),
                )
            })
            .clone()
    })
}

fn parse_rule(rule: &str) -> Option<Retained<NSPredicate>> {
    let format = NSString::from_str(rule);
    objc2::exception::catch(AssertUnwindSafe(|| {
        // SAFETY: no argument array is passed, so no `%@` needs a typed argument; a malformed
        // format throws, which the catch turns into `None`.
        unsafe { NSPredicate::predicateWithFormat_argumentArray(&format, None) }
    }))
    .ok()
}

type LabelTables = Arc<Vec<HashMap<String, String>>>;

/// Label tables keyed by extension bundle, bundle version, and UI locale.
type LabelTableCache = Mutex<HashMap<(PathBuf, String, String), LabelTables>>;

/// The extension's `Localizable.strings` tables, most preferred first: `locale`'s best
/// match, then English. Cached per bundle, version, and locale.
pub(super) fn label_tables(extension: &Path, version: &str, locale: &str) -> LabelTables {
    static CACHE: LazyLock<LabelTableCache> = LazyLock::new(Default::default);
    let key = (extension.to_path_buf(), version.to_string(), locale.to_string());
    if let Some(tables) = CACHE.lock_ignore_poison().get(&key) {
        return tables.clone();
    }
    let tables: LabelTables = Arc::new(load_label_tables(extension, locale));
    CACHE.lock_ignore_poison().insert(key, tables.clone());
    tables
}

fn load_label_tables(extension: &Path, locale: &str) -> Vec<HashMap<String, String>> {
    let loaded = objc2::exception::catch(AssertUnwindSafe(|| {
        let url = NSURL::fileURLWithPath(&NSString::from_str(extension.to_str()?));
        let bundle = NSBundle::bundleWithURL(&url)?;
        let available = bundle.localizations();
        // `NSBundle` does the matching (script, region, and fallback rules included), the same
        // way the provider's own app picks its language.
        let mut picks: Vec<Retained<NSString>> = Vec::new();
        for preference in [locale, "en"] {
            let preferences = NSArray::from_retained_slice(&[NSString::from_str(preference)]);
            let preferences: &NSArray<NSString> = &preferences;
            let pick =
                NSBundle::preferredLocalizationsFromArray_forPreferences(&available, Some(preferences)).firstObject();
            if let Some(pick) = pick
                && !picks.contains(&pick)
            {
                picks.push(pick);
            }
        }
        Some(picks.iter().filter_map(|pick| strings_table(&bundle, pick)).collect())
    }));
    loaded.ok().flatten().unwrap_or_default()
}

/// One localization's `Localizable.strings` as a map. `NSDictionary` reads every format the
/// providers ship (Dropbox's is a binary plist, Drive's UTF-8 text, MacDroid's UTF-16 text).
fn strings_table(bundle: &NSBundle, localization: &NSString) -> Option<HashMap<String, String>> {
    let path = bundle.pathForResource_ofType_inDirectory_forLocalization(
        Some(ns_string!("Localizable")),
        Some(ns_string!("strings")),
        None,
        Some(localization),
    )?;
    let url = NSURL::fileURLWithPath(&path);
    // SAFETY: a `.strings` file loads as a dictionary keyed by strings; a value that isn't a
    // string is filtered out below rather than trusted.
    let table = unsafe { NSDictionary::<NSString, AnyObject>::dictionaryWithContentsOfURL_error(&url) }.ok()?;
    let (keys, values) = table.to_vecs();
    Some(
        keys.iter()
            .zip(values)
            .filter_map(|(key, value)| Some((key.to_string(), value.downcast_ref::<NSString>()?.to_string())))
            .collect(),
    )
}

/// Why a click didn't start its action. Logged, since the user sees only nothing happening.
#[derive(Debug)]
pub(super) enum ScheduleError {
    /// FileProvider.framework, `FPItemManager`, or `FPVendorDefinedActionOperation` is
    /// missing, or no longer has the selectors called here.
    ApiUnavailable,
    /// An Objective-C exception came out of a private call.
    Exception,
}

/// Hands `action_identifier` for `item_identifiers` in `provider_domain_id` to File Provider.
///
/// Returns once the operation is scheduled. The operation's completion block isn't used:
/// its signature is private, and guessing an argument we'd dereference could crash the app
/// over an action whose result the provider shows for itself anyway.
pub(super) fn schedule(
    action_identifier: &str,
    provider_domain_id: &str,
    item_identifiers: &[String],
) -> Result<(), ScheduleError> {
    let scheduled = objc2::exception::catch(AssertUnwindSafe(|| -> Result<(), ScheduleError> {
        let manager = item_manager().ok_or(ScheduleError::ApiUnavailable)?;
        let class = AnyClass::get(c"FPVendorDefinedActionOperation").ok_or(ScheduleError::ApiUnavailable)?;
        if !class.responds_to(sel!(initWithActionIdentifier:providerDomainID:itemIdentifiers:)) {
            return Err(ScheduleError::ApiUnavailable);
        }
        let action = NSString::from_str(action_identifier);
        let domain = NSString::from_str(provider_domain_id);
        let identifiers: Vec<Retained<NSString>> = item_identifiers
            .iter()
            .map(String::as_str)
            .map(NSString::from_str)
            .collect();
        let identifiers = NSArray::from_retained_slice(&identifiers);
        // SAFETY: `+alloc` is NSObject's, sent to a class that exists (looked up above).
        let allocated: Allocated<AnyObject> = unsafe { msg_send![class, alloc] };
        // SAFETY: the initializer exists on the class (checked above) and takes three objects,
        // all live for the call: the action id, the provider domain id, and the item ids. A nil
        // answer becomes `None`.
        let operation: Option<Retained<AnyObject>> = unsafe {
            msg_send![allocated, initWithActionIdentifier: &*action, providerDomainID: &*domain, itemIdentifiers: &*identifiers]
        };
        let operation = operation.ok_or(ScheduleError::ApiUnavailable)?;
        // SAFETY: `scheduleAction:` exists on the manager's class (checked in `item_manager`) and
        // takes the operation, which is live for the call.
        let () = unsafe { msg_send![&*manager, scheduleAction: &*operation] };
        Ok(())
    }));
    scheduled.unwrap_or(Err(ScheduleError::Exception))
}

fn string_of(object: Option<&AnyObject>) -> Option<String> {
    object?.downcast_ref::<NSString>().map(NSString::to_string)
}

fn path_of(url: &AnyObject) -> Option<PathBuf> {
    url.downcast_ref::<NSURL>()?
        .path()
        .map(|path| PathBuf::from(path.to_string()))
}
