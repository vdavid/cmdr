<script lang="ts">
    import { DotLottieSvelte } from '@lottiefiles/dotlottie-svelte'
    import { openPrivacySettings } from '$lib/tauri-commands'

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
        <p>If you want to see the content of this folder:</p>
        <ol>
            <li>Click <strong>Open System Settings</strong> below</li>
            <li>Click <strong>Files & Folders</strong> in the list</li>
            <li>Find <strong>Cmdr</strong> and toggle the switch for this folder.</li>
            <li>Confirm it and click <strong>Quit & Reopen</strong></li>
        </ol>
        <button onclick={() => openPrivacySettings()}>Open System Settings</button>
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

    button {
        padding: 7px 20px;
        border-radius: var(--radius-md);
        font-size: var(--font-size-md);
        font-weight: 500;
        background: var(--color-allow);
        color: white;
        border: none;
        display: block;
        margin: var(--spacing-2xl) auto 0;
        transition: all var(--transition-base);
    }

    button:hover {
        background: color-mix(in oklch, var(--color-allow), white 15%);
    }

    button:focus-visible {
        outline: 2px solid var(--color-allow);
        outline-offset: 1px;
        box-shadow: 0 0 0 4px rgba(0, 0, 0, 0.1);
    }

    @media (prefers-color-scheme: dark) {
        button:focus-visible {
            box-shadow: 0 0 0 4px rgba(255, 255, 255, 0.08);
        }
    }
</style>
