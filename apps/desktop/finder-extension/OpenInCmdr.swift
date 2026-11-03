// A top-level "Open in Cmdr" in Finder's right-click menu. See CLAUDE.md here.
//
// All this does is call the macOS service the app already declares (`src-tauri/Info.plist`),
// so the service's delivery is the only delivery: launching Cmdr, parking the request until
// the frontend is up, and moving the pane all happen on the far side.

import AppKit
import FinderSync
import os

// `OSLog` with `os_log`, ❌ not `Logger`: `Logger` needs macOS 11, and the x86_64 slice targets 10.15
// (`build.sh`), so it fails to compile there.
private let log = OSLog(subsystem: "com.veszelovszki.cmdr.finder-extension", category: "menu")

/// The service's default `NSMenuItem` title in `src-tauri/Info.plist`. `NSPerformService` finds a
/// service by this text whatever the language, and it's the key of the translations in `lproj/`.
private let serviceName = "Open in Cmdr"

/// The startup disk's root. ❌ Don't list other volumes: each one then shows this app's icon in
/// Finder's sidebar (`DETAILS.md`). The service covers them instead.
private let watchedRoot = URL(fileURLWithPath: "/")

final class OpenInCmdr: FIFinderSync {
    override init() {
        super.init()
        FIFinderSyncController.default().directoryURLs = [watchedRoot]
    }

    override func menu(for menuKind: FIMenuKind) -> NSMenu? {
        guard menuKind == .contextualMenuForItems || menuKind == .contextualMenuForContainer else {
            return nil
        }
        let menu = NSMenu(title: "")
        // In the system's language: macOS picks from this bundle's `lproj` folders.
        let title = Bundle.main.localizedString(forKey: serviceName, value: serviceName, table: nil)
        menu.addItem(withTitle: title, action: #selector(openInCmdr(_:)), keyEquivalent: "")
        return menu
    }

    /// The selection, or the folder the click landed in when it landed on the window's background.
    @objc func openInCmdr(_ sender: AnyObject?) {
        let controller = FIFinderSyncController.default()
        var items = controller.selectedItemURLs() ?? []
        if items.isEmpty, let folder = controller.targetedURL() {
            items = [folder]
        }
        guard !items.isEmpty else { return }

        let pasteboard = NSPasteboard.withUniqueName()
        pasteboard.clearContents()
        pasteboard.writeObjects(items.map { $0 as NSURL })
        // This process outlives the call, and the pasteboard's data is ours until the service has
        // read it, which is why it isn't released here. One unique pasteboard per click is small.
        if !NSPerformService(serviceName, pasteboard) {
            os_log("macOS didn't run the Open in Cmdr service. Has it been switched off in System Settings?", log: log, type: .error)
        }
    }
}
