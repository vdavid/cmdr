/**
 * Version comparison for the two surfaces that reason about releases: the What's New popup
 * (is this launch an upgrade over the version we last showed?) and the updater (is the build
 * the manifest offers newer than the one already staged in the bundle?).
 *
 * Shared rather than duplicated because a comparator that disagrees with itself is how one
 * surface calls a release an upgrade while the other calls it a downgrade.
 */

/**
 * Compares two semver strings by their numeric `major.minor.patch` core. Returns a
 * negative number if `a < b`, positive if `a > b`, and 0 if equal. A leading `v` and
 * any pre-release / build suffix are ignored (we only ever compare released versions).
 *
 * Numeric per-component comparison is load-bearing: a string compare would order
 * `0.10.0` before `0.9.0` and misread an upgrade as a downgrade.
 */
export function compareVersions(a: string, b: string): number {
  const coreA = parseVersionCore(a)
  const coreB = parseVersionCore(b)
  for (let i = 0; i < 3; i++) {
    if (coreA[i] !== coreB[i]) return coreA[i] - coreB[i]
  }
  return 0
}

function parseVersionCore(version: string): [number, number, number] {
  const core = version.replace(/^v/, '').split(/[-+]/, 1)[0]
  const parts = core.split('.')
  const major = Number.parseInt(parts[0] ?? '0', 10) || 0
  const minor = Number.parseInt(parts[1] ?? '0', 10) || 0
  const patch = Number.parseInt(parts[2] ?? '0', 10) || 0
  return [major, minor, patch]
}
