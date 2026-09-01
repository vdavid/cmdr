// Android over ADB: path utilities, connect-refusal wording, and the switcher label.
// Pure modules import `./adb-path-utils` directly to stay free of the intl runtime.

export * from './adb-path-utils'
export { adbConnectErrorMessage } from './adb-connect-errors'
export { deviceVolumeLabel } from './adb-volume-label'
