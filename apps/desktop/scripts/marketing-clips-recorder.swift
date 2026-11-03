// The marketing clips' camera: records one Cmdr window (optionally with more of the
// app's windows composited over it) through ScreenCaptureKit, at the display's native
// scale and up to 60 fps, without the window shadow, into a ProRes 4444 `.mov`.
//
// Driven by `marketing-clips-recorder.ts` (one process per clip take), never by hand:
//
//   cmdr-clip-recorder --window-id 1234 [--include-window-id 5678 ...] --out take.mov
//   cmdr-clip-recorder --preflight
//
// Protocol: one JSON object per stdout line.
//   {"event":"recording","firstFrameEpochMs":…,"width":…,"height":…}  first frame written
//   {"event":"done","frames":…,"durationS":…}                          file finalized
//   {"event":"error","message":…}                                      then a non-zero exit
// Stop it with a `stop` line on stdin, SIGINT, or SIGTERM; all three finalize the file.
//
// Why each choice:
// - **ProRes 4444**, not 422: 4:2:2 halves the chroma, which smears colored text edges in
//   the master that every encode is cut from. Big (~60 MB/s at 2x), and gitignored.
// - **`firstFrameEpochMs`** maps the video's t=0 onto the wall clock, which is what the
//   spec's markers are written in (`Date.now()`), so the encoder can trim to them.
// - **The session ends at the STOP time, not the last frame**: ScreenCaptureKit sends a
//   frame only when the window changes, so a still ending would otherwise be cut short.
// - **The display minus every other window, cropped to the FIRST window's frame**, so a
//   second window (`--include-window-id`) shows where it overlaps the first, and the
//   title bar keeps its traffic lights (`record` says why the window filters don't).
//   Anything else the display shows on top of the window is excluded too.
//
// Compiled with `-swift-version 5`: the delegate callbacks below are queue-confined by
// hand (every writer touch happens on `queue`), which Swift 6's checker can't see.
// macOS 27 deprecates the classic `AVAssetWriterInput` calls used here (`add`,
// `startWriting`, `append`) in favor of input receivers; they still work, and this stays
// on them until the replacement is verified (swiftc 6.4, macOS 27.0.1, 2026-10-22).

import AppKit
import AVFoundation
import CoreGraphics
import CoreMedia
import Foundation
import ScreenCaptureKit

func emit(_ object: [String: Any]) {
    guard let data = try? JSONSerialization.data(withJSONObject: object),
        let line = String(data: data, encoding: .utf8)
    else { return }
    print(line)
    fflush(stdout)
}

func fail(_ message: String) -> Never {
    emit(["event": "error", "message": message])
    exit(1)
}

struct Options {
    var windowId: CGWindowID?
    var includeWindowIds: [CGWindowID] = []
    var out: String?
    var fps: Int32 = 60
    var preflight = false

    static func parse(_ args: [String]) -> Options {
        var options = Options()
        var i = 1
        func value() -> String {
            i += 1
            guard i < args.count else { fail("\(args[i - 1]) needs a value") }
            return args[i]
        }
        while i < args.count {
            switch args[i] {
            case "--window-id": options.windowId = CGWindowID(value()) ?? 0
            case "--include-window-id": options.includeWindowIds.append(CGWindowID(value()) ?? 0)
            case "--out": options.out = value()
            case "--fps": options.fps = Int32(value()) ?? 60
            case "--preflight": options.preflight = true
            default: fail("unknown argument \(args[i])")
            }
            i += 1
        }
        return options
    }
}

/// The host clock's now, the clock ScreenCaptureKit stamps its frames on.
func hostNow() -> CMTime {
    CMClockGetTime(CMClockGetHostTimeClock())
}

final class Recorder: NSObject, SCStreamOutput, SCStreamDelegate {
    let queue = DispatchQueue(label: "cmdr.clip-recorder")
    let writer: AVAssetWriter
    let input: AVAssetWriterInput
    let width: Int
    let height: Int
    var stream: SCStream?
    var firstPts: CMTime?
    var frames = 0
    /// The last frame written, re-stamped at the stop time by `finish`.
    var lastSample: CMSampleBuffer?
    var stopping = false

    init(out: URL, width: Int, height: Int) throws {
        try? FileManager.default.removeItem(at: out)
        writer = try AVAssetWriter(outputURL: out, fileType: .mov)
        input = AVAssetWriterInput(
            mediaType: .video,
            outputSettings: [
                AVVideoCodecKey: AVVideoCodecType.proRes4444,
                AVVideoWidthKey: width,
                AVVideoHeightKey: height,
            ])
        input.expectsMediaDataInRealTime = true
        guard writer.canAdd(input) else { fail("AVAssetWriter refused a ProRes 4444 input of \(width)x\(height)") }
        writer.add(input)
        self.width = width
        self.height = height
    }

