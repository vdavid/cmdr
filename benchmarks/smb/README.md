# SMB benchmark

Compares native (OS-mounted SMB via `std::fs`) vs direct (`smb` crate) file operations. Measures upload, list, download,
and delete across configurable file sizes, counts, and NAS targets.

## Setup

1. Copy `config.example.toml` to `config.toml` and fill in your NAS details (credentials, share name, IP). `config.toml`
   is gitignored.
2. Mount each share in Finder (`smb://<ip>/`) so the native path (for example `/Volumes/naspi`) exists.

## Running

```sh
cargo run -p smb-benchmark --release
```

- `--cleanup-only` removes leftover test directories on all targets without running benchmarks.
- Set `RUST_LOG=debug` for verbose SMB protocol logs.

Release mode matters: without it, local file I/O and data generation are noticeably slower.

## Interpreting results

The benchmark prints a table with median times per operation and a "speedup" ratio (native / direct). Values > 1.0 mean
direct is faster.

Each suite runs multiple iterations (after a warmup run). The order of native vs direct is randomized per iteration to
reduce cache bias. Each iteration uses a unique directory name to avoid stale SMB cache reads.

JSON results are saved to `results/` with a timestamp.

## `sspi` version pin

`sspi` 0.18.9 breaks NTLM auth when the `kerberos` feature isn't enabled. We pin `sspi = "=0.18.7"` until that's fixed
upstream.
