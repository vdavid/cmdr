<script lang="ts">
    import { onMount } from 'svelte'

    /** Delay before the loader becomes visible */
    const DELAY_MS = 200

    let visible = $state(false)
    let mounted = false

    onMount(() => {
        mounted = true
        const timer = setTimeout(() => {
            if (mounted) visible = true
        }, DELAY_MS)
        return () => {
            mounted = false
            clearTimeout(timer)
        }
    })
</script>

<div class="loading-container" class:visible>
    <div class="loader"></div>
    <div class="loading-text">Loading...</div>
</div>

<style>
    .loading-container {
        display: flex;
        flex-direction: column;
        align-items: center;
        justify-content: center;
        gap: 20px;
        width: 100%;
        height: 100%;
        opacity: 0;
        transition: opacity 0ms;
    }

    .loading-container.visible {
        opacity: 1;
        transition: opacity 500ms ease-in;
    }

    .loader {
        width: 48px;
        height: 48px;
        background: #a13200;
        display: block;
        position: relative;
        box-sizing: border-box;
        animation: rotationBack 1s ease-in-out infinite reverse;
    }

    .loader::before {
        content: '';
        box-sizing: border-box;
        left: 0;
        top: 0;
        transform: rotate(45deg);
        position: absolute;
        width: 48px;
        height: 48px;
        background: #a13200;
        box-shadow: 0 0 5px rgba(0, 0, 0, 0.15);
    }

    .loader::after {
        content: '';
        box-sizing: border-box;
        width: 32px;
        height: 32px;
        border-radius: 50%;
        position: absolute;
        left: 50%;
        top: 50%;
        background: #ff9e1b;
        transform: translate(-50%, -50%);
        box-shadow: 0 0 5px rgba(0, 0, 0, 0.15);
    }

    .loading-text {
        color: var(--color-text-secondary);
        animation: pulse 3s ease-in-out infinite;
    }

    @keyframes rotationBack {
        0% {
            transform: rotate(0deg);
        }
        100% {
            transform: rotate(-360deg);
        }
    }

    @keyframes pulse {
        0%,
        100% {
            transform: scale(1);
        }
        50% {
            transform: scale(1.1);
        }
    }
</style>
