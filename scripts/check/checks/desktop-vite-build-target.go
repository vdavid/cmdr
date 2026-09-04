package checks

import (
	"fmt"
	"os"
	"regexp"
	"strconv"
	"strings"
)

// viteConfigRelPath is the desktop frontend's Vite config, the one place that
// decides which JS and CSS syntax the shipped bundle is transpiled down to.
const viteConfigRelPath = "apps/desktop/vite.config.js"

// The desktop bundle's browser floor must be PINNED, and pinned to a Safari
// version, because Cmdr's WebKit is the system Safari.
//
// Vite's default `build.target` is `ESBUILD_BASELINE_WIDELY_AVAILABLE_TARGET`,
// a moving baseline that tracks what browsers have "widely available" (Safari
// 16.4 as of Vite 8). An unset target therefore hands the floor to whatever
// Vite's current major thinks is modern, which drifts UP on a routine dep bump
// while `tauri.conf.json`'s `minimumSystemVersion` stays where it is. The gap
// is silent: nothing warns, the build is green, and the app white-screens on
// the first syntax the old WebKit can't parse.
//
// That gap was real, not hypothetical. With the target unset, the bundle
// carried 286 raw `oklch()` colors (Safari 15.4+), so every design token in it
// was unset on the Safari 15.0 that macOS 12 Monterey ships, the OS the plist
// already promised to support. Pinning `safari15` makes esbuild lower them to
// `lab()`, which Safari 15.0 parses.
//
// Non-safari targets (`esnext`, `es2022`, a baseline alias) parse fine and pin
// nothing about WebKit, so they don't count. An array is fine as long as one
// entry names Safari; extra entries only tighten the result.
//
// Deliberately NOT enforced here: an upper bound tied to
// `minimumSystemVersion`. Mapping a macOS version to "the WebKit we must assume"
// is a product call (an untouched Monterey runs Safari 15.0; a fully patched one
// reaches 17.6), and baking one answer in would put this check in a fight with
// the next deliberate floor change. The pin and the plist are moved together by
// hand; `docs/notes/system-requirements-and-es2025.md` records where they stand.
func RunViteBuildTarget(ctx *CheckContext) (CheckResult, error) {
	pin, err := readViteBuildTarget(ctx.RootDir + "/" + viteConfigRelPath)
	if err != nil {
		return CheckResult{}, err
	}
	if msg := viteBuildTargetViolation(pin); msg != "" {
		return CheckResult{}, fmt.Errorf("%s:\n%s", viteConfigRelPath, indentOutput(msg))
	}
	return Success(fmt.Sprintf("`build.target` pinned to %s (safari %d floor)",
		strings.Join(pin.targets, ", "), pin.safariMajor)), nil
}

// viteTargetPin is what the config says about `build.target`.
type viteTargetPin struct {
	// found is true when a `build.target` is present, whatever its value.
	found bool
	// targets are the literal strings it names, empty when the value isn't a
	// string or an array of strings the parser can resolve.
	targets []string
	// safariMajor is the major version of the safari entry, 0 when there is none.
	safariMajor int
}

// viteBuildTargetViolation returns the reason this pin doesn't hold the floor,
// or "" when it does.
func viteBuildTargetViolation(pin viteTargetPin) string {
	const fix = "Pin it to the oldest Safari the macOS floor in `apps/desktop/src-tauri/tauri.conf.json` implies,\n" +
		"e.g. `target: 'safari15'` for macOS 12 Monterey."
	switch {
	case !pin.found:
		return "`build.target` is unset, so Vite's own moving \"widely available\" baseline decides which\n" +
			"syntax the shipped bundle keeps. A Vite major bump then raises the browser floor above the\n" +
			"macOS version the app promises to run on, with no warning and no failing build.\n" + fix
	case len(pin.targets) == 0:
		return "`build.target` is set to something this check can't read as string literals, so it can't\n" +
			"vouch for the floor.\n" + fix
	case pin.safariMajor == 0:
		return fmt.Sprintf("`build.target` is %s, which names no Safari version. Cmdr's WebKit IS the system\n"+
			"Safari, so a target that doesn't name one pins nothing about the macOS floor.\n%s",
			strings.Join(pin.targets, ", "), fix)
	}
	return ""
}

// viteBuildBlockRE finds the opening brace of the `build` option. The leading
// class keeps it off a property access (`config.build:` can't occur, but
// `rebuild:` can) without demanding the key start its own line, which the
// one-line config form doesn't.
var viteBuildBlockRE = regexp.MustCompile(`(?:^|[^.\w$])build\s*:\s*\{`)

// viteTargetKeyRE finds the `target` key inside that block.
var viteTargetKeyRE = regexp.MustCompile(`(?:^|[^.\w$])target\s*:\s*`)

// viteTargetLiteralRE pulls one quoted literal out of a target value.
var viteTargetLiteralRE = regexp.MustCompile(`['"]([^'"]*)['"]`)

