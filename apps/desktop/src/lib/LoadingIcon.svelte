<div class="loading-container">
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
        animation: fadeIn 400ms ease-in;
    }

    @keyframes fadeIn {
        0%,
        50% {
            opacity: 0;
        }
        100% {
            opacity: 1;
        }
    }

    .loader {
        width: 50px;
        height: 50px;
        position: relative;
    }

    .loader:before,
    .loader:after {
        content: '';
        border-radius: 50%;
        position: absolute;
        inset: 0;
        box-shadow: 0 0 10px 2px rgba(0, 0, 0, 0.3) inset;
    }

    .loader:after {
        box-shadow: 0 2px 0 #ff9e1b inset;
        animation: rotate 2s linear infinite;
    }

    @media (prefers-color-scheme: dark) {
        .loader:after {
            box-shadow: 0 2px 0 #a13200 inset;
        }
    }

    @keyframes rotate {
        0% {
            transform: rotate(0);
        }

        100% {
            transform: rotate(360deg);
        }
    }

    .loading-text {
        color: var(--color-text-secondary);
        animation: pulse 3s ease-in-out infinite;
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
