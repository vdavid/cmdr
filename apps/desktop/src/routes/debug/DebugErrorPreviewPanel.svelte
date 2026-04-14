<script lang="ts">
    import { tooltip } from '$lib/tooltip/tooltip'
    import type { FriendlyError } from '$lib/file-explorer/types'

    type ErrorCategory = 'transient' | 'needs_action' | 'serious'

    interface ErrorState {
        code?: number
        name: string
        category: ErrorCategory
        title: string
    }

    const providerNames = [
        'None',
        'Dropbox',
        'Google Drive',
        'OneDrive',
        'Box',
        'pCloud',
        'Nextcloud',
        'Synology Drive',
        'Tresorit',
        'Proton Drive',
        'Sync.com',
        'Egnyte',
        'MacDroid',
        'iCloud Drive',
        'macFUSE',
        'VeraCrypt',
        'Cloud mount',
        'your cloud provider',
    ] as const

    /** Per-row selected provider index, keyed by error name. */
    const providerSelections = $state<Record<string, number>>({})

    const errnoErrors: ErrorState[] = [
        // Transient
        { code: 4, name: 'EINTR', category: 'transient', title: 'Interrupted' },
        { code: 12, name: 'ENOMEM', category: 'transient', title: 'Not enough memory' },
        { code: 16, name: 'EBUSY', category: 'transient', title: 'Resource busy' },
        { code: 35, name: 'EAGAIN', category: 'transient', title: 'Temporarily unavailable' },
        { code: 50, name: 'ENETDOWN', category: 'transient', title: 'Network is down' },
        { code: 52, name: 'ENETRESET', category: 'transient', title: 'Network connection dropped' },
        { code: 53, name: 'ECONNABORTED', category: 'transient', title: 'Connection dropped' },
        { code: 54, name: 'ECONNRESET', category: 'transient', title: 'Connection reset' },
        { code: 60, name: 'ETIMEDOUT', category: 'transient', title: 'Connection timed out' },
        { code: 64, name: 'EHOSTDOWN', category: 'transient', title: 'Host is down' },
        { code: 70, name: 'ESTALE', category: 'transient', title: 'Stale connection' },
        { code: 77, name: 'ENOLCK', category: 'transient', title: 'Lock unavailable' },
        { code: 89, name: 'ECANCELED', category: 'transient', title: 'Cancelled' },
        // NeedsAction
        { code: 1, name: 'EPERM', category: 'needs_action', title: 'Not permitted' },
        { code: 2, name: 'ENOENT', category: 'needs_action', title: 'Path not found' },
        { code: 13, name: 'EACCES', category: 'needs_action', title: 'No permission' },
        { code: 17, name: 'EEXIST', category: 'needs_action', title: 'Already exists' },
        { code: 18, name: 'EXDEV', category: 'needs_action', title: 'Cross-device operation' },
        { code: 20, name: 'ENOTDIR', category: 'needs_action', title: 'Not a folder' },
        { code: 21, name: 'EISDIR', category: 'needs_action', title: 'Is a folder' },
        { code: 28, name: 'ENOSPC', category: 'needs_action', title: 'Disk is full' },
        { code: 30, name: 'EROFS', category: 'needs_action', title: 'Read-only volume' },
        { code: 45, name: 'ENOTSUP', category: 'needs_action', title: 'Not supported' },
        { code: 51, name: 'ENETUNREACH', category: 'needs_action', title: 'Network unreachable' },
        { code: 61, name: 'ECONNREFUSED', category: 'needs_action', title: 'Connection refused' },
        { code: 62, name: 'ELOOP', category: 'needs_action', title: 'Symlink loop' },
        { code: 63, name: 'ENAMETOOLONG', category: 'needs_action', title: 'Name too long' },
        { code: 65, name: 'EHOSTUNREACH', category: 'needs_action', title: 'Host unreachable' },
        { code: 66, name: 'ENOTEMPTY', category: 'needs_action', title: 'Folder not empty' },
        { code: 69, name: 'EDQUOT', category: 'needs_action', title: 'Quota exceeded' },
        { code: 80, name: 'EAUTH', category: 'needs_action', title: 'Authentication required' },
        { code: 81, name: 'ENEEDAUTH', category: 'needs_action', title: 'Authentication required' },
        { code: 82, name: 'EPWROFF', category: 'needs_action', title: 'Device powered off' },
        { code: 93, name: 'ENOATTR', category: 'needs_action', title: 'Attribute not found' },
        // Serious
        { code: 5, name: 'EIO', category: 'serious', title: 'Disk read problem' },
        { code: 22, name: 'EINVAL', category: 'serious', title: 'Unexpected system response' },
        { code: 83, name: 'EDEVERR', category: 'serious', title: 'Device problem' },
    ]

    const volumeErrors: ErrorState[] = [
        { name: 'NotFound', category: 'needs_action', title: 'Path not found' },
        { name: 'PermissionDenied', category: 'needs_action', title: 'No permission' },
        { name: 'AlreadyExists', category: 'needs_action', title: 'Already exists' },
        { name: 'NotSupported', category: 'needs_action', title: 'Not supported' },
        { name: 'DeviceDisconnected', category: 'needs_action', title: 'Device disconnected' },
        { name: 'ReadOnly', category: 'needs_action', title: 'Read-only' },
        { name: 'StorageFull', category: 'needs_action', title: 'Disk is full' },
        { name: 'ConnectionTimeout', category: 'transient', title: 'Connection timed out' },
        { name: 'Cancelled', category: 'transient', title: 'Cancelled' },
        { name: 'IoError (no errno)', category: 'serious', title: "Couldn't read this folder" },
    ]

    /** Maps provider display names to representative paths for provider detection. */
    const providerPathMap: Record<string, string> = {
        Dropbox: '~/Library/CloudStorage/Dropbox/test',
        'Google Drive': '~/Library/CloudStorage/GoogleDrive-user@gmail.com/test',
        OneDrive: '~/Library/CloudStorage/OneDrive-Personal/test',
        Box: '~/Library/CloudStorage/Box-Box/test',
        pCloud: '~/Library/CloudStorage/pCloud/test',
        Nextcloud: '~/Library/CloudStorage/Nextcloud/test',
        'Synology Drive': '~/Library/CloudStorage/SynologyDrive/test',
        Tresorit: '~/Library/CloudStorage/Tresorit/test',
        'Proton Drive': '~/Library/CloudStorage/ProtonDrive/test',
        'Sync.com': '~/Library/CloudStorage/Sync/test',
        Egnyte: '~/Library/CloudStorage/Egnyte/test',
        MacDroid: '~/Library/CloudStorage/MacDroid-device/test',
        'iCloud Drive': '~/Library/Mobile Documents/com~apple~CloudDocs/test',
        macFUSE: '/Volumes/sshfs-mount/test',
        VeraCrypt: '/Volumes/veracrypt1/test',
        'Cloud mount': '~/.CMVolumes/mount/test',
        'your cloud provider': '~/Library/CloudStorage/UnknownProvider/test',
    }

    async function triggerError(pane: 'left' | 'right', state: ErrorState) {
        const providerIndex = providerSelections[state.name] ?? 0
        const providerName = providerNames[providerIndex]
        const providerPath = providerName !== 'None' ? (providerPathMap[providerName] ?? null) : null

        try {
            const { invoke } = await import('@tauri-apps/api/core')
            const friendly = await invoke<FriendlyError>('preview_friendly_error', {
                errorCode: state.code ?? null,
                variant: state.code === undefined ? state.name : null,
                providerPath,
            })
            const { emitTo } = await import('@tauri-apps/api/event')
            await emitTo('main', 'debug-inject-error', { pane, friendly })
        } catch (error) {
            // eslint-disable-next-line no-console -- Debug window is dev-only
            console.error('preview_friendly_error failed:', error)
        }
    }

    async function resetErrors(pane: 'left' | 'right' | 'both') {
        try {
            const { emitTo } = await import('@tauri-apps/api/event')
            await emitTo('main', 'debug-reset-error', { pane })
        } catch {
            // Not in Tauri environment
        }
    }