// viteSafariTargetRE matches an esbuild safari target (`safari15`, `safari15.6`).
var viteSafariTargetRE = regexp.MustCompile(`^safari(\d+)`)

// readViteBuildTarget parses `build.target` out of a Vite config.
//
// Structural parsing rather than a bare grep, because this repo comments
// heavily and the comment explaining the pin quotes the very syntax being
// looked for. Comments are blanked and string literals are masked before any
// brace is counted, so prose can neither fake a pin nor hide one, and a
// `target` under `server` or `optimizeDeps` doesn't answer for `build`.
func readViteBuildTarget(path string) (viteTargetPin, error) {
	data, err := os.ReadFile(path)
	if err != nil {
		return viteTargetPin{}, fmt.Errorf("open %s: %w", path, err)
	}
	source := string(data)
	code := maskJSNonCode(source)

	loc := viteBuildBlockRE.FindStringIndex(code)
	if loc == nil {
		return viteTargetPin{}, nil
	}
	block := jsBlockRange(code, strings.LastIndex(code[loc[0]:loc[1]], "{")+loc[0])
	if block == nil {
		return viteTargetPin{}, nil
	}

	// Only the block's own keys count; a `target` inside a nested object
	// (`rollupOptions`, `terserOptions`) is a different setting.
	inner := code[block[0]:block[1]]
	var offset int
	for {
		key := viteTargetKeyRE.FindStringIndex(inner[offset:])
		if key == nil {
			return viteTargetPin{}, nil
		}
		start := offset + key[0]
		valueAt := offset + key[1]
		if jsBraceDepth(inner[:start]) == 0 {
			return parseViteTargetValue(source[block[0]+valueAt:], code[block[0]+valueAt:]), nil
		}
		offset = valueAt
	}
}

// parseViteTargetValue reads the literals a `target` value names. `raw` is the
// original source from the value onwards, `masked` its comment-blanked and
// string-masked twin, used to find where the value ends.
func parseViteTargetValue(raw, masked string) viteTargetPin {
	pin := viteTargetPin{found: true}

	end := strings.IndexAny(masked, ",\n}")
	if end < 0 {
		end = len(masked)
	}
	// An array value spans past the first comma, so take the whole bracket.
	if open := strings.IndexByte(masked, '['); open >= 0 && open < end {
		close := strings.IndexByte(masked, ']')
		if close < 0 {
			return pin
		}
		end = close
	}

	for _, m := range viteTargetLiteralRE.FindAllStringSubmatch(raw[:end], -1) {
		pin.targets = append(pin.targets, m[1])
		if sm := viteSafariTargetRE.FindStringSubmatch(m[1]); sm != nil {
			major, _ := strconv.Atoi(sm[1])
			// The lowest safari entry is the one that binds.
			if pin.safariMajor == 0 || major < pin.safariMajor {
				pin.safariMajor = major
			}
		}
	}
	return pin
}

// maskJSNonCode returns source with every comment byte and every string-literal
// byte replaced by a space, preserving length so offsets stay usable against the
// original. Quotes themselves survive, so a masked literal still reads as one.
func maskJSNonCode(source string) string {
	out := []byte(source)
	blank := func(from, to int) {
		for i := from; i < to && i < len(out); i++ {
			if out[i] != '\n' {
				out[i] = ' '
			}
		}
	}
	for i := 0; i < len(source); i++ {
		switch {
		case strings.HasPrefix(source[i:], "//"):
			end := strings.IndexByte(source[i:], '\n')
			if end < 0 {
				end = len(source) - i
			}
			blank(i, i+end)
			i += end
		case strings.HasPrefix(source[i:], "/*"):
			end := strings.Index(source[i+2:], "*/")
			if end < 0 {
				end = len(source) - i - 2
			}
			blank(i, i+2+end+2)
			i += 2 + end + 1
		case source[i] == '\'' || source[i] == '"' || source[i] == '`':
			quote := source[i]
			j := i + 1
			for j < len(source) && source[j] != quote {
				if source[j] == '\\' {
					j++
				}
				j++
			}
			blank(i+1, j)
			i = j
		}
	}
	return string(out)
}

// jsBlockRange returns the half-open range just inside the braces of the block
// opening at `open`, or nil when it never closes.
func jsBlockRange(code string, open int) []int {
	depth := 0
	for i := open; i < len(code); i++ {
		switch code[i] {
		case '{':
			depth++
		case '}':
			depth--
			if depth == 0 {
				return []int{open + 1, i}
			}
		}
	}
	return nil
}

// jsBraceDepth counts unclosed braces in a masked span, which is how a key at
// the block's own level is told apart from one inside a nested object.
func jsBraceDepth(code string) int {
	depth := 0
	for i := 0; i < len(code); i++ {
		switch code[i] {
		case '{', '[':
			depth++
		case '}', ']':
			depth--
		}
	}
	return depth
}
