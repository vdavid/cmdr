import { getSetting, resolveCloudConfig } from '$lib/settings'
import { configureAi } from '$lib/tauri-commands'
import { getAppLogger } from '$lib/logging/logger'

const logger = getAppLogger('ai-settings')

/** Push current AI config (provider, context size, cloud credentials) to the Rust backend. */
export async function pushConfigToBackend(): Promise<void> {
    try {
        const resolved = resolveCloudConfig(getSetting('ai.cloudProvider'), getSetting('ai.cloudProviderConfigs'))
        await configureAi(
            getSetting('ai.provider'),
            Number(getSetting('ai.localContextSize')),
            resolved.apiKey,
            resolved.baseUrl,
            resolved.model,
        )
    } catch (e) {
        logger.error("Couldn't push AI config to backend: {error}", { error: e })
    }
}
