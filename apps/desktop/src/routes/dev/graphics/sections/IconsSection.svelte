<script lang="ts">
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import Icon from '$lib/ui/Icon.svelte'
    import { ICON_COMPONENTS, type IconName } from '$lib/ui/icons/icon-map'
    import { tooltip } from '$lib/tooltip/tooltip'

    /** Where each glyph shows up in the app. Derived by grepping `name="<icon>"` call sites. */
    const USAGE: Record<IconName, string> = {
        'arrow-left': 'Back button in the onboarding wizard.',
        check: 'Selected-state check in onboarding cloud-provider rows and the settings checkbox.',
        'chevron-down':
            'Closed-state indicator on the select and combobox triggers, and the expand toggle in the download toast.',
        'chevron-right': 'Expand toggle for the per-file error list in the transfer error dialog.',
        'chevron-up': 'Collapse toggle in the expanded download toast.',
        circle: 'Waiting (not-started) step marker in the drive-indexing checklist.',
        'circle-alert': 'Serious-error marker in the error pane, the transfer error dialog, and inline error rows.',
        'circle-check': 'Done step marker in the drive-indexing checklist.',
        copy: 'Type marker for a copy operation row in the transfer queue window.',
        eject: 'Eject control for removable and network volumes in the volume breadcrumb.',
        eye: 'Reveal-password toggle in settings password fields.',
        'eye-off': 'Hide-password toggle in settings password fields.',
        'folder-input': 'Type marker for a move operation row in the transfer queue window.',
        'git-branch': 'Git repo branch chip, and the branch portal entry inside a `.git` directory.',
        'git-commit-horizontal': 'Commit portal entry inside a virtual `.git` directory.',
        'git-fork': 'Fallback git portal entry inside a virtual `.git` directory.',
        hourglass:
            'Pending recursive-size indicator in the file lists and selection info, and the drive-indexing status indicator.',
        info: 'Restricted-path marker in the volume breadcrumb and file lists, and the info banner in the transfer error dialog.',
        link: 'Symlink badge overlaid on a file or folder icon when the entry is a symlink.',
        list: 'Queue button on the transfer progress dialog (send the transfer to the background, managed in the queue window).',
        lock: 'Pinned-tab marker in the tab bar.',
        'more-horizontal': 'Row actions menu trigger in the search and selection results.',
        pause: 'Pause control on a transfer queue row and the progress dialog (pauses that transfer in place).',
        play: 'Resume control on a paused transfer queue row and the progress dialog (resumes the transfer).',
        'rotate-ccw': 'Reset-to-default button on a settings row.',
        search: 'Search-field leading icon in the settings sidebar and the shared query bar.',
        'shield-check': 'Privacy-reassurance banner in the onboarding AI step.',
        'shield-off': 'Privacy-warning banner in the onboarding AI step.',
        sparkles: 'AI-suggestion marker in the onboarding AI step.',
        tag: 'Tag portal entry inside a virtual `.git` directory.',
        'trash-2': 'Type marker for a delete or trash operation row in the transfer queue window.',
        'triangle-alert':
            'Warning marker in the delete and transfer dialogs, the onboarding AI step, and unreachable tabs.',
        x: 'Clear-field button in the go-to-path dialog, and the dismiss button on toasts.',
    }

    const names = Object.keys(ICON_COMPONENTS) as IconName[]
</script>

<SectionCard id="graphics-icons" label="Icons">
    <p class="intro">
        Inline glyphs rendered through <code>Icon</code>, from the shared registry in
        <code>lib/ui/icons/icon-map.ts</code>. They inherit <code>currentColor</code>, so each one tints to its
        surrounding text. Shown at a uniform 24px review size.
    </p>
    <div class="grid">
        {#each names as name (name)}
            <div class="cell" use:tooltip={USAGE[name]}>
                <div class="icon-host">
                    <Icon {name} size={24} aria-hidden="true" />
                </div>
                <p class="caption">{name}</p>
            </div>
        {/each}
    </div>
</SectionCard>

<style>
    .intro {
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .intro code {
        font-family: var(--font-mono);
        font-size: var(--font-size-xs);
    }

    .grid {
        display: grid;
        grid-template-columns: repeat(auto-fill, minmax(120px, 1fr));
        gap: var(--spacing-lg);
    }

    .cell {
        display: flex;
        flex-direction: column;
        align-items: center;
    }

    .icon-host {
        height: 48px;
        display: flex;
        align-items: center;
        color: var(--color-text-primary);
    }

    .caption {
        margin: var(--spacing-sm) 0 0;
        font-size: var(--font-size-xs);
        font-family: var(--font-mono);
        color: var(--color-text-tertiary);
        text-align: center;
    }
</style>
