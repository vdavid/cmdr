<script lang="ts">
    import { onMount, onDestroy } from 'svelte'
    import SettingRow from '../components/SettingRow.svelte'
    import SettingSelect from '../components/SettingSelect.svelte'
    import Button from '$lib/ui/Button.svelte'
    import ModalDialog from '$lib/ui/ModalDialog.svelte'
    import Spinner from '$lib/ui/Spinner.svelte'
    import SectionCard from '$lib/ui/SectionCard.svelte'
    import { tooltip } from '$lib/tooltip/tooltip'
    import { getSetting, onSpecificSettingChange } from '$lib/settings'
    import {
        getAiRuntimeStatus,
        stopAiServer,
        startAiServer,
        startAiDownload,
        cancelAiDownload,
        uninstallAi,
        formatBytes,
        getSystemMemoryInfo,
        onAiServerReady,
        onAiExtracting,
        onAiDownloadProgress,
        onAiVerifying,
        onAiInstalling,
        onAiInstallComplete,
        onAiStarting,
        type AiRuntimeStatus,
        type AiDownloadProgress,
        type SystemMemoryInfo,
    } from '$lib/tauri-commands'
    import { computeGaugeSegments } from './ram-gauge-utils'
    import { getAppLogger } from '$lib/logging/logger'
    import { colorizeSizeString } from '$lib/file-explorer/selection/selection-info-utils'
    import { t, tString } from '$lib/intl/messages.svelte'

    interface Props {
        searchQuery: string
        shouldShow: (id: string) => boolean
        status: AiRuntimeStatus | null
    }

    const { searchQuery, shouldShow, status: initialStatus }: Props = $props()

    const logger = getAppLogger('ai-settings-local')

    // Dynamic state from backend
    let status = $state<AiRuntimeStatus | null>(initialStatus)
    let showDeleteConfirm = $state(false)
    let isDeleting = $state(false)
    let downloadProgress = $state<AiDownloadProgress | null>(null)
    let isRestarting = $state(false)

    // Multi-step install tracking
    type InstallStep = 'extracting' | 'downloading' | 'verifying' | 'starting' | null
    let installStep = $state<InstallStep>(null)
    let downloadCancelledByUser = $state(false)

    // Track context size for memory estimate
    let pendingContextSize = $state(Number(getSetting('ai.localContextSize')))

    // The context size the server is actually running with (set from backend status on mount,
    // updated after successful Apply)
    let activeContextSize = $state(Number(getSetting('ai.localContextSize')))

    // System memory info for the RAM gauge
    let systemMemory = $state<SystemMemoryInfo | null>(null)
    let memoryPollInterval: ReturnType<typeof setInterval> | null = null

    // Event listeners cleanup
    const unlistenFns: Array<() => void> = []

    onMount(async () => {
        // If server is running, activeContextSize is what the server started with.
        // The backend doesn't expose the running context size directly, so we use the
        // current setting as the best approximation on mount.
        if (status?.serverRunning) {
            activeContextSize = pendingContextSize
        }

        // Subscribe to context size changes (update pending, no auto-restart)
        const unsubCtx = onSpecificSettingChange('ai.localContextSize', (_id, newValue) => {
            pendingContextSize = Number(newValue)
        })
        unlistenFns.push(unsubCtx)

        // Listen for backend events
        const unlistenReady = await onAiServerReady(() => {
            isRestarting = false
            activeContextSize = pendingContextSize
            void refreshStatus()
        })
        unlistenFns.push(unlistenReady)

        const unlistenExtracting = await onAiExtracting(() => {
            installStep = 'extracting'
        })
        unlistenFns.push(unlistenExtracting)

        const unlistenProgress = await onAiDownloadProgress((payload) => {
            downloadProgress = payload
            if (installStep !== 'downloading') {
                installStep = 'downloading'
            }
        })
        unlistenFns.push(unlistenProgress)

        const unlistenVerifying = await onAiVerifying(() => {
            installStep = 'verifying'
        })
        unlistenFns.push(unlistenVerifying)

        const unlistenInstalling = await onAiInstalling(() => {
            installStep = 'starting'
        })
        unlistenFns.push(unlistenInstalling)

        const unlistenInstallComplete = await onAiInstallComplete(() => {
            installStep = null
            downloadProgress = null
            void refreshStatus()
        })
        unlistenFns.push(unlistenInstallComplete)

        const unlistenStarting = await onAiStarting(() => {
            void refreshStatus()
        })
        unlistenFns.push(unlistenStarting)

        // Poll system memory every 5 seconds
        await pollSystemMemory()
        memoryPollInterval = setInterval(() => void pollSystemMemory(), 5000)
    })

    onDestroy(() => {
        for (const fn of unlistenFns) {
            fn()
        }
        if (memoryPollInterval) {
            clearInterval(memoryPollInterval)
        }
    })

    async function pollSystemMemory(): Promise<void> {
        try {
            systemMemory = await getSystemMemoryInfo()
        } catch (e) {
            logger.error("Couldn't get system memory info: {error}", { error: e })
        }
    }

    async function refreshStatus(): Promise<void> {
        try {
            status = await getAiRuntimeStatus()
        } catch (e) {
            logger.error("Couldn't refresh AI status: {error}", { error: e })
        }
    }

    async function performContextRestart(): Promise<void> {
        isRestarting = true
        try {
            await stopAiServer()
            const ctxSize = Number(getSetting('ai.localContextSize'))
            await startAiServer(ctxSize)
        } catch (e) {
            logger.error("Couldn't restart AI server: {error}", { error: e })
            isRestarting = false
        }
        await refreshStatus()
    }

    async function handleApplyContextSize(): Promise<void> {
        await performContextRestart()
    }

    async function handleStartServer(): Promise<void> {
        try {
            const ctxSize = Number(getSetting('ai.localContextSize'))
            await startAiServer(ctxSize)
            activeContextSize = ctxSize
            await refreshStatus()
        } catch (e) {
            logger.error("Couldn't start AI server: {error}", { error: e })
        }
    }

    async function handleStopServer(): Promise<void> {
        try {
            await stopAiServer()
            await refreshStatus()
        } catch (e) {
            logger.error("Couldn't stop AI server: {error}", { error: e })
        }
    }

    async function handleDownloadModel(): Promise<void> {
        downloadCancelledByUser = false
        installStep = 'extracting'
        downloadProgress = { bytesDownloaded: 0, totalBytes: 0, speed: 0, etaSeconds: 0 }
        try {
            await startAiDownload()
        } catch (e) {
            // eslint-disable-next-line @typescript-eslint/no-unnecessary-condition -- set by handleCancelDownload() during the await
            if (downloadCancelledByUser) {
                logger.info('AI download cancelled by user')
            } else {
                logger.error("Couldn't start AI download: {error}", { error: e })
            }
            downloadProgress = null
            installStep = null
        }
        await refreshStatus()
    }

    async function handleCancelDownload(): Promise<void> {
        downloadCancelledByUser = true
        try {
            await cancelAiDownload()
        } catch (e) {
            logger.error("Couldn't cancel AI download: {error}", { error: e })
        }
        installStep = null
        downloadProgress = null
        await refreshStatus()
    }

    async function handleDeleteModel(): Promise<void> {
        isDeleting = true
        try {
            await uninstallAi()
        } catch (e) {
            logger.error("Couldn't delete AI model: {error}", { error: e })
        }
        isDeleting = false
        showDeleteConfirm = false
        await refreshStatus()
    }

    // Derived state
    const modelInstalled = $derived(status?.modelInstalled ?? false)
    const serverRunning = $derived(status?.serverRunning ?? false)
    const serverStarting = $derived(status?.serverStarting ?? false)

    // Memory estimate
    const kvBytesPerToken = $derived(status?.kvBytesPerToken ?? 106496)
    const baseOverheadBytes = $derived(status?.baseOverheadBytes ?? 3500000000)

    // Current AI memory (what the server is actually using now)
    const currentAiMemoryBytes = $derived(
        serverRunning || serverStarting ? kvBytesPerToken * activeContextSize + baseOverheadBytes : 0,
    )

    // Projected AI memory (what the pending context size would use)
    const projectedAiMemoryBytes = $derived(kvBytesPerToken * pendingContextSize + baseOverheadBytes)
    const projectedMemoryFormatted = $derived(formatMemoryEstimate(projectedAiMemoryBytes))

    // Whether the Apply button should be visible
    const showApplyButton = $derived(pendingContextSize !== activeContextSize && serverRunning && !isRestarting)

    // RAM gauge segments (percentages of total RAM)
    const gaugeSegments = $derived(
        systemMemory ? computeGaugeSegments(systemMemory, currentAiMemoryBytes, projectedAiMemoryBytes) : null,
    )

    // Warning state based on projected usage
    const warningLevel = $derived.by((): 'none' | 'caution' | 'danger' => {
        if (!gaugeSegments) return 'none'
        if (gaugeSegments.totalProjectedUsageRatio > 0.9) return 'danger'
        if (gaugeSegments.totalProjectedUsageRatio > 0.7) return 'caution'
        return 'none'
    })

    const warningTooltip = $derived.by(() => {
        if (warningLevel === 'danger') return tString('ai.local.warningDanger')
        if (warningLevel === 'caution') return tString('ai.local.warningCaution')
        return ''
    })

    // Server status text
    const serverStatusText = $derived.by(() => {
        if (isRestarting) return tString('ai.local.statusRestarting')
        if (serverStarting) return tString('ai.local.statusStarting')
        if (serverRunning) return tString('ai.local.statusRunning')
        return tString('ai.local.statusStopped')
    })

    const serverStatusDetail = $derived.by(() => {
        if (!serverRunning || !status?.pid) return ''
        const parts = [`PID ${String(status.pid)}`]
        if (status.port) parts.push(`port ${String(status.port)}`)
        return parts.join(' \u00b7 ')
    })

    // Download progress
    const downloadPercent = $derived(
        downloadProgress && downloadProgress.totalBytes > 0
            ? Math.round((downloadProgress.bytesDownloaded / downloadProgress.totalBytes) * 100)
            : 0,
    )
    const downloadProgressText = $derived.by(() => {
        if (!downloadProgress) return ''
        if (downloadProgress.totalBytes === 0) return tString('ai.local.startingDownload')
        const downloaded = colorizeSizeString(formatBytes(downloadProgress.bytesDownloaded))
        const total = colorizeSizeString(formatBytes(downloadProgress.totalBytes))
        const speed = colorizeSizeString(formatBytes(downloadProgress.speed))
        const eta = formatEta(downloadProgress.etaSeconds)
        const parts = [`${String(downloadPercent)}%`, `${downloaded} / ${total}`, `${speed}/s`]
        if (eta) parts.push(eta)
        return parts.join(' \u00b7 ')
    })

    // Install step display
    const installStepLabel = $derived.by(() => {
        switch (installStep) {
            case 'extracting':
                return tString('ai.local.installStepExtracting')
            case 'downloading':
                return tString('ai.local.installStepDownloading')
            case 'verifying':
                return tString('ai.local.installStepVerifying')
            case 'starting':
                return tString('ai.local.installStepStarting')
            default:
                return ''
        }
    })

    // Whether actions should be disabled (during starting/restarting)
    const actionsDisabled = $derived(serverStarting || isRestarting)

    function formatEta(seconds: number): string {
        if (seconds <= 0) return ''
        if (seconds < 60) return tString('ai.local.etaSeconds', { value: String(Math.ceil(seconds)) })
        if (seconds < 3600) return tString('ai.local.etaMinutes', { value: String(Math.ceil(seconds / 60)) })
        return tString('ai.local.etaHours', { value: String(Math.round(seconds / 3600)) })
    }

    function formatMemoryEstimate(bytes: number): string {
        const gb = bytes / (1024 * 1024 * 1024)
        return `~${gb.toFixed(1)} GB`
    }

    function formatMemoryGb(bytes: number): string {
        const gb = bytes / (1024 * 1024 * 1024)
        return `${gb.toFixed(1)} GB`
    }
