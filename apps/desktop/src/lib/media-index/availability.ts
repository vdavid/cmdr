import { isE2eRun } from '#lib/app-mode.js'
import { isMacOS } from '#lib/platform/index.js'

/**
 * Whether this run can index images. The engine is Apple's Vision plus CLIP's Core ML
 * towers, so a shipped build has one on macOS only, and elsewhere the backend starts no
 * scheduler (#943). The one exception is the never-shipped Playwright E2E build: off
 * macOS it runs the media index on a fake backend (`cmdr-index`'s `playwright-e2e`
 * feature, `media_index/scheduler/lifecycle.rs::platform_backends`), so the
 * image-indexing specs keep running on Linux. `isE2eRun()` stands in for that build
 * here, and it reads the launch mode, so callers settle `initAppMode()` first.
 */
export function imageIndexingAvailable(): boolean {
  return isMacOS() || isE2eRun()
}
