/**
 * Provider-overlay friendly-error copy: (provider, category) → suggestion.
 *
 * `provider.rs` detects which cloud/mount provider manages a path (path patterns
 * + `statfs`); that detection stays in Rust and ships a typed `provider` on the
 * listing-error payload. When a provider is present, the FE replaces the base
 * reason's suggestion with the provider-specific one here, reproducing the old
 * Rust `enrich_with_provider` override exactly. Provider display names and app
 * names are words, so they live here too.
 *
 * `category` is the base reason's `ErrorCategory` (`transient` | `needs_action`
 * | `serious`), which the old Rust code keyed on.
 */

/** Serialized camelCase from Rust `Provider`. */
export type Provider =
  | 'dropbox'
  | 'googleDrive'
  | 'oneDrive'
  | 'box'
  | 'pCloud'
  | 'nextcloud'
  | 'synologyDrive'
  | 'tresorit'
  | 'protonDrive'
  | 'sync'
  | 'egnyte'
  | 'macDroid'
  | 'iCloud'
  | 'pCloudFuse'
  | 'macFuse'
  | 'veraCrypt'
  | 'cmVolumes'
  | 'genericCloudStorage'

export type ProviderCategory = 'transient' | 'needs_action' | 'serious'

/** Provider display name (the `**bold**` name shown in suggestions). */
function displayName(p: Provider): string {
  switch (p) {
    case 'dropbox':
      return 'Dropbox'
    case 'googleDrive':
      return 'Google Drive'
    case 'oneDrive':
      return 'OneDrive'
    case 'box':
      return 'Box'
    case 'pCloud':
      return 'pCloud'
    case 'nextcloud':
      return 'Nextcloud'
    case 'synologyDrive':
      return 'Synology Drive'
    case 'tresorit':
      return 'Tresorit'
    case 'protonDrive':
      return 'Proton Drive'
    case 'sync':
      return 'Sync.com'
    case 'egnyte':
      return 'Egnyte'
    case 'macDroid':
      return 'MacDroid'
    case 'iCloud':
      return 'iCloud Drive'
    case 'pCloudFuse':
      return 'pCloud'
    case 'macFuse':
      return 'macFUSE'
    case 'veraCrypt':
      return 'VeraCrypt'
    case 'cmVolumes':
      return 'Cloud mount'
    case 'genericCloudStorage':
      return 'your cloud provider'
  }
}

/** App name, or null for providers without a single distinct app. */
function appName(p: Provider): string | null {
  switch (p) {
    case 'dropbox':
      return 'Dropbox'
    case 'googleDrive':
      return 'Google Drive'
    case 'oneDrive':
      return 'OneDrive'
    case 'box':
      return 'Box Drive'
    case 'pCloud':
    case 'pCloudFuse':
      return 'pCloud Drive'
    case 'macFuse':
      return null
    case 'nextcloud':
      return 'Nextcloud'
    case 'synologyDrive':
      return 'Synology Drive'
    case 'tresorit':
      return 'Tresorit'
    case 'protonDrive':
      return 'Proton Drive'
    case 'sync':
      return 'Sync.com'
    case 'egnyte':
      return 'Egnyte Connect'
    case 'macDroid':
      return 'MacDroid'
    case 'iCloud':
      return null
    case 'veraCrypt':
      return 'VeraCrypt'
    case 'cmVolumes':
      return null
    case 'genericCloudStorage':
      return null
  }
}

/**
 * Builds the provider-specific suggestion (verbatim from Rust `provider_suggestion`).
 * Provider names are static (trusted), so no escaping is needed.
 */
