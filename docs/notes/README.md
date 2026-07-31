This folder contains notes that are not specs, ADRs, or docs on the system. This is pretty much a catch-all folder for
docs that feel helpful and important for some time, but don't belong anywhere else. Like specs, this folder gets wiped
periodically once we made sure that all important information like intent behind features and processes is captured
somewhere else (code or docs).

One note is load-bearing rather than historical: `index-extraction-baseline.md` holds the before-and-after for the
`cmdr-index` extraction, including the method each number was taken with. Keep it: it's the reference any future "did
the index get slower?" re-measurement compares against, and it records what the crate boundary did and didn't buy.
