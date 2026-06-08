# Dependency rules

- ❌ Never add a dependency without checking license compatibility (`cargo deny check`) and verifying the latest version
  from npm / crates.io / GitHub. Don't trust training data for versions (see the `use-latest-dep-versions` user rule).
  Renovate handles routine updates.
- After bumping npm deps, run `pnpm dedupe`. Without it, nested transitive deps stay pinned to old versions and cause
  false-positive failures (stylelint/postcss misparsing Svelte inline styles, Playwright version skew between AxeBuilder
  and the e2e specs).
