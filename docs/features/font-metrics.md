# Font metrics for accurate column widths

## Overview

The file explorer uses a font metrics system to calculate accurate character widths for optimal column sizing in Brief
mode. This ensures filenames are never truncated unnecessarily while avoiding excessive column widths.

## How it works

### Measurement phase (first app run)

1. **Frontend measurement**: On first app start, the system measures ~67,000 characters using the Canvas API
    - Coverage: Basic Multilingual Plane (BMP) + common emoji
    - Includes CJK, Cyrillic, Arabic, Indic scripts, Latin Extended
    - Takes ~100-300ms, runs in background using `requestIdleCallback`

2. **Binary storage**: Measurements are serialized using bincode2 and saved to disk
    - Location: `~/Library/Application Support/com.veszelovszki.cmdr/font-metrics/system-400-12.bin`
    - Size: ~426KB (500KB theoretical max)
    - Load time: ~5ms

### Width calculation (every directory load)

1. **Rust calculates max width**: During `list_directory_start()`, Rust:
    - Iterates through all filenames
    - Sums character widths using cached metrics
    - Returns the maximum width alongside `listingId` and `totalCount`

2. **Frontend uses width**: BriefList receives `maxFilenameWidth` and:
    - Uses it for column width when available
    - Falls back to estimation (`containerWidth / 3`) if metrics unavailable
    - Automatically adapts columns to actual content

## Performance

- **Measurement**: ~100-300ms (one-time, on first run)
- **Load from disk**: ~5ms (on subsequent runs)
- **Width calculation**: ~1-10ms per directory (depends on file count)
- **Total impact**: Negligible for directories under 100k files

## Code organization

```
src-tauri/src/
  └── font_metrics/
      └── mod.rs              # Core metrics storage and calculation

src/lib/
  └── font-metrics/
      ├── index.ts            # Public API
      └── measure.ts          # Canvas measurement
```

## Font configuration

Currently hardcoded to match CSS:

- **Font family**: system font stack (`-apple-system, BlinkMacSystemFont, 'Segoe UI', system-ui, sans-serif`)
- **Font weight**: 400 (normal)
- **Font size**: 12px (`--font-size-sm`)
- **Font ID**: `system-400-12`

When font settings become user-configurable, the frontend will automatically re-measure and the cache key will be
updated.

## Example

For a directory with files:

- `README.md` (9 chars × 7.2px avg = 65px)
- `package.json` (12 chars × 7.2px avg = 86px)
- `中文文件.txt` (7 chars × 12px avg = 84px)

The system calculates the max width as 86px, ensuring all filenames fit without truncation while keeping columns
compact.

## Limitations

- Only supports a single font configuration at a time
- Unmeasured characters (rare Unicode) fall back to average width
- Column width is fixed for the entire directory (not per-column)
