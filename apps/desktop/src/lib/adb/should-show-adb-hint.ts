/**
 * Whether an MTP pane should offer the fuller way into the same phone.
 *
 * A phone with USB debugging off is a plain MTP row and says nothing about ADB,
 * so nobody finds the feature. One quiet line in the pane header is the whole
 * discoverability story (`DETAILS.md` § "The line offering USB debugging").
 *
 * Pure, because the decision is the part worth pinning down: the pane component
 * reads the settings and the volume list and hands the answers in.
 */

/** The pane and the volume list, as the hint sees them. */
export interface AdbHintInputs {
  /** Whether this pane is showing a device over MTP. */
  isMtpPane: boolean
  /** The pane's device name, which is what an ADB twin would share. */
  deviceName: string
  /** Every device row's name, MTP and ADB alike. */
  deviceNames: readonly string[]
  /** `fileOperations.adbEnabled`: whether Cmdr follows Android devices at all. */
  adbEnabled: boolean
  /** `behavior.adbHintDismissed`: whether this has already been said once. */
  dismissed: boolean
}

/**
 * Whether to show the line.
 *
 * ❗ Silent when the same phone ALREADY has an ADB row: USB debugging is on, and
 * telling someone to turn on what they turned on is what makes a hint a nag.
 * ❗ Silent when ADB support is off, because turning USB debugging on would then
 * produce no row and the advice would lead nowhere.
 */
export function shouldShowAdbHint(inputs: AdbHintInputs): boolean {
  if (!inputs.isMtpPane || inputs.dismissed || !inputs.adbEnabled) return false
  // Two rows under one name are the MTP half and the ADB half of one phone; the
  // switcher's own label rule reads the pair the same way.
  return inputs.deviceNames.filter((name) => name === inputs.deviceName).length < 2
}
