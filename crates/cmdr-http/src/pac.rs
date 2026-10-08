//! PAC files, run by macOS itself (`CFNetworkExecuteProxyAutoConfigurationURL` / `…Script`).
//!
//! Auto proxy discovery (WPAD) arrives here too: with it on, macOS hands over the discovered PAC
//! URL (`http://wpad/wpad.dat` by DNS) as an ordinary PAC entry.
//!
//! The evaluation is asynchronous on a run loop, and reqwest asks synchronously, so each cache
//! miss runs on a short-lived thread of its own with its own run loop, and the asking thread waits
//! at most [`TIMEOUT`]. Answers are cached per PAC and per `scheme://host:port`.

use std::cell::RefCell;
use std::collections::HashMap;
use std::ffi::c_void;
use std::ptr::NonNull;
use std::sync::mpsc;
use std::sync::{LazyLock, Mutex};
use std::time::{Duration, Instant};

use objc2_core_foundation::{CFArray, CFError, CFIndex, CFRetained, CFRunLoop, CFRunLoopSource, CFString, CFURL};
use reqwest::Url;

use crate::entries::cf::{cf_url, entries_from};
use crate::entries::{Entry, Pac, PacSource};

/// How long the asking thread waits for a PAC answer before going direct, the way macOS does
/// when a PAC can't be run. Covers fetching the file, which CFNetwork does itself.
pub(crate) const TIMEOUT: Duration = Duration::from_secs(3);
/// How long an answer stays good. A PAC file rarely changes, and this bounds how long a changed
/// one takes to apply.
const ANSWER_TTL: Duration = Duration::from_secs(5 * 60);
/// How long a failure stays cached, so an unreachable PAC costs one [`TIMEOUT`] per host per
/// half minute rather than one per request.
const FAILURE_TTL: Duration = Duration::from_secs(30);
/// Entries kept before the cache starts over: a few dozen hosts in normal use.
const CACHE_CAP: usize = 256;

/// The process-wide evaluator [`crate::client_builder`]'s clients share.
pub(crate) fn shared() -> &'static MacPac {
    static SHARED: LazyLock<MacPac> = LazyLock::new(|| MacPac::new(TIMEOUT));
    &SHARED
}

type Key = (PacSource, String);
/// An answer (`None` for a failure) and when it was had.
type Cached = (Option<Vec<Entry>>, Instant);

/// A PAC evaluator with a per-host answer cache.
pub(crate) struct MacPac {
    timeout: Duration,
    cache: Mutex<HashMap<Key, Cached>>,
    /// One evaluation at a time: a burst of first connections to one host (an S3 upload's parts)
    /// waits for the first answer instead of running the same PAC N times.
    turn: Mutex<()>,
}

impl MacPac {
    pub(crate) fn new(timeout: Duration) -> Self {
        Self {
            timeout,
            cache: Mutex::new(HashMap::new()),
            turn: Mutex::new(()),
        }
    }

    fn cached(&self, key: &Key) -> Option<Option<Vec<Entry>>> {
        let cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        let (answer, at) = cache.get(key)?;
        let ttl = if answer.is_some() { ANSWER_TTL } else { FAILURE_TTL };
        (at.elapsed() < ttl).then(|| answer.clone())
    }

    fn store(&self, key: Key, answer: Option<Vec<Entry>>) {
        let mut cache = self.cache.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if cache.len() >= CACHE_CAP {
            cache.clear();
        }
        cache.insert(key, (answer, Instant::now()));
    }
}

impl Pac for MacPac {
    fn evaluate(&self, source: &PacSource, url: &Url) -> Option<Vec<Entry>> {
        let key = (source.clone(), url.as_str().to_string());
        if let Some(answer) = self.cached(&key) {
            return answer;
        }
        let _turn = self.turn.lock().unwrap_or_else(std::sync::PoisonError::into_inner);
        if let Some(answer) = self.cached(&key) {
            return answer;
        }
        let answer = evaluate_on_own_thread(source.clone(), url.as_str().to_string(), self.timeout);
        if answer.is_none() {
            log::warn!(target: "cmdr_http", "couldn't run the PAC file for {url}, so it goes direct");
        }
        self.store(key, answer.clone());
        answer
    }
}

/// Runs one evaluation on a fresh thread and waits for it, at most `timeout` plus a margin for
/// the thread to notice its own deadline.
fn evaluate_on_own_thread(source: PacSource, url: String, timeout: Duration) -> Option<Vec<Entry>> {
    let (sender, receiver) = mpsc::channel();
    let spawned = std::thread::Builder::new()
        .name(String::from("cmdr-http-pac"))
        .spawn(move || {
            // The receiver may have given up already; nothing else to tell.
            let _ = sender.send(execute(&source, &url, timeout));
        });
    if spawned.is_err() {
        return None;
    }
    receiver.recv_timeout(timeout + Duration::from_secs(1)).ok().flatten()
}