export function getProviderSuggestion(provider: Provider, category: ProviderCategory): string {
  const name = displayName(provider)

  switch (provider) {
    case 'macDroid':
      switch (category) {
        case 'transient':
          return 'This folder is managed by **MacDroid**. Here\'s what to try:\n- Open MacDroid and check that your phone is connected\n- Make sure your phone is unlocked and set to file transfer mode\n- Unplug and replug the USB cable, then navigate here again'
        case 'needs_action':
          return 'This folder is managed by **MacDroid**. Here\'s what to try:\n- Open MacDroid and check that your phone is connected\n- Make sure your phone is unlocked with the screen on\n- Check that USB file transfer mode is enabled on your phone'
        case 'serious':
          return 'This folder is managed by **MacDroid**. Here\'s what to try:\n- Unplug and replug the USB cable\n- Restart MacDroid\n- Try a different USB port or cable'
      }
      break

    case 'iCloud':
      switch (category) {
        case 'transient':
          return `This folder is managed by **${name}**. Here's what to try:\n- Check your internet connection\n- Make sure you're signed in to iCloud in System Settings\n- Navigate here again to retry`
        case 'needs_action':
          return `This folder is managed by **${name}**. Here's what to try:\n- Check that iCloud Drive is enabled in **System Settings > Apple Account > iCloud**\n- Make sure you're signed in to the right Apple account\n- Check your iCloud storage isn't full`
        case 'serious':
          return `This folder is managed by **${name}**. Here's what to try:\n- Sign out and back in to iCloud in System Settings\n- Check Apple's [system status page](https://www.apple.com/support/systemstatus/)`
      }
      break

    case 'macFuse':
      switch (category) {
        case 'transient':
          return 'This is a **macFUSE** mount. The remote server may be slow or unreachable. Here\'s what to try:\n- Check your network connection\n- Check that the remote server is running\n- Navigate here again to retry'
        case 'serious':
          return 'This is a **macFUSE** mount. The FUSE process backing it has likely crashed or disconnected. Here\'s what to try:\n- Force-unmount the volume: run `umount -f /Volumes/<name>` in Terminal\n- Remount using the original mount command\n- If this keeps happening, check that macFUSE is up to date'
        case 'needs_action':
          return 'This is a **macFUSE** mount. Here\'s what to try:\n- Check that the FUSE process backing this mount is still running\n- Force-unmount and remount the volume if needed\n- Make sure macFUSE is up to date in **System Settings > General > Login Items & Extensions**'
      }
      break

    case 'pCloudFuse':
      switch (category) {
        case 'transient':
          return 'This folder is on **pCloud**\'s virtual drive. Here\'s what to try:\n- Check your internet connection\n- Make sure the pCloud app is running\n- Navigate here again to retry'
        case 'serious':
          return "This folder is on **pCloud**'s virtual drive. The pCloud FUSE process may have crashed. Here's what to try:\n- Quit and reopen the pCloud app\n- If the drive doesn't reappear, force-unmount it: run `umount -f /Volumes/pCloudDrive` in Terminal\n- After a macOS update, re-approve pCloud's system extension in **System Settings > General > Login Items & Extensions**"
        case 'needs_action':
          return "This folder is on **pCloud**'s virtual drive. Here's what to try:\n- Make sure the pCloud app is running and you're signed in\n- Check your internet connection\n- After a macOS update, re-approve pCloud's system extension in **System Settings > General > Login Items & Extensions**"
      }
      break

    case 'veraCrypt':
      switch (category) {
        case 'transient':
          return `This is a **${name}** encrypted volume. Here's what to try:\n- Check that the VeraCrypt volume is still mounted\n- Navigate here again to retry`
        case 'needs_action':
          return `This is a **${name}** encrypted volume. Here's what to try:\n- Open VeraCrypt and check that this volume is mounted\n- Dismount and remount the volume if needed`
        case 'serious':
          return `This is a **${name}** encrypted volume. Here's what to try:\n- Dismount and remount the volume in VeraCrypt\n- If the volume keeps having issues, check it with VeraCrypt's repair tools`
      }
      break

    case 'cmVolumes':
      if (category === 'transient') {
        return 'This is a cloud mount. Here\'s what to try:\n- Check your internet connection\n- Check that the mount software (CloudMounter, Mountain Duck, etc.) is running\n- Navigate here again to retry'
      }
      return 'This is a cloud mount. Here\'s what to try:\n- Check that the mount software (CloudMounter, Mountain Duck, etc.) is running\n- Disconnect and reconnect the mount\n- Check your credentials haven\'t expired'

    case 'genericCloudStorage':
      if (category === 'transient') {
        return 'This folder is managed by a cloud provider. Here\'s what to try:\n- Check your internet connection\n- Check that the sync app is running\n- Navigate here again to retry'
      }
      return 'This folder is managed by a cloud provider. Here\'s what to try:\n- Check that the sync app is running\n- Sign out and back in to the cloud app\n- Check your internet connection'

    // Cloud providers with an app name: Dropbox, Google Drive, OneDrive, Box,
    // pCloud, Nextcloud, SynologyDrive, Tresorit, ProtonDrive, Sync, Egnyte.
    default: {
      const app = appName(provider) ?? name
      switch (category) {
        case 'transient':
          return `This folder is managed by **${name}**. Here's what to try:\n- Check your internet connection\n- Open ${app} and make sure it's running and synced\n- Navigate here again to retry`
        case 'needs_action':
          return `This folder is managed by **${name}**. Here's what to try:\n- Open ${app} and check your sync status\n- Make sure you're signed in to ${app}\n- Check that you have access to this folder in ${name}`
        case 'serious':
          return `This folder is managed by **${name}**. Here's what to try:\n- Quit and reopen ${app}\n- Sign out and back in to ${app}\n- Check ${name}'s status page for outages`
      }
    }
  }
  // Unreachable; every (provider, category) is handled above.
  return ''
}
