# CSS health checks with Stylelint

We use Stylelint to validate CSS and catch common issues.

Checks for:

- Unused/hallucinated CSS variables
- Variables to follow the pattern `--color-*`, `--spacing-*`, or `--font-*`.
- CSS syntax errors
- General CSS best practices

Run it by `pnpm stylelint:fix` or via `./scripts/check.sh`

## Config

Stylelint is configured in `.stylelintrc.mjs` (JavaScript format) with:

1. **`postcss-html` syntax** - Allows parsing `<style>` blocks in `.svelte` files
2. **`stylelint-value-no-unknown-custom-properties` plugin** - The key plugin that validates CSS variables
3. **Absolute path to `src/app.css`** - Uses `import.meta.url` to compute an absolute path, which is required because
   the plugin resolves relative paths from the current working directory. IDEs may run stylelint from the linted file's
   directory rather than the project root, causing relative paths to fail.
