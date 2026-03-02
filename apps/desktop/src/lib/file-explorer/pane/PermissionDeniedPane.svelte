<script lang="ts">
    import { DotLottieSvelte } from '@lottiefiles/dotlottie-svelte'
    import { openPrivacySettings } from '$lib/tauri-commands'
    import { isMacOS } from '$lib/shortcuts/key-capture'
    import Button from '$lib/ui/Button.svelte'

    interface Props {
        folderPath: string
    }

    const { folderPath }: Props = $props()
</script>

<div class="permission-denied">
    <div class="content">
        <div class="icon"><DotLottieSvelte src="/icons/lock-closing.lottie" autoplay speed={0.5} /></div>
        <h2>No permission</h2>
        <p class="folder-path">{folderPath}</p>
        {#if isMacOS()}
            <p>If you want to see the content of this folder:</p>
            <ol>
                <li>Click <strong>Open System Settings</strong> below</li>
                <li>Click <strong>Files & Folders</strong> in the list</li>
                <li>Find <strong>Cmdr</strong> and toggle the switch for this folder.</li>
                <li>Confirm it and click <strong>Quit & Reopen</strong></li>
            </ol>
            <div class="cta">
                <Button variant="primary" onclick={() => openPrivacySettings()}>Open System Settings</Button>
            </div>
        {:else}
            <p>You don't have permission to read this folder. Check that your user has the right file permissions.</p>
        {/if}
    </div>
</div>

<style>
    .permission-denied {
        display: flex;
        align-items: center;
        justify-content: center;
        height: 100%;
        padding: var(--spacing-xl);
        line-height: 24px;
    }

    .content {
        max-width: 400px;
    }

    .icon {
        width: 96px;
        height: 96px;
        margin: 0 auto var(--spacing-lg);
    }

    h2 {
        font-size: var(--font-size-xl);
        font-weight: 600;
        margin: 0 0 var(--spacing-2xl) 0;
        text-align: center;
    }

    .folder-path {
        color: var(--color-text-secondary);
    }

    p {
        line-height: 1.5;
    }

    .cta {
        display: flex;
        justify-content: center;
        margin-top: var(--spacing-2xl);
    }
</style>
