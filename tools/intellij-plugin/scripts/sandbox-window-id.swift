// Prints `<window-id>\t<window-name>` for every on-screen window whose owner is the sandbox IDE.
//
// Tier 2 screenshots have to name a window: `screencapture -l <id>` grabs the sandbox alone even when it's buried, so
// the picture can never pick up whatever David has open, and the window never has to be raised. No CLI prints window
// IDs and the system Python has no `Quartz`, so this is the shortest path. Run it with `swift <this file>`; it needs
// no project and no build.
//
// Usage from the tier 2 recipe in `DETAILS.md`:
//   ID=$(swift scripts/sandbox-window-id.swift | head -1 | cut -f1)
//   screencapture -x -o -l "$ID" /tmp/tier2.png

import CoreGraphics
import Foundation

let windows = CGWindowListCopyWindowInfo([.optionOnScreenOnly, .excludeDesktopElements], kCGNullWindowID)
    as? [[String: Any]] ?? []

for window in windows {
    // The sandbox runs from the Gradle-managed JBR, so its owner name is the generic "java", not "idea". The window
    // title is what identifies it: the IDE puts the project name in every frame title.
    let name = window[kCGWindowName as String] as? String ?? ""
    guard name.contains("sandbox-project") else { continue }
    guard let id = window[kCGWindowNumber as String] as? Int else { continue }
    print("\(id)\t\(name)")
}
