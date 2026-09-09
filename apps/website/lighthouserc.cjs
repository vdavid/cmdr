module.exports = {
  ci: {
    collect: {
      startServerCommand: 'PORT=4322 pnpm preview',
      startServerReadyPattern: 'Local',
      url: ['http://localhost:4322/'],
      numberOfRuns: 3,
      // CI runs this inside the Playwright container (ci.yml's Website job, so the visual
      // baselines render where they were shot), which means Chrome runs as root and refuses to
      // start with its own sandbox. `--disable-dev-shm-usage` sends Chrome's shared memory to
      // `/tmp` instead of the 64 MB `/dev/shm` a container gets by default, which the renderer
      // can exhaust and crash on (`Inspector.targetCrashed`, surfacing as CHROME_INTERSTITIAL_ERROR
      // on a page that loads fine). The Playwright run in that same container passes both flags
      // itself, which is why it stays green while this lane doesn't. Harmless locally.
      settings: { chromeFlags: '--no-sandbox --disable-dev-shm-usage' },
    },
    assert: {
      assertions: {
        // Performance
        'categories:performance': ['warn', { minScore: 0.9 }],

        // Accessibility
        'categories:accessibility': ['error', { minScore: 0.9 }],

        // Best practices
        'categories:best-practices': ['warn', { minScore: 0.9 }],

        // SEO
        'categories:seo': ['error', { minScore: 0.9 }],

        // Specific checks
        'first-contentful-paint': ['warn', { maxNumericValue: 1800 }],
        'largest-contentful-paint': ['warn', { maxNumericValue: 2500 }],
        'cumulative-layout-shift': ['warn', { maxNumericValue: 0.1 }],
      },
    },
    upload: {
      target: 'temporary-public-storage',
    },
  },
}
