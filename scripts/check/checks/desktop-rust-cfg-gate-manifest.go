package checks

import (
	"strings"

	"github.com/BurntSushi/toml"
)

// extractMacOSCrateModules parses Cargo.toml and returns the set of Rust module names
// (hyphens converted to underscores) for crates declared under [target.'cfg(target_os = "macos")'.dependencies].
//
// A crate ALSO declared unconditionally (in `[dependencies]` or `[dev-dependencies]`) is
// left out: it links on every target through that second declaration, so naming it
// outside a gate breaks nothing. `tar` is the live example, macOS-only for production
// extraction and an all-target dev-dependency so the tarball-building test compiles in
// the Linux lane. A `cfg(unix)` declaration counts the same way: it reaches Linux, the
// only other target this check guards (`xattr`, a Unix dev-dependency because it doesn't
// build for Windows).
func extractMacOSCrateModules(cargoPath string) (map[string]bool, error) {
	var cargo map[string]any
	if _, err := toml.DecodeFile(cargoPath, &cargo); err != nil {
		return nil, err
	}

	allTargets := allTargetDepNames(cargo)

	// Navigate: target -> cfg(target_os = "macos") -> dependencies
	targetSection, ok := cargo["target"]
	if !ok {
		return nil, nil
	}
	targetMap, ok := targetSection.(map[string]any)
	if !ok {
		return nil, nil
	}

	cfgSection, ok := targetMap[`cfg(target_os = "macos")`]
	if !ok {
		return nil, nil
	}
	cfgMap, ok := cfgSection.(map[string]any)
	if !ok {
		return nil, nil
	}

	depsSection, ok := cfgMap["dependencies"]
	if !ok {
		return nil, nil
	}
	depsMap, ok := depsSection.(map[string]any)
	if !ok {
		return nil, nil
	}

	modules := make(map[string]bool, len(depsMap))
	for crateName := range depsMap {
		if allTargets[crateName] {
			continue
		}
		moduleName := strings.ReplaceAll(crateName, "-", "_")
		modules[moduleName] = true
	}
	return modules, nil
}

// allTargetDepNames collects the crate names a manifest declares for macOS and Linux both:
// the unconditional `[dependencies]` and `[dev-dependencies]` tables, and their
// `cfg(unix)` twins.
func allTargetDepNames(cargo map[string]any) map[string]bool {
	names := map[string]bool{}
	sections := []map[string]any{cargo}
	if targets, ok := cargo["target"].(map[string]any); ok {
		if unix, ok := targets["cfg(unix)"].(map[string]any); ok {
			sections = append(sections, unix)
		}
	}
	for _, section := range sections {
		for _, table := range []string{"dependencies", "dev-dependencies"} {
			deps, ok := section[table].(map[string]any)
			if !ok {
				continue
			}
			for crateName := range deps {
				names[crateName] = true
			}
		}
	}
	return names
}
