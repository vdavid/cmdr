# Generating test files

Generate test data for stress-testing the file explorer with large directories.

## Usage

Do `go run scripts/test-data-generator/main.go`.

## What it does

Creates (or syncs to) four folders under `_ignored/test-data/`:

| Folder                    | Target file count |
| ------------------------- | ----------------- |
| `folder with 1000 files`  | 1,000             |
| `folder with 5000 files`  | 5,000             |
| `folder with 20000 files` | 20,000            |
| `folder with 50000 files` | 50,000            |

No magic.

Each file:

- Named with a random timestamp: `{YYYY}-{MM}-{DD} {hh}-{mm}-{ss}.md`
- Contains one random sentence between 60 and 100 chars or so.

## Sync behavior

- If a folder has **fewer files** than the target → creates new ones
- If a folder has **more files** than the target → deletes some random ones
- If a folder is at target → noop

Progress dots are printed every 5,000 files.

## Time estimate

Creating all 76,000 files from scratch takes ~15–30 seconds depending on disk/SSD speed.
