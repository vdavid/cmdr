//! Bonjour (mDNS/DNS-SD) discovery implementation using NSNetServiceBrowser.
//!
//! Uses Apple's Foundation framework to discover SMB services on the local network.
//! The browser listens for `_smb._tcp.local` service advertisements and notifies
//! when hosts appear or disappear.
//!
//! Note: NSNetServiceBrowser is deprecated by Apple in favor of Network.framework's nw_browser_t,
//! but it still works and is the simplest option for mDNS discovery from Rust.

// Suppress deprecation warnings for NSNetService* APIs - they're deprecated but still work
// Suppress snake_case warnings for ObjC delegate methods that must use camelCase
#![allow(deprecated, non_snake_case)]

use crate::network::{
    DiscoveryState, NetworkHost, on_discovery_state_changed, on_host_found, on_host_lost, service_name_to_id,
};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{
    NSDefaultRunLoopMode, NSNetService, NSNetServiceBrowser, NSNetServiceBrowserDelegate, NSObject, NSObjectProtocol,
    NSRunLoop, NSString,
};
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// SMB service type for Bonjour discovery.
const SMB_SERVICE_TYPE: &str = "_smb._tcp.";
/// Local domain for Bonjour discovery.
const LOCAL_DOMAIN: &str = "local.";
/// Default SMB port.
const SMB_DEFAULT_PORT: u16 = 445;

/// Global Bonjour discovery manager.
static BONJOUR_MANAGER: OnceLock<Mutex<Option<BonjourManager>>> = OnceLock::new();

/// Manager for Bonjour discovery lifecycle.
struct BonjourManager {
    browser: Retained<NSNetServiceBrowser>,
    // Keep delegate alive - the browser holds a weak reference
    _delegate: Retained<BonjourDelegate>,
}

// SAFETY: The BonjourManager is only accessed from the main thread where the run loop runs.
// We need Send to store it in a static Mutex, but actual access is synchronized.
unsafe impl Send for BonjourManager {}

/// Global app handle for sending events.
static APP_HANDLE: OnceLock<Mutex<Option<AppHandle>>> = OnceLock::new();

fn get_app_handle() -> Option<AppHandle> {
    APP_HANDLE
        .get()
        .and_then(|m| m.lock().ok())
        .and_then(|guard| guard.clone())
}

fn set_app_handle(handle: AppHandle) {
    let storage = APP_HANDLE.get_or_init(|| Mutex::new(None));
    if let Ok(mut guard) = storage.lock() {
        *guard = Some(handle);
    }
}

/// Internal state for the Bonjour delegate.
struct BonjourDelegateIvars {
    /// Track if we've received the first batch of services (moreComing = false).
    initial_scan_complete: RefCell<bool>,
}

