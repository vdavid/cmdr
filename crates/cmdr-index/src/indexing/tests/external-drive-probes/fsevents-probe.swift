// Minimal FSEvents LIVE-delivery probe. Watches argv[1] for ~5s; prints
// "FSEVENT-FIRED <path>" on the first callback, then exits 0. Exits 42 if no
// event arrives before the runloop deadline. C-convention callback + a global
// flag (no Swift-closure-as-C-callback bridging, which is what crashed before).
import CoreServices
import Foundation

let watchPath = CommandLine.arguments.count > 1 ? CommandLine.arguments[1] : "/"

// C-convention callback: no captures, writes a global. Safe as an FSEvents callback.
func cb(_ stream: ConstFSEventStreamRef,
        _ info: UnsafeMutableRawPointer?,
        _ numEvents: Int,
        _ eventPaths: UnsafeMutableRawPointer,
        _ flags: UnsafePointer<FSEventStreamEventFlags>,
        _ ids: UnsafePointer<FSEventStreamEventId>) {
    let paths = unsafeBitCast(eventPaths, to: NSArray.self) as? [String] ?? []
    print("FSEVENT-FIRED \(paths.first ?? "?")")
    fflush(stdout)
    exit(0)
}

var ctx = FSEventStreamContext()
let pathsToWatch = [watchPath] as CFArray
guard let stream = FSEventStreamCreate(
    kCFAllocatorDefault, cb, &ctx, pathsToWatch,
    FSEventStreamEventId(kFSEventStreamEventIdSinceNow),
    0.2, // latency
    FSEventStreamCreateFlags(kFSEventStreamCreateFlagFileEvents | kFSEventStreamCreateFlagNoDefer | kFSEventStreamCreateFlagUseCFTypes)
) else {
    print("FSEVENT-CREATE-FAILED")
    exit(1)
}

FSEventStreamScheduleWithRunLoop(stream, CFRunLoopGetCurrent(), CFRunLoopMode.defaultMode.rawValue)
guard FSEventStreamStart(stream) else {
    print("FSEVENT-START-FAILED")
    exit(1)
}
print("WATCHING \(watchPath)")
fflush(stdout)

// Bound the wait: run the loop ~5s, then declare no-event.
CFRunLoopRunInMode(CFRunLoopMode.defaultMode, 5.0, false)
print("NO-FSEVENT-WITHIN-DEADLINE")
fflush(stdout)
exit(42)
