# Disk access and permissions

How Cmdr handles macOS file system permissions.

## Full disk access

macOS requires apps to request permission before accessing protected folders like Downloads, Documents, and Desktop.
Cmdr can request **full disk access** (FDA) at first launch, which grants access to all folders at once.

### First launch behavior

On first launch, the app checks if it has FDA. If not, it shows an onboarding screen with two options:

| User action              | What happens                                                                 |
| ------------------------ | ---------------------------------------------------------------------------- |
| **Open System Settings** | Setting saved as `allow`. App stays on the prompt with restart instructions. |
| **Deny**                 | Setting saved as `deny`. App proceeds without FDA.                           |

### Subsequent launches

On every launch, the app checks FDA status and the saved setting:

| FDA granted? | Setting       | Result                                                 |
| ------------ | ------------- | ------------------------------------------------------ |
| Yes          | Any           | Proceed to app. Setting updated to `allow`.            |
| No           | `notAskedYet` | Show onboarding prompt (first launch).                 |
| No           | `allow`       | Show onboarding prompt with "revoked" messaging.       |
| No           | `deny`        | Proceed to app. (User explicitly declined, don't ask.) |

### Settings

The setting is stored in `settings.json`:

```json
{
    "fullDiskAccessChoice": "notAskedYet"
}
```

Possible values:

- `notAskedYet` — Default. First launch, prompt not yet shown.
- `allow` — User clicked "Open System Settings" (presumably granted FDA).
- `deny` — User clicked "Deny". Don't show prompt again.

**Location**: `~/Library/Application Support/com.veszelovszki.cmdr/settings.json`

To reset and show the prompt again:

```bash
rm ~/Library/Application\ Support/com.veszelovszki.cmdr/settings.json
```

---

## Permission denied errors

When the app tries to access a folder it doesn't have permission for (error code 13), it shows a "No permission" pane
with instructions on how to grant access in System Settings.

This happens when:

- User navigates to a protected folder (Downloads, Documents, Desktop)
- FDA wasn't granted, and the folder-specific permission wasn't given either

### Background vs. explicit access

| Context                      | Behavior                                       |
| ---------------------------- | ---------------------------------------------- |
| User navigates to folder     | Show permission denied pane with instructions. |
| Background process (watcher) | Fail silently. No UI, no error log.            |

---

## How FDA check works

The app checks FDA by attempting to read `~/Library/Mail`. This folder is always protected by macOS, so:

- If readable → FDA is granted
- If not readable → FDA is not granted

This check takes under 5 ms and happens at every launch.

## Manual testing

To test FDA manually:

1. Disable FDA in System Settings for the app you run Cmdr with, like Warp Terminal. This _should_ do the trick but it
   seemlingly doesn't. So the next steps are needed.
2. Run `osascript -e 'id of app "Warp"'` (replace `Warp` with the name of the app you run Cmdr with)
3. Then `tccutil reset SystemPolicyDownloadsFolder dev.warp.Warp-Terminal` - Replace `dev.warp.Warp-Terminal` with
   whatever the previous call returned.
    - Or even `tccutil reset All dev.warp.Warp-Terminal`

And to reset the allow/deny setting: `rm ~/Library/Application\ Support/com.veszelovszki.cmdr/settings.json`
