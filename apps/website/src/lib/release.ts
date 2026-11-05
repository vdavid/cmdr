import latestRelease from '../../public/latest.json'

export const version = latestRelease.version

/** Display version: drops a trailing `.0` patch (`0.16.0` → `0.16`), keeps non-zero patches (`0.16.1`). */
export const versionDisplay = version.endsWith('.0') ? version.slice(0, -2) : version

const downloadBase = import.meta.env.PUBLIC_DOWNLOAD_BASE_URL
// The EU mirror, which holds every release the live manifest can name (`docs/guides/releasing.md`
// § Where downloads are served from). The frozen dev copy of `latest.json` names an older version
// it doesn't hold, so these links 404 in dev only.
const mirrorBase = `https://download.getcmdr.com/v${version}`

function dmgUrl(arch: string): string {
  // `?src=website` lets the download endpoint tag these as website-button downloads, distinct from
  // Homebrew (detected by User-Agent) and direct links shared elsewhere. The direct mirror link (used
  // when no download base is configured) carries no telemetry, so the param is irrelevant there.
  if (downloadBase) return `${downloadBase}/download/${version}/${arch}?src=website`
  // Release files spell the Intel arch `x64`, never `x86_64`.
  return `${mirrorBase}/Cmdr_${version}_${arch === 'x86_64' ? 'x64' : arch}.dmg`
}

export const dmgUrls = {
  aarch64: dmgUrl('aarch64'),
  x86_64: dmgUrl('x86_64'),
  universal: dmgUrl('universal'),
}

function formatBytes(bytes: number): string {
  return `${Math.round(bytes / (1024 * 1024))} MB`
}

const rawSizes = latestRelease.dmgSizes

/** Formatted download sizes (for example, "15 MB"), null if not yet populated by CI */
export const dmgSizes =
  rawSizes.universal > 0
    ? {
        aarch64: formatBytes(rawSizes.aarch64),
        x86_64: formatBytes(rawSizes.x86_64),
        universal: formatBytes(rawSizes.universal),
      }
    : null
