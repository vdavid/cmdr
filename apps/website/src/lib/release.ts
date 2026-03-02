import latestRelease from '../../public/latest.json'

export const version = latestRelease.version

const base = `https://github.com/vdavid/cmdr/releases/download/v${version}`

export const dmgUrls = {
    aarch64: `${base}/Cmdr_${version}_aarch64.dmg`,
    x86_64: `${base}/Cmdr_${version}_x86_64.dmg`,
    universal: `${base}/Cmdr_${version}_universal.dmg`,
}

/** @deprecated Use dmgUrls instead */
export const dmgUrl = dmgUrls.universal

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