    func stream(_ stream: SCStream, didOutputSampleBuffer sampleBuffer: CMSampleBuffer, of type: SCStreamOutputType) {
        guard type == .screen, !stopping, sampleBuffer.isValid else { return }
        // Only COMPLETE frames carry pixels; idle/blank/suspended ones are bookkeeping.
        guard
            let attachments = CMSampleBufferGetSampleAttachmentsArray(sampleBuffer, createIfNecessary: false)
                as? [[SCStreamFrameInfo: Any]],
            let rawStatus = attachments.first?[.status] as? Int,
            SCFrameStatus(rawValue: rawStatus) == .complete
        else { return }

        let pts = CMSampleBufferGetPresentationTimeStamp(sampleBuffer)
        if firstPts == nil {
            guard writer.startWriting() else {
                fail("AVAssetWriter could not start: \(writer.error?.localizedDescription ?? "unknown")")
            }
            writer.startSession(atSourceTime: pts)
            firstPts = pts
            let age = CMTimeGetSeconds(CMTimeSubtract(hostNow(), pts))
            let epochMs = (Date().timeIntervalSince1970 - age) * 1000
            emit(["event": "recording", "firstFrameEpochMs": epochMs, "width": width, "height": height])
        }
        if input.isReadyForMoreMediaData {
            if input.append(sampleBuffer) {
                frames += 1
                lastSample = sampleBuffer
            }
        }
    }

    func stream(_ stream: SCStream, didStopWithError error: Error) {
        fail("ScreenCaptureKit stopped the stream: \(error.localizedDescription)")
    }

    /// Stops the capture and finalizes the file, then exits. Safe to call twice.
    func finish() {
        queue.async {
            if self.stopping { return }
            self.stopping = true
            let stopAt = hostNow()
            self.stream?.stopCapture { _ in
                self.queue.async {
                    guard let first = self.firstPts else { fail("stopped before the first frame arrived") }
                    // The last frame again, stamped at the stop: `endSession` alone left the
                    // file ending at the last CHANGE (a 2.75 s take probed as 2.46 s, verified
                    // 2026-10-23), which cut a still ending short.
                    if let last = self.lastSample, CMTimeCompare(stopAt, CMSampleBufferGetPresentationTimeStamp(last)) > 0 {
                        var timing = CMSampleTimingInfo(duration: .invalid, presentationTimeStamp: stopAt, decodeTimeStamp: .invalid)
                        var copy: CMSampleBuffer?
                        if CMSampleBufferCreateCopyWithNewTiming(
                            allocator: nil, sampleBuffer: last, sampleTimingEntryCount: 1, sampleTimingArray: &timing,
                            sampleBufferOut: &copy) == noErr,
                            let copy, self.input.isReadyForMoreMediaData
                        {
                            self.input.append(copy)
                        }
                    }
                    self.input.markAsFinished()
                    self.writer.endSession(atSourceTime: stopAt)
                    self.writer.finishWriting {
                        if self.writer.status != .completed {
                            fail("writing the .mov failed: \(self.writer.error?.localizedDescription ?? "unknown")")
                        }
                        emit([
                            "event": "done", "frames": self.frames,
                            "durationS": CMTimeGetSeconds(CMTimeSubtract(stopAt, first)),
                        ])
                        exit(0)
                    }
                }
            }
        }
    }
}

func preflight() async -> Never {
    let access = CGPreflightScreenCaptureAccess()
    var windows = -1
    var message = ""
    do {
        windows = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: true).windows.count
    } catch {
        message = error.localizedDescription
    }
    emit(["event": "preflight", "screenCaptureAccess": access, "shareableWindows": windows, "message": message])
    exit(access && windows >= 0 ? 0 : 2)
}

/// Moves the mouse pointer off the recorded window. The pointer itself is never recorded
/// (`showsCursor = false`), but one resting over the window still HOVERS: buttons light up
/// and tooltips open mid-clip (seen on the first real run, 2026-10-22).
func parkCursor(outside frame: CGRect, displays: [SCDisplay]) {
    guard let display = displays.first(where: { $0.frame.intersects(frame) }) ?? displays.first else { return }
    let screen = display.frame
    let candidates = [
        CGPoint(x: screen.maxX - 2, y: screen.midY),
        CGPoint(x: screen.minX + 2, y: screen.midY),
        CGPoint(x: screen.midX, y: screen.maxY - 2),
    ]
    let spot = candidates.first(where: { !frame.contains($0) }) ?? candidates[0]
    CGWarpMouseCursorPosition(spot)
}

