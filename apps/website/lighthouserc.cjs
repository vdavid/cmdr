module.exports = {
    ci: {
        collect: {
            startServerCommand: 'pnpm preview -- --port 4322',
            startServerReadyPattern: 'Local',
            url: ['http://localhost:4322/'],
            numberOfRuns: 3,
        },
        assert: {
            assertions: {
                // Performance
                'categories:performance': ['warn', { minScore: 0.8 }],

                // Accessibility
                'categories:accessibility': ['error', { minScore: 0.9 }],

                // Best practices
                'categories:best-practices': ['warn', { minScore: 0.9 }],

                // SEO
                'categories:seo': ['error', { minScore: 0.9 }],

                // Specific checks
                'first-contentful-paint': ['warn', { maxNumericValue: 2000 }],
                'largest-contentful-paint': ['warn', { maxNumericValue: 3000 }],
                'cumulative-layout-shift': ['warn', { maxNumericValue: 0.1 }],
            },
        },
        upload: {
            target: 'temporary-public-storage',
        },
    },
}
