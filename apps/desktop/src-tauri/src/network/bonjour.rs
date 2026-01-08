//! Bonjour (mDNS/DNS-SD) discovery implementation using NSNetServiceBrowser.
//!
//! Uses Apple's Foundation framework to discover SMB services on the local network.
//! The browser listens for `_smb._tcp.local` service advertisements and notifies
//! when hosts appear or disappear.
//!
//! After discovery, services are resolved to get their actual IP addresses via mDNS.
//!
//! Note: NSNetServiceBrowser is deprecated by Apple in favor of Network.framework's nw_browser_t,
//! but it still works and is the simplest option for mDNS discovery from Rust.

// Suppress deprecation warnings for NSNetService* APIs - they're deprecated but still work
// Suppress snake_case warnings for ObjC delegate methods that must use camelCase
#![allow(deprecated, non_snake_case)]

use crate::network::{
    DiscoveryState, NetworkHost, on_discovery_state_changed, on_host_found, on_host_lost, on_host_resolved,
    service_name_to_id,
};
use log::{info, warn};
use objc2::rc::Retained;
use objc2::runtime::ProtocolObject;
use objc2::{DefinedClass, MainThreadMarker, MainThreadOnly, define_class, msg_send};
use objc2_foundation::{
    NSArray, NSData, NSDefaultRunLoopMode, NSNetService, NSNetServiceBrowser, NSNetServiceBrowserDelegate,
    NSNetServiceDelegate, NSObject, NSObjectProtocol, NSRunLoop, NSString,
};
use std::cell::RefCell;
use std::collections::HashMap;
use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
use std::sync::{Mutex, OnceLock};
use tauri::AppHandle;

/// SMB service type for Bonjour discovery.
const SMB_SERVICE_TYPE: &str = "_smb._tcp.";
/// Local domain for Bonjour discovery.
const LOCAL_DOMAIN: &str = "local.";
/// Default SMB port.
const SMB_DEFAULT_PORT: u16 = 445;
/// Timeout for service resolution in seconds.
const RESOLVE_TIMEOUT: f64 = 5.0;

/// Global Bonjour discovery manager.
static BONJOUR_MANAGER: OnceLock<Mutex<Option<BonjourManager>>> = OnceLock::new();

/// Manager for Bonjour discovery lifecycle.
struct BonjourManager {
    browser: Retained<NSNetServiceBrowser>,
    // Keep delegate alive - the browser holds a weak reference
    _delegate: Retained<BonjourDelegate>,
    // Keep resolving services and their delegates alive
    resolving_services: HashMap<String, (Retained<NSNetService>, Retained<ServiceResolveDelegate>)>,
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

/// Internal state for the Bonjour browser delegate.
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

