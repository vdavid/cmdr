# Homebrew cask

`cmdr.rb` is the submission template for `Homebrew/homebrew-cask`. Once merged there, that repo is the canonical home
(BrewTestBot auto-bumps it) and this copy goes stale by design.

Before touching `cmdr.rb`, read [`docs/guides/homebrew-cask.md`](../../../../docs/guides/homebrew-cask.md): it explains
the non-obvious constraints (why the `url` must not be a `github.com` one, how to refresh the `sha256`, how to test in a
throwaway tap) and the submission process.
