/**
 * Listing-path friendly-error copy: `reason` + params → user-facing message.
 *
 * The Rust backend classifies a listing/empty-root failure into a typed
 * `ListingErrorReason` plus structured params and ships them over IPC; this
 * factory owns the WORDS. One reason per currently-distinct message, mirroring
 * the old Rust `errno.rs` / `kinds.rs` / `empty_root.rs` arms verbatim (a
 * behavior-preserving move, not a copy redesign).
 *
 * Every interpolated runtime value (path, OS message) is escaped via `esc(...)`
 * before landing in a trusted template; `{system_settings}` and friends expand
 * to the localized macOS pane labels. See `compose.ts`.
 */

import type { FriendlyErrorMessage } from './friendly-error-message'
import { esc, expandSystemStrings } from './compose'

/**
 * Typed listing-error classification from the backend. Variant-carried params
 * keep impossible combinations unrepresentable. Serialized camelCase from Rust
 * (`#[serde(tag = "reason", ...)]`).
 */
export type ListingErrorReason =
  // ── errno: transient ──
  | { reason: 'interrupted' }
  | { reason: 'notEnoughMemory' }
  | { reason: 'resourceBusy'; path: string }
  | { reason: 'temporarilyUnavailable' }
  | { reason: 'networkDown' }
  | { reason: 'networkConnectionDropped' }
  | { reason: 'connectionDropped' }
  | { reason: 'connectionReset' }
  | { reason: 'connectionTimedOutErrno' }
  | { reason: 'hostDown' }
  | { reason: 'staleConnection' }
  | { reason: 'lockUnavailable' }
  | { reason: 'cancelledErrno' }
  // ── errno: needs-action ──
  | { reason: 'notPermitted'; path: string }
  | { reason: 'pathNotFoundErrno'; path: string }
  | { reason: 'noPermissionErrno'; path: string }
  | { reason: 'alreadyExistsErrno'; path: string }
  | { reason: 'crossDeviceOperation' }
  | { reason: 'notAFolder'; path: string }
  | { reason: 'isAFolderErrno'; path: string }
  | { reason: 'diskFullErrno' }
  | { reason: 'readOnlyVolumeErrno' }
  | { reason: 'notSupportedErrno' }
  | { reason: 'networkUnreachable' }
  | { reason: 'connectionRefused' }
  | { reason: 'symlinkLoopErrno'; path: string }
  | { reason: 'nameTooLongErrno' }
  | { reason: 'hostUnreachable' }
  | { reason: 'folderNotEmpty'; path: string }
  | { reason: 'quotaExceeded' }
  | { reason: 'authRequiredEauth' }
  | { reason: 'authRequiredEneedauth' }
  | { reason: 'devicePoweredOff' }
  | { reason: 'attributeNotFound' }
  // ── errno: serious ──
  | { reason: 'diskReadProblem'; path: string }
  | { reason: 'unexpectedSystemResponse' }
  | { reason: 'deviceProblem' }
  | { reason: 'couldntReadUnknown'; path: string }
  // ── typed VolumeError variants (shared "kinds") ──
  | { reason: 'notFound'; path: string }
  | { reason: 'tccRestricted'; path: string }
  | { reason: 'permissionDenied'; path: string }
  | { reason: 'alreadyExists'; path: string }
  | { reason: 'cancelled' }
  | { reason: 'deviceDisconnected'; path: string }
  | { reason: 'readOnly' }
  | { reason: 'storageFull' }
  | { reason: 'connectionTimedOut' }
  | { reason: 'notSupported' }
  | { reason: 'deletePending'; path: string }
  | { reason: 'ioSerious'; path: string; osMessage: string }
  | { reason: 'isADirectory'; path: string }
  // ── empty-root hint ──
  | { reason: 'emptyRootICloud' }

