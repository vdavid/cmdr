import latestRelease from '../../public/latest.json'

export const version = latestRelease.version
export const dmgUrl = `https://github.com/vdavid/cmdr/releases/download/v${version}/Cmdr_${version}_universal.dmg`