</script>

<!-- Status card -->
<div class="status-card">
    {#if installStep !== null}
        <div class="install-step-label">{installStepLabel}</div>
        {#if installStep === 'downloading'}
            <div class="progress-bar-container">
                <div class="progress-bar-fill" style="width: {String(downloadPercent)}%"></div>
            </div>
            <!-- eslint-disable-next-line svelte/no-at-html-tags -- Markup built from formatBytes + tier classes; no user input. -->
            <span class="progress-text">{@html downloadProgressText}</span>
        {/if}
    {:else if modelInstalled}
        <div class="status-row">
            <span class="status-label">{tString('ai.local.modelLabel')}</span>
            <span class="status-value"
                >{status?.modelName ?? tString('ai.local.modelUnknown')} ({status?.modelSizeFormatted ??
                    tString('ai.local.modelSizeUnknown')})</span
            >
        </div>
        <div class="status-row">
            <span class="status-label">{tString('ai.local.serverLabel')}</span>
            <span
                class="status-value"
                class:status-running={serverRunning && !isRestarting && !serverStarting}
                class:status-stopped={!serverRunning && !serverStarting && !isRestarting}
            >
                {serverStatusText}{#if serverStatusDetail}
                    <span class="status-detail">&middot; {serverStatusDetail}</span>{/if}
            </span>
        </div>
    {:else}
        <p class="not-installed-text">
            {t('ai.local.notInstalled', {
                modelName: status?.modelName ?? 'Ministral 3B',
                modelSize: status?.modelSizeFormatted ?? '2.0 GB',
            })}
        </p>
    {/if}
</div>

<!-- Context window (only when installed). Card-framed to match the rest of
     Settings; the status card, RAM gauge actions, and modal below stay outside
     a card (already visually distinct full-bleed blocks). -->
{#if modelInstalled && shouldShow('ai.localContextSize')}
    <SectionCard>
        <SettingRow
            id="ai.localContextSize"
            label={tString('settings.ai.localContextSize.label')}
            description={tString('settings.ai.localContextSize.description')}
            split
            {searchQuery}
        >
            <div class="context-size-controls">
                <SettingSelect id="ai.localContextSize" />
                {#if showApplyButton}
                    <div class="apply-wrapper">
                        {#if warningLevel === 'caution'}
                            <span
                                class="warning-icon warning-caution"
                                use:tooltip={warningTooltip}
                                aria-label={tString('ai.local.memoryWarningAria')}
                            >
                                &#x26A0;
                            </span>
                        {:else if warningLevel === 'danger'}
                            <span
                                class="warning-icon warning-danger"
                                use:tooltip={warningTooltip}
                                aria-label={tString('ai.local.memoryWarningAria')}
                            >
                                &#x26A0;
                            </span>
                        {/if}
                        <Button
                            variant="primary"
                            size="mini"
                            disabled={actionsDisabled}
                            onclick={() => void handleApplyContextSize()}
                        >
                            {tString('ai.local.applyContextSize')}
                        </Button>
                    </div>
                {/if}
            </div>
        </SettingRow>

        <!-- RAM gauge -->
        {#if systemMemory && systemMemory.totalBytes > 0 && gaugeSegments}
            <div class="ram-gauge-container" aria-label={tString('ai.local.ramGaugeAria')}>
                <div class="ram-gauge-bar">
                    <div class="ram-segment ram-system" style="width: {gaugeSegments.systemPercent.toFixed(2)}%"></div>
                    <div
                        class="ram-segment ram-other-apps"
                        style="width: {gaugeSegments.otherAppsPercent.toFixed(2)}%"
                    ></div>
                    <div
                        class="ram-segment ram-current-ai"
                        style="width: {gaugeSegments.retainedAiPercent.toFixed(2)}%"
                    ></div>
                    {#if gaugeSegments.addedPercent > 0}
                        <div
                            class="ram-segment ram-projected"
                            style="width: {gaugeSegments.addedPercent.toFixed(2)}%"
                        ></div>
                    {/if}
                    {#if gaugeSegments.freedPercent > 0}
                        <div
                            class="ram-segment ram-freed"
                            style="width: {gaugeSegments.freedPercent.toFixed(2)}%"
                        ></div>
                    {/if}
                </div>
                <div class="ram-legend">
                    <span class="ram-legend-item"
                        ><span class="ram-legend-swatch ram-system"></span>{t('ai.local.ramLegendSystem', {
                            size: formatMemoryGb(gaugeSegments.systemBytes),
                        })}</span
                    >
                    <span class="ram-legend-item"
                        ><span class="ram-legend-swatch ram-other-apps"></span>{t('ai.local.ramLegendApps', {
                            size: formatMemoryGb(gaugeSegments.otherAppsBytes),
                        })}</span
                    >
                    <span class="ram-legend-item"
                        ><span class="ram-legend-swatch ram-current-ai"></span>{t('ai.local.ramLegendCmdrAi', {
                            size: projectedMemoryFormatted,
                        })}</span
                    >
                    {#if gaugeSegments.addedPercent > 0}
                        <span class="ram-legend-item"
                            ><span class="ram-legend-swatch ram-projected"></span>{tString(
                                'ai.local.ramLegendProjected',
                            )}</span
                        >
                    {/if}
                    {#if gaugeSegments.freedPercent > 0}
                        <span class="ram-legend-item"
                            ><span class="ram-legend-swatch ram-freed"></span>{tString('ai.local.ramLegendFreed')}</span
                        >
                    {/if}
                    <span class="ram-legend-item"
                        ><span class="ram-legend-swatch ram-free-space"></span>{t('ai.local.ramLegendFree', {
                            size: formatMemoryGb(gaugeSegments.freeBytes),
                        })}</span
                    >
                </div>
            </div>
        {/if}
    </SectionCard>
{/if}

<!-- Actions -->
<div class="actions">
    {#if installStep === 'extracting' || installStep === 'downloading'}
        <Button variant="secondary" onclick={() => void handleCancelDownload()}>{tString('ai.local.cancel')}</Button>
    {:else if installStep !== null}
        <!-- Verifying/starting: can't cancel after download completes -->
    {:else if modelInstalled}
        {#if serverRunning}
            <Button variant="secondary" disabled={actionsDisabled} onclick={() => void handleStopServer()}
                >{tString('ai.local.stopServer')}</Button
            >
        {:else}
            <Button variant="secondary" disabled={actionsDisabled} onclick={() => void handleStartServer()}
                >{tString('ai.local.startServer')}</Button
            >
        {/if}
        <Button variant="danger" disabled={actionsDisabled} onclick={() => (showDeleteConfirm = true)}
            >{tString('ai.local.deleteModel')}</Button
        >
    {:else}
        <Button variant="secondary" onclick={() => void handleDownloadModel()}
            >{tString('ai.local.downloadModel')}</Button
        >
    {/if}
</div>

<!-- Delete model confirmation dialog -->
{#if showDeleteConfirm}
    <ModalDialog
        titleId="delete-ai-model-title"
        dialogId="delete-ai-model"
        role="alertdialog"
        onclose={() => {
            if (!isDeleting) showDeleteConfirm = false
        }}
        containerStyle="width: 400px"
        onkeydown={(e: KeyboardEvent) => {
            if (e.key === 'Enter' && !isDeleting) {
                void handleDeleteModel()
            }
        }}
    >
        {#snippet title()}{isDeleting
                ? tString('ai.local.deleteDialogTitleDeleting')
                : tString('ai.local.deleteDialogTitle')}{/snippet}
        <div class="confirm-body">
            {#if isDeleting}
                <div class="deleting-status">
                    <Spinner size="sm" />
                    <span>{tString('ai.local.deletingStatus')}</span>
                </div>
            {:else}
                <p class="confirm-message">
                    {t('ai.local.deleteConfirmMessage', { modelSize: status?.modelSizeFormatted ?? '2.0 GB' })}
                </p>
            {/if}
        </div>
        {#snippet footer()}
            <Button variant="secondary" disabled={isDeleting} onclick={() => (showDeleteConfirm = false)}
                >{tString('ai.local.cancel')}</Button
            >
            <Button variant="danger" disabled={isDeleting} onclick={() => void handleDeleteModel()}>
                {isDeleting ? tString('ai.local.deleteButtonDeleting') : tString('ai.local.deleteButton')}
            </Button>
        {/snippet}
    </ModalDialog>
{/if}

<style>
    /* Status card */
    .status-card {
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-lg);
        padding: var(--spacing-xs) var(--spacing-lg);
        margin-bottom: var(--spacing-lg);
    }

    .status-row {
        display: flex;
        justify-content: space-between;
        align-items: center;
        gap: var(--spacing-xl);
        padding: var(--spacing-sm) 0;
    }

    .status-row:not(:last-child) {
        border-bottom: 1px solid var(--color-border-subtle);
    }

    .status-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        flex-shrink: 0;
    }

    .status-value {
        font-size: var(--font-size-sm);
        color: var(--color-text-primary);
        font-weight: 500;
        text-align: right;
    }

    .status-detail {
        color: var(--color-text-secondary);
        font-weight: 400;
    }

    .status-running {
        color: var(--color-allow);
    }

    .status-stopped {
        color: var(--color-text-tertiary);
    }

    .not-installed-text {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        line-height: 1.4;
        margin: var(--spacing-xs) 0;
    }

    /* Install step */
    .install-step-label {
        font-size: var(--font-size-sm);
        color: var(--color-text-secondary);
        padding: var(--spacing-sm) 0 var(--spacing-xs);
    }

    /* Progress bar */
    .progress-bar-container {
        width: 100%;
        height: 4px;
        background: var(--color-bg-tertiary);
        border-radius: var(--radius-xs);
        overflow: hidden;
        margin: var(--spacing-xs) 0;
    }

    .progress-bar-fill {
        height: 100%;
        background: var(--color-accent);
        border-radius: var(--radius-xs);
        transition: width var(--transition-slow);
    }

    .progress-text {
        font-size: var(--font-size-xs);
        color: var(--color-text-secondary);
        font-variant-numeric: tabular-nums;
    }

    /* Context size controls with Apply button */
    .context-size-controls {
        display: flex;
        align-items: center;
        gap: var(--spacing-sm);
    }

    .apply-wrapper {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
    }

    .warning-icon {
        font-size: var(--font-size-md);
        line-height: 1;
        cursor: default;
    }

    .warning-caution {
        color: var(--color-warning);
    }

    .warning-danger {
        color: var(--color-error);
    }

    /* RAM gauge */
    .ram-gauge-container {
        margin: var(--spacing-xs) 0 0;
    }

    .ram-gauge-bar {
        display: flex;
        width: 100%;
        height: 5px;
        border-radius: var(--radius-sm);
        overflow: hidden;
        background: var(--color-bg-secondary);
    }

    .ram-segment {
        height: 100%;
        flex-shrink: 0;
        min-width: 0;
    }

    .ram-system {
        background: var(--color-border);
    }

    .ram-other-apps {
        background: var(--color-text-tertiary);
    }

    .ram-current-ai {
        background: var(--color-accent);
    }

    .ram-projected {
        background: var(--color-accent);
        opacity: 0.5;
    }

    .ram-freed {
        background: var(--color-allow);
        opacity: 0.5;
    }

    .ram-legend {
        display: flex;
        flex-wrap: wrap;
        gap: var(--spacing-sm) var(--spacing-md);
        margin-top: var(--spacing-xs);
    }

    .ram-legend-item {
        display: flex;
        align-items: center;
        gap: var(--spacing-xs);
        font-size: var(--font-size-xs);
        color: var(--color-text-tertiary);
    }

    .ram-legend-swatch {
        display: inline-block;
        width: 8px;
        height: 8px;
        border-radius: var(--radius-xs);
    }

    .ram-free-space {
        background: var(--color-bg-secondary);
        border: 1px solid var(--color-border-subtle);
    }

    /* Actions */
    .actions {
        display: flex;
        gap: var(--spacing-md);
        margin-top: var(--spacing-sm);
    }

    /* Delete confirmation dialog */
    .confirm-body {
        padding: 0 var(--spacing-xl);
    }

    .confirm-message {
        margin: 0;
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
        line-height: 1.4;
    }

    .deleting-status {
        display: flex;
        align-items: center;
        justify-content: center;
        gap: var(--spacing-sm);
        margin: 0 0 var(--spacing-lg);
        font-size: var(--font-size-md);
        color: var(--color-text-secondary);
    }
</style>
