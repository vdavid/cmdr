package checks

import (
	"path/filepath"
	"testing"
)

// These tests cover the PARSER only, never the real `vite.config.js`. The
// registered `desktop-vite-build-target` check is what guards the real config:
// it fingerprints that file, so it re-runs on the edit it exists to catch, and
// CI runs it on every push. A Go test doing the same would drag
// `apps/desktop/vite.config.js` into `goTestsInputs` (see
// `realTreeReadingTests`), costing every Go lint a cache miss to duplicate a
// guard that already runs.

func TestViteBuildTargetReadsAStringPin(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig({
  build: {
    target: 'safari15',
    chunkSizeWarningLimit: 1000,
  },
})`)
	if want := []string{"safari15"}; len(pin.targets) != 1 || pin.targets[0] != want[0] {
		t.Fatalf("targets = %v, want %v", pin.targets, want)
	}
	if pin.safariMajor != 15 {
		t.Fatalf("safariMajor = %d, want 15", pin.safariMajor)
	}
}

// A multi-target array is legal Vite config, and only its safari entry says
// anything about the WebKit the macOS build has to parse.
func TestViteBuildTargetReadsAnArrayPin(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig({
  build: { target: ["chrome110", 'safari15'] },
})`)
	if len(pin.targets) != 2 {
		t.Fatalf("targets = %v, want two entries", pin.targets)
	}
	if pin.safariMajor != 15 {
		t.Fatalf("safariMajor = %d, want 15", pin.safariMajor)
	}
}

// The bug this check exists for: no `target` at all, so Vite's own moving
// "widely available" baseline decides the floor.
func TestViteBuildTargetReportsAMissingPin(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig({
  build: { chunkSizeWarningLimit: 1000 },
})`)
	if pin.found {
		t.Fatalf("an absent `target` must not read as pinned, got %v", pin.targets)
	}
	if msg := viteBuildTargetViolation(pin); msg == "" {
		t.Fatal("an absent `target` must be reported")
	}
}

// `esnext` parses fine and pins nothing about WebKit, which is the shape that
// reads as "we thought about it" while leaving the floor wide open.
func TestViteBuildTargetRejectsANonSafariPin(t *testing.T) {
	for _, target := range []string{"esnext", "es2022", "baseline-widely-available"} {
		pin := parseViteConfigForTest(t, "export default defineConfig({ build: { target: '"+target+"' } })")
		if !pin.found {
			t.Fatalf("%q should still parse as a pin", target)
		}
		if pin.safariMajor != 0 {
			t.Fatalf("%q must not count as a safari pin", target)
		}
		if msg := viteBuildTargetViolation(pin); msg == "" {
			t.Fatalf("%q pins no WebKit floor and must be reported", target)
		}
	}
}

// A `target` the parser can't resolve to literals is not a pin it can vouch for.
func TestViteBuildTargetRejectsAComputedPin(t *testing.T) {
	pin := parseViteConfigForTest(t, `const t = 'safari15'
export default defineConfig({ build: { target: t } })`)
	if msg := viteBuildTargetViolation(pin); msg == "" {
		t.Fatal("a target the check can't read must be reported, not assumed good")
	}
}

// This repo comments heavily, and the comment explaining the pin names
// `target` and carries braces. A parser that reads prose as config would either
// pass on a deleted pin or trip over a brace in a sentence.
func TestViteBuildTargetIgnoresCommentsAndStrings(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig({
  // Leaving build: { target: 'safari15' } unset lets Vite decide.
  /* another one: build: { target: 'safari15' } */
  define: { __NOTE__: JSON.stringify("build: { target: 'safari15' }") },
  build: { chunkSizeWarningLimit: 1000 },
})`)
	if pin.found {
		t.Fatalf("prose and string literals are not a pin, got %v", pin.targets)
	}
}

// `target` under `server` or `optimizeDeps` is a different setting entirely and
// says nothing about what the shipped bundle is transpiled to.
func TestViteBuildTargetIgnoresATargetOutsideTheBuildBlock(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig({
  optimizeDeps: { esbuildOptions: { target: 'safari15' } },
  build: { chunkSizeWarningLimit: 1000 },
})`)
	if pin.found {
		t.Fatalf("only `build.target` counts, got %v", pin.targets)
	}
}

// A nested object inside `build` must not hide the block's closing brace.
func TestViteBuildTargetSeesPastNestedObjects(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig({
  build: {
    rollupOptions: { checks: { pluginTimings: false } },
    target: 'safari15',
  },
})`)
	if pin.safariMajor != 15 {
		t.Fatalf("safariMajor = %d, want 15", pin.safariMajor)
	}
}

// A `build` block reached through the async-factory form Vite also accepts.
func TestViteBuildTargetHandlesTheFactoryForm(t *testing.T) {
	pin := parseViteConfigForTest(t, `export default defineConfig(async () => ({
  plugins: [],
  build: {
    target: 'safari15',
  },
}))`)
	if pin.safariMajor != 15 {
		t.Fatalf("safariMajor = %d, want 15", pin.safariMajor)
	}
}

func parseViteConfigForTest(t *testing.T, source string) viteTargetPin {
	t.Helper()
	dir := t.TempDir()
	writeFiles(t, dir, map[string]string{"vite.config.js": source})
	pin, err := readViteBuildTarget(filepath.Join(dir, "vite.config.js"))
	if err != nil {
		t.Fatalf("readViteBuildTarget: %v", err)
	}
	return pin
}
