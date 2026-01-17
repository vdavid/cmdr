# CSS health checks

We have two complementary tools for CSS quality:

1. **Stylelint** - Validates CSS syntax and catches undefined variables
2. **CSS unused checker** - Finds unused/undefined CSS classes and variables

## Stylelint

Checks for:

- Undefined CSS variables (typos, hallucinations)
- Variable naming pattern `--color-*`, `--spacing-*`, or `--font-*`
- CSS syntax errors
- General CSS best practices

Run: `pnpm stylelint:fix` or via `./scripts/check.sh`

### Config

Stylelint is configured in `.stylelintrc.mjs` with:

1. **`postcss-html` syntax** - Parses `<style>` blocks in `.svelte` files
2. **`stylelint-value-no-unknown-custom-properties` plugin** - Validates CSS variables
3. **Absolute path to `src/app.css`** - Uses `import.meta.url` for IDE compatibility

## CSS unused checker

A Go-based tool that detects:

- **Unused CSS variables** - Defined in `app.css` but never used with `var(--name)`
- **Unused CSS classes** - Defined in `<style>` but never used in templates
- **Undefined CSS classes** - Used in templates but no CSS definition exists

Run: `go run ./scripts/check-css-unused` or via `./scripts/check.sh`

### How it works

1. Scans `app.css` and all `.svelte` files in `apps/desktop/src/`
2. Extracts CSS class and variable definitions from `<style>` sections
3. Extracts class usages from templates (outside `<script>` and `<style>`)
4. Reports mismatches

### Allowlist

Some classes/variables are used dynamically and can't be detected statically. Add them to `scripts/check-css-unused/allowlist.go`:

```go
var allowedUnusedClasses = map[string]bool{
    "size-bytes": true, // Applied dynamically via triad.tierClass
}

var allowedUnusedVariables = map[string]bool{
    // Add variables here with a comment explaining why
}

var allowedUndefinedClasses = map[string]bool{
    // Classes used for JS selection or third-party libs
}
```

### Limitations

- Only scans the desktop app (not website - it uses Tailwind)
- Doesn't detect class usages in TypeScript files (too many false positives)
- Dynamic class bindings like `class={someVar}` aren't detected (use allowlist)

### File structure

```
scripts/check-css-unused/
├── main.go        # CLI entry point
├── parser.go      # CSS/Svelte parsing with regex
├── scanner.go     # File walking and content extraction
├── reporter.go    # Output formatting
├── allowlist.go   # Allowlist definitions
├── utils.go       # Helper functions
└── go.mod
```
