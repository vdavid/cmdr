module.exports = {
  ci: {
    collect: {
      startServerCommand: 'PORT=4322 pnpm preview',
      startServerReadyPattern: 'Local',
      url: ['http://localhost:4322/'],
      numberOfRuns: 3,
      // CI runs this inside the Playwright container (ci.yml's Website job, so the visual
      // baselines render where they were shot), which means Chrome runs as root and refuses to
      // start with its own sandbox. Harmless locally.
      settings: { chromeFlags: '--no-sandbox' },
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
