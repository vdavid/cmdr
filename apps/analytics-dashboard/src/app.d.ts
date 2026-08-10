declare global {
  namespace App {
    interface Locals {
      /** Email from the verified Cloudflare Access JWT, set by `hooks.server.ts`. */
      email?: string
    }
    interface Platform {
      env?: {
        UMAMI_API_URL: string
        UMAMI_USERNAME: string
        UMAMI_PASSWORD: string
        UMAMI_WEBSITE_ID: string
        UMAMI_BLOG_WEBSITE_ID: string
        UMAMI_PRVW_WEBSITE_ID: string
        PADDLE_API_KEY_LIVE: string
        POSTHOG_API_KEY: string
        POSTHOG_PROJECT_ID: string
        POSTHOG_API_URL: string
        GITHUB_TOKEN?: string
        LICENSE_SERVER_ADMIN_TOKEN: string
        /** Optional local-QA override for the api-server base URL (defaults to production). */
        WORKER_BASE_URL?: string
      }
    }
  }
}

export {}
