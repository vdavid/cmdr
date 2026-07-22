/**
 * Fixtures for `crash-report` (`$lib/crash-reporter/CrashReportDialog.svelte`).
 *
 * The dialog's only branching content is the report id line and the expandable
 * JSON, so the two states are "a modern report with a short id" and "an older
 * report without one". The JSON is deliberately long: the details block has to
 * stay readable and scrollable inside a 440px dialog.
 *
 * Raw copy on purpose: this module is dev-only and sits outside the i18n-enforced
 * areas, so fixture strings never reach the message catalog.
 */

import type { CrashReport } from '$lib/tauri-commands'

const BACKTRACE_FRAMES = [
  'core::panicking::panic_fmt::h4f2a1c9e8b3d7a10',
  'core::option::expect_failed::h9c1e0b7d24af3e55',
  'cmdr_lib::file_system::volume::smb::SmbVolume::list_directory::h71b3c9a0d5e2f884',
  'cmdr_lib::file_system::listing::cache::ListingCache::refresh::hc0d81a6f39be47a2',
  'cmdr_lib::commands::file_system::list_directory::{{closure}}::h2d9f4e6a8c1b0357',
  'tokio::runtime::task::harness::Harness<T,S>::poll::h5a8c3f01b9d7e264',
  'tokio::runtime::scheduler::multi_thread::worker::Context::run_task::hd41f8b06c5a29e37',
  'std::sys::pal::unix::thread::Thread::new::thread_start::h8e0b2d75a1c96f43',
]

const ACTIVE_SETTINGS = {
  indexingEnabled: true,
  aiProvider: 'anthropic',
  mcpEnabled: true,
  verboseLogging: false,
}

/**
 * Keyed by the `crash-report` entry's state ids in `gallery-registry.ts`. Values
 * are optional so a lookup by an id that drifted out of the registry is
 * detectable rather than silently typed as present.
 */
export const crashReportFixtures: Record<string, CrashReport | undefined> = {
  panic: {
    version: 1,
    timestamp: '2026-07-21T22:14:07Z',
    signal: null,
    panicMessage:
      'called `Option::unwrap()` on a `None` value: no cached listing for smb://naspolya.local/media/photos/2026',
    backtraceFrames: BACKTRACE_FRAMES,
    threadName: 'tokio-runtime-worker',
    threadCount: 24,
    appVersion: '0.9.4',
    osVersion: 'macOS 26.1 (25B78)',
    arch: 'aarch64',
    uptimeSecs: 4_812,
    activeSettings: ACTIVE_SETTINGS,
    possibleCrashLoop: false,
    buildMode: 'release',
    shortId: 'CRASH-7QK2M',
    diagId: 'diag_5f1c9a2e-8d34-4b17-9c60-2ab7e05d4813',
  },
  // A report written by an older app version: no `shortId`, so the report-id line
  // is absent. Also a signal crash rather than a panic, and flagged as a possible
  // crash loop.
  'signal-no-report-id': {
    version: 1,
    timestamp: '2026-07-22T06:03:41Z',
    signal: 'SIGSEGV',
    panicMessage: null,
    backtraceFrames: BACKTRACE_FRAMES.slice(2),
    threadName: null,
    threadCount: 12,
    appVersion: '0.8.1',
    osVersion: 'macOS 15.6 (24G84)',
    arch: 'x86_64',
    uptimeSecs: 3,
    activeSettings: { ...ACTIVE_SETTINGS, aiProvider: null, mcpEnabled: null },
    possibleCrashLoop: true,
    buildMode: 'release',
    shortId: null,
  },
}