/// What the callback leaves for [`execute`]: `None` until it fires, then the answer or a failure.
type Slot = RefCell<Option<Option<Vec<Entry>>>>;

/// Runs the PAC on THIS thread's run loop, in a private mode so nothing else on it runs, until
/// the callback fires or `timeout` passes.
fn execute(source: &PacSource, url: &str, timeout: Duration) -> Option<Vec<Entry>> {
    let target = cf_url(url)?;
    let slot: Slot = RefCell::new(None);
    let mut context = StreamClientContext {
        version: 0,
        info: (&raw const slot).cast_mut().cast(),
        retain: None,
        release: None,
        copy_description: None,
    };
    let raw = match source {
        PacSource::Url(pac_url) => {
            let pac_url = cf_url(pac_url)?;
            // SAFETY: both URLs are valid CFURLs for the call; `context` outlives the call (CF copies
            // it), and its `info` points at `slot`, which outlives the run-loop source: the source is
            // invalidated below before `slot` goes out of scope, so the callback can't fire later.
            unsafe { CFNetworkExecuteProxyAutoConfigurationURL(&*pac_url, &*target, on_result, &mut context) }
        }
        PacSource::Script(script) => {
            let script = CFString::from_str(script);
            // SAFETY: as for the URL variant above, with a valid CFString script.
            unsafe { CFNetworkExecuteProxyAutoConfigurationScript(&*script, &*target, on_result, &mut context) }
        }
    };
    // SAFETY: `raw` comes from a CFNetwork `Execute…` call, which returns a +1 reference the
    // caller owns ("Create rule"), or null.
    let run_loop_source = unsafe { CFRetained::from_raw(NonNull::new(raw)?) };
    let deadline = Instant::now() + timeout;
    run_source_until(&run_loop_source, &slot, deadline);
    run_loop_source.invalidate();
    slot.into_inner().flatten()
}

/// Runs `source` on this thread's run loop until `slot` is filled or `deadline` passes.
fn run_source_until(source: &CFRunLoopSource, slot: &Slot, deadline: Instant) {
    let Some(run_loop) = CFRunLoop::current() else { return };
    let mode = CFString::from_static_str("com.veszelovszki.cmdr.pac");
    run_loop.add_source(Some(source), Some(&mode));
    while slot.borrow().is_none() {
        let left = deadline.saturating_duration_since(Instant::now());
        if left.is_zero() {
            break;
        }
        CFRunLoop::run_in_mode(Some(&mode), left.as_secs_f64(), true);
    }
}

/// CFNetwork's result callback. `client` is the `info` from [`execute`]'s context.
extern "C" fn on_result(client: *mut c_void, proxies: *const CFArray, error: *const CFError) {
    // SAFETY: `client` is `&slot` from `execute`, on this same thread's stack: the callback only
    // fires inside `run_source_until` on that thread, before the source is invalidated.
    let slot = unsafe { &*client.cast_const().cast::<Slot>() };
    let answer = if error.is_null() && !proxies.is_null() {
        // SAFETY: non-null, and CFNetwork keeps the list alive for the callback's duration.
        Some(entries_from(unsafe { &*proxies }))
    } else {
        if !error.is_null() {
            // SAFETY: non-null, and CFNetwork keeps the error alive for the callback's duration.
            let error = unsafe { &*error };
            log::debug!(target: "cmdr_http", "PAC evaluation refused: code {}", error.code());
        }
        None
    };
    *slot.borrow_mut() = Some(answer);
}

/// `CFStreamClientContext`, laid out as CFNetwork reads it. No retain or release: `info` is a
/// stack slot whose lifetime [`execute`] guarantees.
#[repr(C)]
struct StreamClientContext {
    version: CFIndex,
    info: *mut c_void,
    retain: Option<extern "C" fn(*mut c_void) -> *mut c_void>,
    release: Option<extern "C" fn(*mut c_void)>,
    copy_description: Option<extern "C" fn(*mut c_void) -> *const CFString>,
}

type ResultCallback = extern "C" fn(*mut c_void, *const CFArray, *const CFError);

// Declared here rather than taken from `objc2-cf-network`: its binding types the callback's
// proxy list as non-null, and CFNetwork passes null when the PAC fails.
#[link(name = "CFNetwork", kind = "framework")]
unsafe extern "C" {
    fn CFNetworkExecuteProxyAutoConfigurationURL(
        proxy_auto_config_url: *const CFURL,
        target_url: *const CFURL,
        callback: ResultCallback,
        client_context: *mut StreamClientContext,
    ) -> *mut CFRunLoopSource;
    fn CFNetworkExecuteProxyAutoConfigurationScript(
        proxy_auto_configuration_script: *const CFString,
        target_url: *const CFURL,
        callback: ResultCallback,
        client_context: *mut StreamClientContext,
    ) -> *mut CFRunLoopSource;
}

#[cfg(test)]
#[path = "pac_test.rs"]
mod tests;
