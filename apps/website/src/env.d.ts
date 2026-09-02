/// <reference types="astro/client" />
/// <reference types="unplugin-icons/types/astro" />

/**
 * The site's `PUBLIC_*` build-time variables, mirroring `.env.example`.
 *
 * Without these declarations `import.meta.env.PUBLIC_ANYTHING` is `any`, so a typo in a variable
 * name reads as `undefined` and the feature quietly turns itself off (an unset
 * `PUBLIC_DOWNLOAD_BASE_URL`, for instance, sends every download button to GitHub with no
 * telemetry). Every one is optional: builds run with most of them unset.
 */
interface ImportMetaEnv {
  readonly PUBLIC_DOWNLOAD_BASE_URL?: string
  readonly PUBLIC_LISTMONK_LIST_UUID?: string
  readonly PUBLIC_PADDLE_ALLOW_SANDBOX?: string
  readonly PUBLIC_PADDLE_CLIENT_TOKEN?: string
  readonly PUBLIC_PADDLE_ENVIRONMENT?: string
  readonly PUBLIC_PADDLE_PRICE_ID_COMMERCIAL_PERPETUAL?: string
  readonly PUBLIC_PADDLE_PRICE_ID_COMMERCIAL_SUBSCRIPTION?: string
  readonly PUBLIC_POSTHOG_HOST?: string
  readonly PUBLIC_POSTHOG_KEY?: string
  readonly PUBLIC_UMAMI_HOST?: string
  readonly PUBLIC_UMAMI_WEBSITE_ID?: string
}

interface ImportMeta {
  readonly env: ImportMetaEnv
}
