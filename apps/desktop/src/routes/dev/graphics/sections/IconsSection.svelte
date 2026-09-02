<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { ICON_COMPONENTS, type IconName } from '$lib/ui/icons/icon-map'
    import GlyphGrid from '../GlyphGrid.svelte'

    /** Where each glyph shows up in the app. Derived by grepping `name="<icon>"` call sites. */
    const USAGE: Record<IconName, string> = {
        'app-window': 'Show button on a running operation row in the operation queue, which puts that operation back in the main window\'s progress dialog.',
        archive: 'Archive action on an Ask Cmdr chat row in the sessions panel, and the archived-view toggle.',
        'archive-restore': 'Unarchive action on an archived Ask Cmdr chat row in the sessions panel.',
        'arrow-left': 'Back button in the onboarding wizard.',
        'arrow-right': 'Rename metaphor between the current and new name in the Ask Cmdr bulk-rename review dialog.',
        bot: 'The status-corner wake indicator while Ask Cmdr is running a conversation it started on its own.',
        'brain-circuit': 'The status-corner wake indicator when Ask Cmdr is allowed to watch but a gate (Full Disk Access, or an API key) still stands in the way.',
        check: 'Selected-state check in onboarding cloud-provider rows, the settings checkbox, the select and combobox dropdowns, the breadcrumb volume menu, and the transfer scan-complete marker.',
        'chevron-down': 'Closed-state indicator on the combobox trigger, and the expand toggle in the download toast.',
        'chevron-right': 'Expand toggle for the per-file error list in the transfer error dialog.',
        'chevron-up': 'Collapse toggle in the expanded download toast.',
        'chevrons-up-down': 'macOS pop-up-button chevron stepper on the select trigger.',
        circle: 'Waiting (not-started) step marker in the drive-indexing checklist.',
        'circle-alert': 'Serious-error marker in the error pane, the transfer error dialog, inline error rows, and the SMB share-list error state.',
        'circle-check': 'Done step marker in the drive-indexing checklist, and the "indexed" file-icon image-index overlay.',
        'circle-dashed': 'The "waiting to be indexed" (pending) file-icon image-index overlay.',
        'circle-dot': 'The "some images still pending" folder-icon image-index coverage overlay.',
        'circle-slash': 'The "not included in image search" (excluded) file-icon image-index overlay.',
        'circle-x': 'Unreachable-host status marker in the network (SMB) browser, and the "could not be indexed" (failed) file-icon image-index overlay.',
        clock: 'Timed-out-host status marker in the network (SMB) browser.',
        copy: 'Type marker for a copy operation row in the operation queue window.',
        'corner-down-left': 'Back-to-presets button in the settings custom-value input (returns to the preset dropdown).',
        download: 'Download button for the on-device CLIP semantic-search model in the Image search settings card.',
        eject: 'Eject control for removable and network volumes in the volume breadcrumb.',
        eye: 'Reveal-password toggle in settings password fields.',
        'eye-off': 'Hide-password toggle in settings password fields.',
        file: 'Fallback file glyph in the search and selection results, and the drag overlay, when no OS icon is available.',
        'file-archive': 'Type marker for a zip-edit (archive_edit) operation row in the operation queue window.',
        'file-plus': 'Type marker for a new-file (create) operation row in the operation queue window.',
        folder:
            'Fallback folder glyph in the search and selection results, the SMB share list, the drag overlay, and the volume-breadcrumb placeholder.',
        'folder-input': 'Type marker for a move operation row in the operation queue window.',
        'folder-plus': 'Type marker for a new-folder (create) operation row in the operation queue window.',
        'git-branch': 'Git repo branch chip, and the branch portal entry inside a `.git` directory.',
        'git-commit-horizontal': 'Commit portal entry inside a virtual `.git` directory.',
        'git-fork': 'Fallback git portal entry inside a virtual `.git` directory.',
        globe: 'Network volume marker in the volume breadcrumb (volume row and placeholder).',
        hourglass:
            'Pending recursive-size indicator in the file lists and selection info, and the drive-indexing status indicator.',
        info: 'Restricted-path marker in the volume breadcrumb and file lists, and the info banner in the transfer error dialog.',
        key: 'Forget-saved-password button in the SMB share-list header.',
        link: 'Symlink badge overlaid on a file or folder icon when the entry is a symlink.',
        list: 'Queue button on the transfer progress dialog (send the transfer to the background, managed in the queue window).',
        lock: 'Pinned-tab marker in the tab bar, the read-only-volume indicator in the breadcrumb, and the SMB login-form lock.',
        'messages-square': 'Opens the Ask Cmdr sessions panel (past chats) from the rail header.',
        monitor: 'System theme-mode toggle option, and the host marker in the network browser.',
        moon: 'Dark theme-mode toggle option.',
        'more-horizontal': 'Row actions menu trigger in the search and selection results.',
        paperclip: 'The "ask about selection" attach button in the Ask Cmdr composer, and its drag-to-attach drop hint.',
        pause: 'Pause control on an operation queue row and the progress dialog (pauses that transfer in place).',
        pencil: 'Type marker for a rename operation row in the operation queue window.',
        play: 'Resume control on a paused operation queue row and the progress dialog (resumes the operation).',
        plus: 'New-tab button at the right end of each pane\'s tab bar.',
        'rotate-ccw': 'Reset-to-default button on a settings row.',
        'rotate-cw': 'Retry control for a timed-out volume refresh in the volume breadcrumb, and the "changed since indexing" (stale) file-icon image-index overlay.',
        search: 'Search-field leading icon in the settings sidebar and the shared query bar.',
        'shield-check': 'Privacy-reassurance banner in the onboarding AI step.',
        'shield-off': 'Privacy-warning banner in the onboarding AI step.',
        sparkles: 'AI-suggestion marker in the onboarding AI step, and the Ask Cmdr rail header and empty state.',
        square: 'Stop button in the Ask Cmdr composer while the assistant is answering.',
        sun: 'Light theme-mode toggle option.',
        tag: 'Tag portal entry inside a virtual `.git` directory.',
        'trash-2': 'Type marker for a delete or trash operation row in the operation queue window.',
        'triangle-alert':
            'Warning marker in the delete and transfer dialogs, the onboarding AI step, unreachable tabs, the Advanced and Keyboard-shortcuts settings banners, the MTP connection error, and the SMB login form.',
        x: 'Clear-field button in the go-to-path dialog, and the dismiss button on toasts.',
    }

    interface IconEntry {
        id: IconName
        caption: string
        usage: string
    }

    const items: IconEntry[] = (Object.keys(ICON_COMPONENTS) as IconName[]).map((name) => ({
        id: name,
        caption: name,
        usage: USAGE[name],
    }))
</script>

<SectionCard id="graphics-icons" label="Icons">
    <GlyphGrid {items}>
        {#snippet intro()}
            Inline glyphs rendered through <code>Icon</code>, from the shared registry in
            <code>lib/ui/icons/icon-map.ts</code>. They inherit <code>currentColor</code>, so each one tints to its
            surrounding text. Shown at a uniform 24px review size.
        {/snippet}
        {#snippet glyph(item: IconEntry)}
            <Icon name={item.id} size={24} aria-hidden="true" />
        {/snippet}
    </GlyphGrid>
</SectionCard>

<style>
    code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }
</style>