/** Maps a classified listing reason to its user-facing message (verbatim copy). */
export function getListingErrorMessage(r: ListingErrorReason): FriendlyErrorMessage {
  switch (r.reason) {
    // ── errno: transient ──
    case 'interrupted':
      return {
        title: 'Interrupted',
        message:
          'A system operation was interrupted before it could finish. This is almost always a one-off, caused by a signal or background process momentarily getting in the way.',
        suggestion:
          'Navigate here again to retry. This kind of interruption almost never happens twice in a row.',
      }
    case 'notEnoughMemory':
      return {
        title: 'Not enough memory',
        message:
          'The system ran out of available memory (RAM) while reading this folder. This can happen when many apps are open at once, or when a folder contains a very large number of files.',
        suggestion:
          "Here's what to try:\n- Close some apps to free up memory, especially ones using lots of resources (browsers with many tabs, editors, media apps)\n- Check memory usage in **Activity Monitor** (search for it in Spotlight)\n- Navigate here again to retry",
      }
    case 'resourceBusy':
      return {
        title: 'Resource busy',
        message: `Cmdr couldn't access \`${esc(r.path)}\` because another app or process is currently using it exclusively. This is usually temporary.`,
        suggestion:
          'Wait a moment, then navigate here again. If it keeps happening, check which app might be holding the file open (in Terminal, run `lsof +D <folder-path>` to see which processes are using this folder).',
      }
    case 'temporarilyUnavailable':
      return {
        title: 'Temporarily unavailable',
        message:
          'The system is momentarily too busy to handle this request. This is a transient condition that typically clears up on its own within seconds.',
        suggestion:
          'Navigate here again to retry. This usually resolves on its own. If it keeps happening, the system might be under heavy load. Check **Activity Monitor** for apps consuming a lot of resources.',
      }
    case 'networkDown':
      return {
        title: 'Network is down',
        message:
          "Your Mac's network connection is down, so Cmdr can't reach this volume. This could mean Wi-Fi is disconnected, an Ethernet cable is unplugged, or the network interface is disabled.",
        suggestion: expandSystemStrings(
          "Here's what to try:\n- Check Wi-Fi or Ethernet status in **{system_settings} > Network**\n- If you're on Wi-Fi, try turning it off and on again\n- In Terminal, run `ping google.com` to test your connection\n- Navigate here again once you're back online",
        ),
      }
    case 'networkConnectionDropped':
      return {
        title: 'Network connection dropped',
        message:
          'The network connection was unexpectedly reset while Cmdr was reading this folder. This can happen when a router restarts, a VPN reconnects, or the network is temporarily unstable.',
        suggestion:
          "Here's what to try:\n- Check your network connection\n- If you're on a VPN, make sure it's still connected\n- Navigate here again to retry",
      }
    case 'connectionDropped':
      return {
        title: 'Connection dropped',
        message:
          'The connection was dropped by the server or the network before Cmdr could finish reading. This often means the server is overloaded or restarting.',
        suggestion:
          "Here's what to try:\n- Check that the server is running and responsive\n- Check your network connection\n- Navigate here again to retry",
      }
    case 'connectionReset':
      return {
        title: 'Connection reset',
        message:
          'The remote server closed the connection unexpectedly. This can happen when the server restarts, hits a timeout, or runs into an internal problem.',
        suggestion:
          "Here's what to try:\n- Check that the server is running\n- In Terminal, try `ping <hostname>` to test if the server is reachable\n- Navigate here again to retry",
      }
    case 'connectionTimedOutErrno':
      return {
        title: 'Connection timed out',
        message:
          "Cmdr tried to read this folder but the connection didn't respond in time. This usually means the server or device is slow, unreachable, or the network between you and it is congested.",
        suggestion:
          "Here's what to try:\n- Check that the device or server is powered on and reachable\n- Check your Wi-Fi or Ethernet connection\n- In Terminal, try `ping <hostname>` to test connectivity\n- Navigate here again to retry",
      }
    case 'hostDown':
      return {
        title: 'Host is down',
        message:
          "The remote host (the computer or server hosting this volume) isn't responding. It may be powered off, sleeping, or temporarily unreachable.",
        suggestion:
          "Here's what to try:\n- Check that the host is powered on and connected to the network\n- In Terminal, try `ping <hostname>` to test if it's reachable\n- If it's a NAS or server, check its management interface\n- Navigate here again once the host is back",
      }
    case 'staleConnection':
      return {
        title: 'Stale connection',
        message:
          'Cmdr is trying to access this folder using an old reference that the server no longer recognizes. This commonly happens with network drives (NFS, SMB) after the server restarts, the share is remounted, or the connection was interrupted.',
        suggestion:
          "Here's what to try:\n- Navigate away from this folder and come back\n- If this is a network drive, try unmounting and remounting it in Finder\n- Check that the server hosting this folder is running\n- In Terminal, run `mount` to see currently mounted volumes",
      }
    case 'lockUnavailable':
      return {
        title: 'Lock unavailable',
        message:
          'The system ran out of file locks. File locks are how apps coordinate access to shared files (preventing two apps from writing to the same file at once). Running out usually means too many apps are accessing files simultaneously.',
        suggestion:
          "Here's what to try:\n- Close some apps, especially ones that work with many files (editors, IDEs, backup tools)\n- In Terminal, run `lsof | wc -l` to see how many files are open across all apps\n- If the problem keeps happening, you can raise the limit with `ulimit -n 4096` in Terminal\n- Navigate here again to retry",
      }
    case 'cancelledErrno':
      return {
        title: 'Cancelled',
        message: 'The operation was cancelled before it could finish.',
        suggestion: "Navigate here again whenever you're ready to retry.",
      }

    // ── errno: needs-action ──
    case 'notPermitted':
      return {
        title: 'Not permitted',
        message: `macOS blocked Cmdr from accessing \`${esc(r.path)}\`. This usually means the folder is protected by macOS security policies, or Cmdr hasn't been granted the right permissions yet.`,
        suggestion: expandSystemStrings(
          "Here's what to try:\n- Open **{system_settings} > {privacy_and_security} > {files_and_folders}** and grant Cmdr access\n- If this is a system-protected folder (like system directories), you may need to grant Cmdr **{full_disk_access}** under {privacy_and_security}\n- In Terminal, run `ls -la` on this path to check ownership and permissions",
        ),
      }
    case 'pathNotFoundErrno':
      return {
        title: 'Path not found',
        message: `Cmdr couldn't find \`${esc(r.path)}\`. It may have been moved, renamed, or deleted while Cmdr was trying to access it.`,
        suggestion:
          "Here's what to try:\n- Check that the path is spelled correctly\n- If this is on a network drive, make sure it's connected and the share is accessible\n- Navigate to the parent folder and look for the item there\n- In Terminal, run `ls -la` on the parent folder to see what's there",
      }
    case 'noPermissionErrno':
      return {
        title: 'No permission',
        message: `Cmdr doesn't have permission to access \`${esc(r.path)}\`. macOS controls which apps can access which folders, and Cmdr hasn't been granted access to this one yet.`,
        suggestion: expandSystemStrings(
          "Here's what to try:\n- Open **{system_settings} > {privacy_and_security} > {files_and_folders}** and grant Cmdr access\n- Check the folder's permissions in Finder: right-click it, choose Get Info, and look under Sharing & Permissions\n- If this is a shared folder, ask the owner to update permissions\n- In Terminal, run `ls -la` on this path to see the current permissions",
        ),
      }
    case 'alreadyExistsErrno':
      return {
        title: 'Already exists',
        message: `A file or folder already exists at \`${esc(r.path)}\`, so Cmdr can't create a new one there.`,
        suggestion: 'Rename the existing item or choose a different name for the new one.',
      }
    case 'crossDeviceOperation':
      return {
        title: 'Cross-device operation',
        message:
          "Cmdr can't move this item directly because the source and destination are on different volumes (for example, an internal drive and a USB stick). Moving across volumes requires copying the data and then removing the original.",
        suggestion:
          'Copy the item to the destination instead of moving it. Cmdr will handle the copy automatically.',
      }
    case 'notAFolder':
      return {
        title: 'Not a folder',
        message: `Cmdr expected \`${esc(r.path)}\` to be a folder, but it's a file. This can happen if something was recently renamed or replaced.`,
        suggestion: 'Check the path and make sure it points to a folder, not a file.',
      }
    case 'isAFolderErrno':
      return {
        title: 'Is a folder',
        message: `Cmdr expected \`${esc(r.path)}\` to be a file, but it's a folder. This can happen if something was recently renamed or replaced.`,
        suggestion: 'Check the path and make sure it points to a file, not a folder.',
      }
    case 'diskFullErrno':
      return {
        title: 'Disk is full',
        message: "There isn't enough free space on this volume to complete the operation.",
        suggestion: expandSystemStrings(
          "Here's what to try:\n- Free up space by moving or deleting files you no longer need\n- Empty the Trash (right-click the Trash icon in the Dock)\n- In Terminal, run `df -h` to see how much space is left on each volume\n- Check **{system_settings} > General > Storage** for a breakdown of what's using space",
        ),
      }
    case 'readOnlyVolumeErrno':
      return {
        title: 'Read-only volume',
        message:
          "This volume is mounted as read-only, so Cmdr can't make changes to it. This could be because the device has a physical write-protection switch, the disk image was mounted read-only, or the file system doesn't support writing.",
        suggestion:
          "Here's what to try:\n- If the device has a physical write-protection switch (common on SD cards), flip it off\n- If this is a disk image, remount it with write access\n- Otherwise, copy the files to a writable location first",
      }
    case 'notSupportedErrno':
      return {
        title: 'Not supported',
        message:
          "This operation isn't supported on this file system. Different file systems (like FAT32, NTFS, or network shares) support different features, and this one doesn't support what Cmdr is trying to do.",
        suggestion:
          "Try a different approach, or use Finder for this operation. If you're working with an external drive, it might be formatted with a file system that has limitations (for example, FAT32 can't store files larger than 4 GB).",
      }
    case 'networkUnreachable':
      return {
        title: 'Network unreachable',
        message:
          "Cmdr can't reach the network this volume is on. This often means you're not connected to the right network, or a VPN isn't active.",
        suggestion:
          "Here's what to try:\n- Check your Wi-Fi or Ethernet connection\n- Make sure you're on the right network (for example, your office Wi-Fi or VPN)\n- In Terminal, try `ping <hostname>` to test if the server is reachable\n- Navigate here again once you're connected",
      }
    case 'connectionRefused':
      return {
        title: 'Connection refused',
        message:
          "The server actively refused the connection. This usually means the server software (for example, an SMB or NFS service) isn't running, or it's configured to reject connections from this Mac.",
        suggestion:
          "Here's what to try:\n- Check that the server is running and its file sharing service is active\n- Verify the server address and port are correct\n- In Terminal, try `ping <hostname>` to check if the server is reachable at all\n- Navigate here again to retry",
      }
    case 'symlinkLoopErrno':
      return {
        title: 'Symlink loop',
        message: `Cmdr found a circular chain of symbolic links (shortcuts that point to other shortcuts) at \`${esc(r.path)}\`. Following these links leads in a circle, so Cmdr can't reach the actual file or folder.`,
        suggestion:
          "Here's what to try:\n- In Terminal, run `ls -la` on this path to see where the symbolic links point\n- Find and fix the link that creates the loop\n- If you're not sure which link is the problem, follow them one by one with `readlink <path>`",
      }
    case 'nameTooLongErrno':
      return {
        title: 'Name too long',
        message:
          "The file or folder name exceeds the system's limit (255 characters on most Mac volumes). This can also happen when the full path (all folders combined) exceeds the system's maximum path length.",
        suggestion:
          'Rename the item to use a shorter name. If the name looks reasonable, the full path (including all parent folders) might be too long. Try moving it to a shorter path.',
      }
    case 'hostUnreachable':
      return {
        title: 'Host unreachable',
        message:
          "Cmdr can't find a network route to the host this volume is on. This usually means the host is on a different network, behind a firewall, or the routing configuration needs updating.",
        suggestion:
          "Here's what to try:\n- Check that the host is powered on and on the same network\n- If you need a VPN to reach it, make sure the VPN is connected\n- In Terminal, try `ping <hostname>` to test connectivity\n- Navigate here again once the host is reachable",
      }
    case 'folderNotEmpty':
      return {
        title: 'Folder not empty',
        message: `Cmdr can't remove \`${esc(r.path)}\` because it still contains files or subfolders. The system requires a folder to be empty before it can be removed this way.`,
        suggestion: 'Delete the contents of the folder first, then try removing the folder again.',
      }
    case 'quotaExceeded':
      return {
        title: 'Quota exceeded',
        message:
          "You've reached your disk quota (the maximum amount of space allocated to your user account) on this volume. This is common on shared servers and network drives where an administrator sets per-user limits.",
        suggestion:
          "Here's what to try:\n- Free up space by removing files you no longer need on this volume\n- Ask your system administrator to increase your quota\n- In Terminal, run `quota` to see your current usage and limit",
      }
    case 'authRequiredEauth':
      return {
        title: 'Authentication required',
        message:
          "Cmdr couldn't authenticate with this volume. Your saved credentials may have expired, or the server is rejecting the current login.",
        suggestion:
          "Here's what to try:\n- Disconnect and reconnect the volume, and enter your username and password again\n- Check that your password hasn't changed or expired\n- If this is a company server, check with your IT team",
      }
    case 'authRequiredEneedauth':
      return {
        title: 'Authentication required',
        message:
          'This volume requires you to log in, but no credentials have been provided yet.',
        suggestion:
          "Here's what to try:\n- Disconnect and reconnect the volume in Finder\n- Enter your username and password when prompted\n- If you're not sure about the credentials, check with the server's administrator",
      }
    case 'devicePoweredOff':
      return {
        title: 'Device powered off',
        message:
          "The device is powered off or in a deep sleep state, so Cmdr can't communicate with it.",
        suggestion: 'Turn on the device, wait for it to fully start up, then navigate here again.',
      }
    case 'attributeNotFound':
      return {
        title: 'Attribute not found',
        message:
          "Cmdr tried to read a file attribute (extra metadata like tags or permissions) that doesn't exist on this item. This can happen when the file system doesn't support extended attributes, or when the attribute was removed.",
        suggestion:
          "This file system may not support the metadata Cmdr needs. Try the operation on a different volume, or copy the file to your Mac's internal drive first.",
      }

    // ── errno: serious ──
    case 'diskReadProblem':
      return {
        title: 'Disk read problem',
        message: `Cmdr hit a hardware-level read problem at \`${esc(r.path)}\`. This means the disk or device had trouble reading the data, which could be a temporary glitch or a sign of a failing disk.`,
        suggestion:
          "Here's what to try:\n- Check that the disk or device is still properly connected\n- Open **Disk Utility** (search for it in Spotlight) and run **First Aid** on this volume\n- If this keeps happening, back up your data as soon as possible. The disk may be developing bad sectors or starting to wear out.",
      }
    case 'unexpectedSystemResponse':
      return {
        title: 'Unexpected system response',
        message:
          "The system returned an unexpected response for this operation. This can happen when a volume's file system has inconsistencies, or when the volume is in an unusual state.",
        suggestion:
          "Here's what to try:\n- Navigate here again to retry\n- If this keeps happening, open **Disk Utility** (search for it in Spotlight) and run **First Aid** on this volume to check for file system problems",
      }
    case 'deviceProblem':
      return {
        title: 'Device problem',
        message:
          'The device reported a hardware-level problem. This could be a loose connection, a worn-out cable, or an issue with the device itself.',
        suggestion:
          "Here's what to try:\n- Disconnect and reconnect the device\n- Try a different USB port or cable\n- If it's an external drive, try connecting it to a different computer to see if the problem follows the device\n- If this keeps happening, the device may need repair or replacement",
      }
    case 'couldntReadUnknown':
      return {
        title: "Couldn't read this folder",
        message: `Cmdr ran into an unexpected problem reading \`${esc(r.path)}\`. Check the technical details below for the specific system code, which can help with troubleshooting.`,
        suggestion:
          "Here's what to try:\n- Check that the disk or device is still connected\n- Navigate here again to retry\n- If this keeps happening, open **Disk Utility** and run **First Aid** on this volume",
      }

    // ── typed VolumeError variants (shared "kinds") ──
    case 'notFound':
      return {
        title: 'Path not found',
        message: `Cmdr couldn't find \`${esc(r.path)}\`. It may have been moved, renamed, or deleted while Cmdr was trying to access it.`,
        suggestion:
          "Here's what to try:\n- Check that the path is spelled correctly\n- If this is on a network drive, make sure it's connected and the share is accessible\n- Navigate to the parent folder and look for the item there\n- In Terminal, run `ls -la` on the parent folder to see what's there",
      }
    case 'tccRestricted':
      return {
        title: 'This folder is restricted by macOS',
        message: `Cmdr can't read \`${esc(r.path)}\` because macOS hasn't granted access to this folder yet. This is a privacy gate, not a Cmdr bug.`,
        suggestion: expandSystemStrings(
          'Two ways to fix:\n- Grant Cmdr **{full_disk_access}** in **{system_settings} → {privacy_and_security} → {full_disk_access}** to remove all such limits at once.\n- Or grant per-folder access for just this folder in **{system_settings} → {privacy_and_security} → {files_and_folders} → Cmdr**.',
        ),
      }
    case 'permissionDenied':
      return {
        title: 'No permission',
        message: `Cmdr doesn't have permission to access \`${esc(r.path)}\`. macOS controls which apps can access which folders, and Cmdr hasn't been granted access to this one yet.`,
        suggestion: expandSystemStrings(
          "Here's what to try:\n- Open **{system_settings} > {privacy_and_security} > {files_and_folders}** and grant Cmdr access\n- Check the folder's permissions in Finder: right-click the folder, choose Get Info, and look under Sharing & Permissions\n- If this is a shared folder, ask the owner to update permissions\n- In Terminal, run `ls -la` on the path to see the current permissions",
        ),
      }
    case 'alreadyExists':
      return {
        title: 'Already exists',
        message: `A file or folder already exists at \`${esc(r.path)}\`, so Cmdr can't create a new one there.`,
        suggestion: 'Rename the existing item or choose a different name for the new one.',
      }
    case 'cancelled':
      return {
        title: 'Cancelled',
        message: 'The operation was cancelled before it could finish.',
        suggestion: "Navigate here again whenever you're ready to retry.",
      }
    case 'deviceDisconnected':
      return {
        title: 'Device disconnected',
        message: `The device holding \`${esc(r.path)}\` was disconnected during the operation. This can happen if a USB cable comes loose, a phone goes to sleep, or a network drive drops its connection.`,
        suggestion:
          "Here's what to try:\n- Reconnect the device and make sure the cable is secure\n- If it's a phone, unlock it and make sure file transfer mode is active\n- Navigate here again once the device is back",
      }
    case 'readOnly':
      return {
        title: 'Read-only',
        message:
          "This volume is read-only, so Cmdr can't make changes to it. This could be because the device has a physical write-protection switch, the disk image was mounted as read-only, or the file system doesn't support writing.",
        suggestion:
          "Here's what to try:\n- If the device has a physical write-protection switch (common on SD cards), flip it off\n- If this is a disk image, remount it with write access\n- Otherwise, copy the files to a writable location first",
      }
    case 'storageFull':
      return {
        title: 'Disk is full',
        message: "There isn't enough free space on this volume to complete the operation.",
        suggestion:
          "Here's what to try:\n- Free up space by moving or deleting files you no longer need\n- Empty the Trash (right-click the Trash icon in the Dock)\n- In Terminal, run `df -h` to see how much space is left on each volume",
      }
    case 'connectionTimedOut':
      return {
        title: 'Connection timed out',
        message:
          "Cmdr tried to access this resource but the connection didn't respond in time. This usually means the server or device is slow to respond, or the network connection is unstable.",
        suggestion:
          "Here's what to try:\n- Check that the device or server is powered on and reachable\n- Check your Wi-Fi or Ethernet connection\n- In Terminal, try `ping <hostname>` to test if the server is reachable\n- Try again",
      }
    case 'notSupported':
      return {
        title: 'Not supported',
        message:
          "This operation isn't supported on this type of volume. Some volumes (like phone storage or certain network drives) don't support all operations.",
        suggestion: 'Try a different approach, or use Finder for this operation.',
      }
    case 'deletePending':
      return {
        title: 'File is being removed',
        message: `\`${esc(r.path)}\` is on its way out. The server marked it for deletion, but another open handle is keeping it around until that handle closes.`,
        suggestion:
          "Here's what to try:\n- Wait a moment and try again — once the last handle closes, the file disappears\n- Close any other apps that might have this file open\n- If it sticks around, restart Cmdr to drop any handles it might still hold",
      }
    case 'ioSerious':
      return {
        title: "Couldn't read this folder",
        message: `Cmdr ran into a problem with \`${esc(r.path)}\`: ${esc(r.osMessage)}. This could be a temporary glitch or a sign that the disk or device needs attention.`,
        suggestion:
          "Here's what to try:\n- Check that the disk or device is still connected\n- Try again\n- If this keeps happening, try running **Disk Utility > First Aid** on this volume",
      }
    case 'isADirectory':
      return {
        title: 'This is a folder, not a file',
        message: `Cmdr tried to open \`${esc(r.path)}\` as a file, but it's a folder.`,
        suggestion: 'Navigate into the folder instead of opening it as a file.',
      }

    // ── empty-root hint ──
    case 'emptyRootICloud':
      return {
        title: 'iCloud Drive looks empty',
        message: expandSystemStrings(
          'Cmdr opened iCloud Drive but it came back with no files. macOS hides iCloud Drive contents from apps that don\'t have **{full_disk_access}**, so granting Cmdr that permission is the most likely fix.\n\nIf your iCloud Drive really is empty, you can ignore this hint.',
        ),
        suggestion: expandSystemStrings(
          "Here's what to try:\n- Open [**{system_settings} > {privacy_and_security}**](x-apple.systempreferences:com.apple.preference.security?Privacy) and pick **{full_disk_access}**\n- Add Cmdr (use the **+** button) and toggle it on\n- Quit and reopen Cmdr\n- Come back here to retry",
        ),
      }
  }
}