func record(_ options: Options) async {
    guard let windowId = options.windowId, let out = options.out else {
        fail("usage: --window-id <id> [--include-window-id <id>]... --out <file.mov>")
    }
    let content: SCShareableContent
    do {
        content = try await SCShareableContent.excludingDesktopWindows(false, onScreenWindowsOnly: false)
    } catch {
        fail("no shareable content (is Screen Recording granted to this terminal?): \(error.localizedDescription)")
    }
    func window(_ id: CGWindowID) -> SCWindow {
        guard let found = content.windows.first(where: { $0.windowID == id }) else {
            fail("window \(id) isn't shareable; is it on screen and on this Space?")
        }
        return found
    }
    let primary = window(windowId)
    parkCursor(outside: primary.frame, displays: content.displays)

    // The DISPLAY, minus every window but ours, cropped to the primary window's frame.
    // ❗ Not `desktopIndependentWindow` and not `display(including:)`: both make macOS 27
    // swap the captured window's traffic lights for its purple "being shared" pill, which
    // then sits in the corner of every clip. Excluding everything else is display capture
    // to macOS, so the window keeps its real title bar (verified on macOS 27.0.1,
    // 2026-10-23, by recording the same window both ways).
    let wanted = Set([windowId] + options.includeWindowIds)
    for id in options.includeWindowIds { _ = window(id) }
    guard
        let display = content.displays.first(where: { $0.frame.contains(CGPoint(x: primary.frame.midX, y: primary.frame.midY)) })
    else { fail("window \(windowId) isn't on any display") }
    let filter = SCContentFilter(display: display, excludingWindows: content.windows.filter { !wanted.contains($0.windowID) })
    let config = SCStreamConfiguration()
    config.ignoreShadowsDisplay = true
    config.sourceRect = primary.frame.offsetBy(dx: -display.frame.minX, dy: -display.frame.minY)
    let scale = CGFloat(filter.pointPixelScale)
    // Even dimensions: every 4:2:0 encode downstream needs them.
    let width = (Int((primary.frame.width * scale).rounded()) / 2) * 2
    let height = (Int((primary.frame.height * scale).rounded()) / 2) * 2
    config.width = width
    config.height = height
    config.minimumFrameInterval = CMTime(value: 1, timescale: options.fps)
    config.pixelFormat = kCVPixelFormatType_32BGRA
    config.colorSpaceName = CGColorSpace.sRGB
    config.showsCursor = false
    config.capturesAudio = false
    config.queueDepth = 8

    let recorder: Recorder
    do {
        recorder = try Recorder(out: URL(fileURLWithPath: out), width: width, height: height)
    } catch {
        fail("could not open \(out) for writing: \(error.localizedDescription)")
    }
    let stream = SCStream(filter: filter, configuration: config, delegate: recorder)
    recorder.stream = stream
    do {
        try stream.addStreamOutput(recorder, type: .screen, sampleHandlerQueue: recorder.queue)
        try await stream.startCapture()
    } catch {
        fail("could not start the capture: \(error.localizedDescription)")
    }

    // Three ways to say stop, all finalizing the file: a `stop` line, SIGINT, SIGTERM.
    signal(SIGINT, SIG_IGN)
    signal(SIGTERM, SIG_IGN)
    signalSources = [SIGINT, SIGTERM].map { DispatchSource.makeSignalSource(signal: $0, queue: .main) }
    for source in signalSources {
        source.setEventHandler { recorder.finish() }
        source.resume()
    }
    Thread.detachNewThread {
        while let line = readLine() {
            if line.trimmingCharacters(in: .whitespaces) == "stop" { break }
        }
        // A closed stdin means the orchestrator is gone: finalize rather than orphan.
        recorder.finish()
    }
}

/// Held for the process's life: a dropped `DispatchSource` stops delivering.
var signalSources: [DispatchSourceSignal] = []

@main
struct ClipRecorder {
    /// Synchronous on purpose: the work runs in a `Task`, and the main thread parks in
    /// `dispatchMain` serving the queues every callback above lands on. Every exit is an
    /// explicit `exit()` from `finish`, `fail`, or `preflight`.
    static func main() {
        let options = Options.parse(CommandLine.arguments)
        // ❗ Connects to the window server before ScreenCaptureKit needs it. Without an
        // NSApplication, `startCapture` aborts in `CGS_REQUIRE_INIT` (verified on macOS
        // 27.0.1, 2026-10-22); `--preflight` works without it, which is how that hid.
        // `.prohibited`: no Dock icon, never the key app.
        NSApplication.shared.setActivationPolicy(.prohibited)
        Task {
            if options.preflight { await preflight() }
            await record(options)
        }
        dispatchMain()
    }
}
