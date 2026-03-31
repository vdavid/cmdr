module.exports = {
  ci: {
    collect: {
      startServerCommand: 'PORT=4322 pnpm preview',
      startServerReadyPattern: 'Local',
      url: ['http://localhost:4322/'],
      numberOfRuns: 3,
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