            // Create host with no IP yet - will be resolved via NSNetService.resolve()
            let host = NetworkHost {
                id: id.clone(),
                name,
                hostname: None,   // Will be set after resolution
                ip_address: None, // Will be set after resolution
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

            // Start resolving the service to get hostname and IP
            start_resolving_service(service, &id);
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

            // Stop resolving if in progress
            stop_resolving_service(&id);

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

// --- Service Resolution Delegate ---

/// Internal state for the service resolution delegate.
struct ServiceResolveDelegateIvars {
    /// Host ID for this service.
    host_id: RefCell<String>,
}

define_class!(
    #[unsafe(super(NSObject))]
    #[thread_kind = MainThreadOnly]
    #[name = "RCServiceResolveDelegate"]
    #[ivars = ServiceResolveDelegateIvars]
    struct ServiceResolveDelegate;

    unsafe impl NSObjectProtocol for ServiceResolveDelegate {}

    unsafe impl NSNetServiceDelegate for ServiceResolveDelegate {
        #[unsafe(method(netServiceDidResolveAddress:))]
        fn netServiceDidResolveAddress(&self, service: &NSNetService) {
            let host_id = self.ivars().host_id.borrow().clone();

            // Get hostname
            let hostname = service.hostName().map(|h| h.to_string());

            // Extract IP addresses from the resolved service
            let ip_address = extract_ip_from_service(service);

            // Get port
            let port = {
                let raw_port = service.port();
                if raw_port > 0 {
                    raw_port as u16
                } else {
                    SMB_DEFAULT_PORT
                }
            };

            info!(
                "Bonjour resolved {}: hostname={:?}, ip={:?}, port={}",
                host_id, hostname, ip_address, port
            );

            // Notify about resolution
            if let Some(app_handle) = get_app_handle() {
                on_host_resolved(&host_id, hostname, ip_address, port, &app_handle);
            }

            // Clean up - remove from resolving set
            stop_resolving_service(&host_id);
        }

        #[unsafe(method(netService:didNotResolve:))]
        fn netService_didNotResolve(&self, _service: &NSNetService, _error_dict: &objc2_foundation::NSDictionary) {
            let host_id = self.ivars().host_id.borrow().clone();
            warn!("Bonjour failed to resolve {}", host_id);

            // Clean up
            stop_resolving_service(&host_id);
        }
    }
);

impl ServiceResolveDelegate {
    fn new(mtm: MainThreadMarker, host_id: String) -> Retained<Self> {
        let this = Self::alloc(mtm).set_ivars(ServiceResolveDelegateIvars {
            host_id: RefCell::new(host_id),
        });
        unsafe { msg_send![super(this), init] }
    }
}

/// Extracts the first usable IP address from a resolved NSNetService.
fn extract_ip_from_service(service: &NSNetService) -> Option<String> {
    // Get addresses array - this returns Option<Retained<NSArray<NSData>>>
    let addresses: Retained<NSArray<NSData>> = service.addresses()?;

    // Iterate through addresses to find an IP, preferring IPv4
    let mut ipv6_addr: Option<String> = None;

    let count = addresses.count();
    for i in 0..count {
        // Get the NSData object
        let data = addresses.objectAtIndex(i);

        // Get the bytes and length from NSData using msg_send
        // (NSData.bytes() and .length are not directly exposed in objc2-foundation)
        let length: usize = unsafe { msg_send![&*data, length] };
        if length < 4 {
            continue;
        }

        let bytes_ptr: *const u8 = unsafe { msg_send![&*data, bytes] };
        if bytes_ptr.is_null() {
            continue;
        }

        // Read the sockaddr structure
        let bytes = unsafe { std::slice::from_raw_parts(bytes_ptr, length) };

        if let Some(ip) = parse_sockaddr(bytes) {
            if ip.is_ipv4() {
                return Some(ip.to_string());
            } else if ipv6_addr.is_none() {
                ipv6_addr = Some(ip.to_string());
            }
        }
    }

    // Return IPv6 if no IPv4 found
    ipv6_addr
}

/// Parses a sockaddr from raw bytes.
fn parse_sockaddr(bytes: &[u8]) -> Option<IpAddr> {
    if bytes.len() < 2 {
        return None;
    }

    // On macOS/BSD: struct sockaddr has sa_len (1 byte) then sa_family (1 byte)
    let family = bytes[1];

    // AF_INET = 2, AF_INET6 = 30 on macOS
    match family {
        2 => {
            // IPv4: struct sockaddr_in is 16 bytes
            // sin_port at offset 2 (2 bytes), sin_addr at offset 4 (4 bytes)
            if bytes.len() >= 8 {
                let ip = Ipv4Addr::new(bytes[4], bytes[5], bytes[6], bytes[7]);
                Some(IpAddr::V4(ip))
            } else {
                None
            }
        }
        30 => {
            // IPv6: struct sockaddr_in6 is 28 bytes
            // sin6_port at offset 2, sin6_flowinfo at 4, sin6_addr at offset 8 (16 bytes)
            if bytes.len() >= 24 {
                let mut addr_bytes = [0u8; 16];
                addr_bytes.copy_from_slice(&bytes[8..24]);
                let ip = Ipv6Addr::from(addr_bytes);
                Some(IpAddr::V6(ip))
            } else {
                None
            }
        }
        _ => None,
    }
}

// --- Service Resolution Management ---

/// Starts resolving a service to get its hostname and IP.
fn start_resolving_service(service: &NSNetService, host_id: &str) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };

