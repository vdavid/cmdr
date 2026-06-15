# Homebrew cask

`cmdr.rb` is the source of truth for the cask's **shape** (`url`, `livecheck`, `depends_on`, `app`, `zap`). The live
channel is the personal tap [`vdavid/homebrew-tap`](https://github.com/vdavid/homebrew-tap), whose `Casks/cmdr.rb`
carries this exact shape; release CI (`release.yml`'s `bump-tap` job) rewrites only its `version` + `sha256` per
release. So make shape edits here; don't hand-bump `version` / `sha256` (CI does it). The tap stays canonical until
`vdavid/cmdr` clears Homebrew's notability bar, at which point a resubmission to `Homebrew/homebrew-cask` becomes the
canonical home and the tap retires.

Before touching `cmdr.rb`, read [`docs/guides/homebrew-cask.md`](../../../../docs/guides/homebrew-cask.md): it explains
the non-obvious constraints (why the `url` must not be a `github.com` one, how the tap bump works, how to test in a
throwaway tap) and the resubmission process.

Full details: [DETAILS.md](DETAILS.md).
