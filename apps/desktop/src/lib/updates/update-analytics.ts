/**
 * What one update check reports to analytics, as pure vocabulary.
 *
 * Without it the update path is invisible: the dashboard can count that an install ASKED (the
 * `update_checks` row the manifest proxy writes) and what version it's running, and nothing in
 * between. So "everyone is current", "everyone has a build staged they never restart into", and
 * "the install fails on every one of them" all look the same from the outside, which is exactly
 * the ambiguity that let two real defects sit unnoticed for weeks.
 *
 * ❌ Every property is categorical: an outcome token, a typed failure kind, and a version number
 * from our own release list. Never a URL, a path, a bundle location, or the text of a failure.
 */

import { trackEvent } from '$lib/tauri-commands'
import type { BundleWriteBlocker } from '$lib/tauri-commands'

/**
 * What became of one check.
 *
 * `already_staged` is the one that earns its place twice over: it separates "this install is
 * current" from "this install downloaded something weeks ago and is still running the old build",
 * and a rising count of it against a flat `staged` is the stuck population, visible directly.
 */
export type UpdateCheckOutcome =
  /** Nothing newer than what's running. */
  | 'up_to_date'
  /** A build was downloaded and synced into the bundle; only a restart is missing. */
  | 'staged'
  /** A build was already staged and the server offered nothing that beats it. */
  | 'already_staged'
  /** An update exists, but this install can't write its own bundle. */
  | 'blocked'
  /** The check, the download, or the install didn't get there. */
  | 'failed'

/** The typed reason behind a non-happy outcome, or `none`. ❌ Never a message. */
export type UpdateCheckFailure =
  | 'none'
  /** Reaching or reading the manifest. */
  | 'check'
  /** Fetching or verifying the tarball. */
  | 'download'
  /** Syncing the tarball into the bundle. */
  | 'install'
  /** macOS App Translocation: the app was opened from where it was downloaded. */
  | 'translocated'
  /** The bundle sits on a read-only volume, in practice a mounted disk image. */
  | 'read_only_volume'

export interface UpdateCheckReport {
  outcome: UpdateCheckOutcome
  failure?: UpdateCheckFailure
  /** The version sitting in the bundle waiting for a restart, if any. */
  stagedVersion?: string | null
}

/**
 * The event's properties. Pure (no IPC, no gating), so the vocabulary is unit-testable without a
 * running app, the same split the backend's event modules use.
 */
export function updateCheckProps(report: UpdateCheckReport): Record<string, string> {
  return {
    outcome: report.outcome,
    failure: report.failure ?? 'none',
    staged_version: report.stagedVersion ?? 'none',
  }
}

/** Reports one finished check. Fire-and-forget, like every frontend event. */
export function reportUpdateCheck(report: UpdateCheckReport): void {
  void trackEvent('update_check', updateCheckProps(report))
}

/** The blocker's analytics token. Separate from the IPC spelling so a rename of one can't
 * silently split one number into two on the dashboard. */
export function blockerFailure(blocker: BundleWriteBlocker): UpdateCheckFailure {
  return blocker === 'translocated' ? 'translocated' : 'read_only_volume'
}