define_class!(
    // SAFETY:
    // - NSObject has no special subclassing requirements.
    // - BonjourDelegate doesn't implement Drop.
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RCBonjourDelegate"]
    #[ivars = BonjourDelegateIvars]
    struct BonjourDelegate;

    unsafe impl NSObjectProtocol for BonjourDelegate {}

    unsafe impl NSNetServiceBrowserDelegate for BonjourDelegate {
        #[unsafe(method(netServiceBrowserWillSearch:))]
        fn netServiceBrowserWillSearch(&self, _browser: &NSNetServiceBrowser) {
            if let Some(app_handle) = get_app_handle() {
                on_discovery_state_changed(DiscoveryState::Searching, &app_handle);
            }
        }

        #[unsafe(method(netServiceBrowserDidStopSearch:))]
        fn netServiceBrowserDidStopSearch(&self, _browser: &NSNetServiceBrowser) {
            if let Some(app_handle) = get_app_handle() {
                on_discovery_state_changed(DiscoveryState::Idle, &app_handle);
            }
        }

        #[unsafe(method(netServiceBrowser:didFindService:moreComing:))]
        fn netServiceBrowser_didFindService_moreComing(
            &self,
            _browser: &NSNetServiceBrowser,
            service: &NSNetService,
            more_coming: bool,
        ) {
            let name = service.name().to_string();
            let id = service_name_to_id(&name);

            // Get port if available (may be 0 if not resolved yet)
            let port = {
                let raw_port = service.port();
                if raw_port > 0 {
                    raw_port as u16
                } else {
                    SMB_DEFAULT_PORT
                }
            };

            let host = NetworkHost {
                id,
                name,
                hostname: None,   // Will be resolved lazily
                ip_address: None, // Will be resolved lazily
                port,
            };

            if let Some(app_handle) = get_app_handle() {
                on_host_found(host, &app_handle);

                // If this is the last service in the current batch, mark initial scan complete
                if !more_coming && !*self.ivars().initial_scan_complete.borrow() {
                    *self.ivars().initial_scan_complete.borrow_mut() = true;
                    on_discovery_state_changed(DiscoveryState::Active, &app_handle);
                }
            }
        }

        #[unsafe(method(netServiceBrowser:didRemoveService:moreComing:))]
        fn netServiceBrowser_didRemoveService_moreComing(
            &self,
            _browser: &NSNetServiceBrowser,
            service: &NSNetService,
            _more_coming: bool,
        ) {
            let name = service.name().to_string();
            let id = service_name_to_id(&name);

            if let Some(app_handle) = get_app_handle() {
                on_host_lost(&id, &app_handle);
            }
        }
    }
);

impl BonjourDelegate {
    fn new(mtm: MainThreadMarker) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(BonjourDelegateIvars {
            initial_scan_complete: RefCell::new(false),
        });
        unsafe { msg_send![super(this), init] }
    }
}

fn get_bonjour_manager() -> &'static Mutex<Option<BonjourManager>> {
    BONJOUR_MANAGER.get_or_init(|| Mutex::new(None))
}

/// Starts Bonjour discovery for SMB hosts.
///
/// This should be called from the main thread during app initialization.
/// Discovery runs continuously in the background, emitting events when hosts
/// appear or disappear on the network.
pub fn start_discovery(app_handle: AppHandle) {
    // Get main thread marker - this will panic if not called from main thread
    let Some(mtm) = MainThreadMarker::new() else {
        eprintln!("[NETWORK] Warning: start_discovery must be called from main thread");
        return;
    };

    let mut manager_guard = get_bonjour_manager().lock().unwrap();

    // Don't start if already running
    if manager_guard.is_some() {
        return;
    }

    // Store app handle for event emission
    set_app_handle(app_handle);

    // Create the browser and delegate on the main thread
    let browser = NSNetServiceBrowser::new();
    let delegate = BonjourDelegate::new(mtm);

    // Set the delegate
    // SAFETY: We keep the delegate alive in BonjourManager
    unsafe {
        browser.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    }

    // Schedule in the main run loop
    let run_loop = NSRunLoop::mainRunLoop();
    unsafe {
        browser.scheduleInRunLoop_forMode(&run_loop, NSDefaultRunLoopMode);
    }

    // Start searching for SMB services
    let service_type = NSString::from_str(SMB_SERVICE_TYPE);
    let domain = NSString::from_str(LOCAL_DOMAIN);
    browser.searchForServicesOfType_inDomain(&service_type, &domain);

    *manager_guard = Some(BonjourManager {
        browser,
        _delegate: delegate,
    });
}

/// Stops Bonjour discovery.
#[allow(dead_code)]
pub fn stop_discovery() {
    let mut manager_guard = get_bonjour_manager().lock().unwrap();

    if let Some(manager) = manager_guard.take() {
        manager.browser.stop();

        // Remove from run loop
        let run_loop = NSRunLoop::mainRunLoop();
        unsafe {
            manager
                .browser
                .removeFromRunLoop_forMode(&run_loop, NSDefaultRunLoopMode);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_constants() {
        assert_eq!(SMB_SERVICE_TYPE, "_smb._tcp.");
        assert_eq!(LOCAL_DOMAIN, "local.");
        assert_eq!(SMB_DEFAULT_PORT, 445);
    }
}
