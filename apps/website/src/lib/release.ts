import latestRelease from '../../public/latest.json'

export const version = latestRelease.version

/** Display version: drops a trailing `.0` patch (`0.16.0` → `0.16`), keeps non-zero patches (`0.16.1`). */
export const versionDisplay = version.endsWith('.0') ? version.slice(0, -2) : version

const downloadBase = import.meta.env.PUBLIC_DOWNLOAD_BASE_URL
const githubBase = `https://github.com/vdavid/cmdr/releases/download/v${version}`

function dmgUrl(arch: string): string {
  if (downloadBase) return `${downloadBase}/download/${version}/${arch}`
  return `${githubBase}/Cmdr_${version}_${arch}.dmg`
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