    let mut manager_guard = get_bonjour_manager().lock().unwrap();
    let Some(manager) = manager_guard.as_mut() else {
        return;
    };

    // Don't re-resolve if already resolving
    if manager.resolving_services.contains_key(host_id) {
        return;
    }

    // Create a new service instance for resolution (can't reuse the browser's service)
    // Use raw msg_send since NSNetService doesn't implement MainThreadOnly in objc2-foundation
    let domain = NSString::from_str(LOCAL_DOMAIN);
    let service_type = NSString::from_str(SMB_SERVICE_TYPE);
    let service_name = service.name();

    // Allocate and init using raw Objective-C messaging to avoid trait bound issues
    let resolve_service: Retained<NSNetService> = unsafe {
        let cls = objc2::class!(NSNetService);
        let alloc_ptr: *mut NSNetService = msg_send![cls, alloc];
        let init_ptr: *mut NSNetService = msg_send![
            alloc_ptr,
            initWithDomain: &*domain,
            type: &*service_type,
            name: &*service_name,
            port: 0i32
        ];
        // Convert raw pointer to Retained - this takes ownership
        Retained::from_raw(init_ptr).expect("NSNetService init failed")
    };

    // Create delegate
    let delegate = ServiceResolveDelegate::new(mtm, host_id.to_string());

    // Set delegate
    unsafe {
        resolve_service.setDelegate(Some(ProtocolObject::from_ref(&*delegate)));
    }

    // Schedule in run loop
    let run_loop = NSRunLoop::mainRunLoop();
    unsafe {
        resolve_service.scheduleInRunLoop_forMode(&run_loop, NSDefaultRunLoopMode);
    }

    // Start resolution with timeout
    resolve_service.resolveWithTimeout(RESOLVE_TIMEOUT);

    // Store to keep alive
    manager
        .resolving_services
        .insert(host_id.to_string(), (resolve_service, delegate));
}

/// Stops resolving a service and cleans up.
fn stop_resolving_service(host_id: &str) {
    let mut manager_guard = get_bonjour_manager().lock().unwrap();
    let Some(manager) = manager_guard.as_mut() else {
        return;
    };

    if let Some((service, _delegate)) = manager.resolving_services.remove(host_id) {
        service.stop();

        // Remove from run loop
        let run_loop = NSRunLoop::mainRunLoop();
        unsafe {
            service.removeFromRunLoop_forMode(&run_loop, NSDefaultRunLoopMode);
        }
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
        resolving_services: HashMap::new(),
    });
}

/// Stops Bonjour discovery.
#[allow(dead_code)]
pub fn stop_discovery() {
    let mut manager_guard = get_bonjour_manager().lock().unwrap();

    if let Some(manager) = manager_guard.take() {
        manager.browser.stop();

        // Stop all resolving services
        for (_, (service, _)) in manager.resolving_services {
            service.stop();
        }

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

    #[test]
    fn test_parse_sockaddr_ipv4() {
        // sockaddr_in for 192.168.1.150
        let bytes: [u8; 16] = [
            16, // sa_len
            2,  // sa_family = AF_INET
            0x01, 0xBB, // sin_port = 445 (network order)
            192, 168, 1, 150, // sin_addr
            0, 0, 0, 0, 0, 0, 0, 0, // padding
        ];

        let ip = parse_sockaddr(&bytes);
        assert_eq!(ip, Some(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 150))));
    }
}