</script>

<section class="debug-section">
    <h2>Error pane preview</h2>
    <div class="error-preview-panel">
        <div class="error-preview-actions">
            <button class="index-button" onclick={() => void resetErrors('both')}>Reset both panes</button>
        </div>

        <div class="error-group-header">Transient (errno)</div>
        {#each errnoErrors.filter((e) => e.category === 'transient') as state (state.name)}
            <div class="error-row">
                <span class="error-label" use:tooltip={{ text: state.title }}>
                    {state.name}{#if state.code !== undefined} ({state.code}){/if}
                    <span class="error-title">{state.title}</span>
                </span>
                <select
                    class="error-provider-select"
                    value={providerNames[providerSelections[state.name] ?? 0]}
                    onchange={(e) => {
                        const target = e.currentTarget
                        providerSelections[state.name] = providerNames.indexOf(target.value as (typeof providerNames)[number])
                    }}
                >
                    {#each providerNames as name (name)}
                        <option value={name}>{name}</option>
                    {/each}
                </select>
                <button class="error-trigger-btn" onclick={() => void triggerError('left', state)}>L</button>
                <button class="error-trigger-btn" onclick={() => void triggerError('right', state)}>R</button>
            </div>
        {/each}

        <div class="error-group-header">Needs action (errno)</div>
        {#each errnoErrors.filter((e) => e.category === 'needs_action') as state (state.name)}
            <div class="error-row">
                <span class="error-label" use:tooltip={{ text: state.title }}>
                    {state.name}{#if state.code !== undefined} ({state.code}){/if}
                    <span class="error-title">{state.title}</span>
                </span>
                <select
                    class="error-provider-select"
                    value={providerNames[providerSelections[state.name] ?? 0]}
                    onchange={(e) => {
                        const target = e.currentTarget
                        providerSelections[state.name] = providerNames.indexOf(target.value as (typeof providerNames)[number])
                    }}
                >
                    {#each providerNames as name (name)}
                        <option value={name}>{name}</option>
                    {/each}
                </select>
                <button class="error-trigger-btn" onclick={() => void triggerError('left', state)}>L</button>
                <button class="error-trigger-btn" onclick={() => void triggerError('right', state)}>R</button>
            </div>
        {/each}

        <div class="error-group-header">Serious (errno)</div>
        {#each errnoErrors.filter((e) => e.category === 'serious') as state (state.name)}
            <div class="error-row">
                <span class="error-label" use:tooltip={{ text: state.title }}>
                    {state.name}{#if state.code !== undefined} ({state.code}){/if}
                    <span class="error-title">{state.title}</span>
                </span>
                <select
                    class="error-provider-select"
                    value={providerNames[providerSelections[state.name] ?? 0]}
                    onchange={(e) => {
                        const target = e.currentTarget
                        providerSelections[state.name] = providerNames.indexOf(target.value as (typeof providerNames)[number])
                    }}
                >
                    {#each providerNames as name (name)}
                        <option value={name}>{name}</option>
                    {/each}
                </select>
                <button class="error-trigger-btn" onclick={() => void triggerError('left', state)}>L</button>
                <button class="error-trigger-btn" onclick={() => void triggerError('right', state)}>R</button>
            </div>
        {/each}

        <div class="error-group-header">VolumeError variants</div>
        {#each volumeErrors as state (state.name)}
            <div class="error-row">
                <span class="error-label" use:tooltip={{ text: state.title }}>
                    {state.name}
                    <span class="error-title">{state.title}</span>
                </span>
                <select
                    class="error-provider-select"
                    value={providerNames[providerSelections[state.name] ?? 0]}
                    onchange={(e) => {
                        const target = e.currentTarget
                        providerSelections[state.name] = providerNames.indexOf(target.value as (typeof providerNames)[number])
                    }}
                >
                    {#each providerNames as name (name)}
                        <option value={name}>{name}</option>
                    {/each}
                </select>
                <button class="error-trigger-btn" onclick={() => void triggerError('left', state)}>L</button>
                <button class="error-trigger-btn" onclick={() => void triggerError('right', state)}>R</button>
            </div>
        {/each}

        <div class="error-preview-actions">
            <button class="index-button" onclick={() => void resetErrors('both')}>Reset both panes</button>
        </div>
    </div>
</section>
