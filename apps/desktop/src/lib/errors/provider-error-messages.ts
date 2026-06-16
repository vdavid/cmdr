/**
 * Provider-overlay friendly-error copy: (provider, category) → suggestion.
 *
 * `provider.rs` detects which cloud/mount provider manages a path (path patterns
 * + `statfs`); that detection stays in Rust and ships a typed `provider` on the
 * listing-error payload. When a provider is present, the FE replaces the base
 * reason's suggestion with the provider-specific one here, reproducing the old
 * Rust `enrich_with_provider` override exactly. Provider display names and app
 * names are words, so they live in the catalog too.
 *
 * `category` is the base reason's `ErrorCategory` (`transient` | `needs_action`
 * | `serious`), which the old Rust code keyed on.
 *
 * The literal English lives in the `errors.provider.*` message catalog and is
 * pulled via `getMessage()` (a RAW catalog lookup, never ICU `t()`): these
 * strings carry markdown and bypass ICU. The shared app-backed template carries
 * `{name}` / `{app}` tokens substituted here; provider names are static
 * (trusted), so no escaping is needed.
 */

import { getMessage } from '$lib/intl/messages.svelte'
import type { MessageKey } from '$lib/intl/keys.gen'

/** Serialized camelCase from Rust `Provider`. */
export type Provider =
  | 'dropbox'
  | 'googleDrive'
  | 'oneDrive'
  | 'box'
  | 'pCloud'
  | 'nextcloud'
  | 'synologyDrive'
  | 'tresorit'
  | 'protonDrive'
  | 'sync'
  | 'egnyte'
  | 'macDroid'
  | 'iCloud'
  | 'pCloudFuse'
  | 'macFuse'
  | 'veraCrypt'
  | 'cmVolumes'
  | 'genericCloudStorage'

export type ProviderCategory = 'transient' | 'needs_action' | 'serious'

/** Provider display name (the `**bold**` name shown in suggestions). Catalog copy. */
function displayName(p: Provider): string {
  return getMessage(`errors.provider.${p}.displayName`)
}

/**
 * App name for the shared app-backed template, or `null` for providers without a
 * single distinct app (`macFuse`, `iCloud`, `cmVolumes`, `genericCloudStorage`),
 * whose copy never references one. Catalog copy.
 */
function appName(p: Provider): string | null {
  return PROVIDERS_WITHOUT_APP_NAME.has(p) ? null : getMessage(`errors.provider.${p}.appName` as MessageKey)
}

/** Providers whose copy references no single distinct app (no `appName` catalog key). */
const PROVIDERS_WITHOUT_APP_NAME = new Set<Provider>(['macFuse', 'iCloud', 'cmVolumes', 'genericCloudStorage'])

/**
 * Providers with their own bespoke suggestion table (one catalog key per
 * category, or the collapsed `nonTransient` variant). Everything else uses the
 * shared `errors.provider.appBased.*` template with `{name}` / `{app}` tokens.
 */
const BESPOKE_PROVIDERS = new Set<Provider>([
  'macDroid',
  'iCloud',
  'macFuse',
  'pCloudFuse',
  'veraCrypt',
  'cmVolumes',
  'genericCloudStorage',
])

/** Providers that collapse needs_action + serious into one `nonTransient` message. */
const COLLAPSED_CATEGORY_PROVIDERS = new Set<Provider>(['cmVolumes', 'genericCloudStorage'])

/** The catalog leaf for a provider category (`needs_action` → `needsAction`). */
function categoryLeaf(category: ProviderCategory): string {
  return category === 'needs_action' ? 'needsAction' : category
}

/** Substitutes `{name}` / `{app}` tokens in a catalog template (names are trusted, unescaped). */
function fillTemplate(template: string, name: string, app: string): string {
  return template.replaceAll('{name}', name).replaceAll('{app}', app)
}

/**
 * Builds the provider-specific suggestion from the `errors.provider.*` catalog
 * (1:1 with the old Rust `provider_suggestion`). Provider names are static
 * (trusted), so no escaping is needed.
 */
export function getProviderSuggestion(provider: Provider, category: ProviderCategory): string {
  const name = displayName(provider)

  if (BESPOKE_PROVIDERS.has(provider)) {
    const leaf = COLLAPSED_CATEGORY_PROVIDERS.has(provider)
      ? category === 'transient'
        ? 'transient'
        : 'nonTransient'
      : categoryLeaf(category)
    return fillTemplate(getMessage(`errors.provider.${provider}.${leaf}` as MessageKey), name, name)
  }

  // App-backed providers share one template keyed only on category.
  const app = appName(provider) ?? name
  return fillTemplate(getMessage(`errors.provider.appBased.${categoryLeaf(category)}` as MessageKey), name, app)
}
