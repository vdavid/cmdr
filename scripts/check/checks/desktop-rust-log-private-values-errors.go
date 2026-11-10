package checks

import (
	"regexp"
	"strings"
)

// A path-carrying error is one whose text holds the user's paths and names: a
// type that writes its log text through `ErrorLogText` (logged with `LogError`),
// and one with a `log_detail()` (`WriteOperationError`, logged as `detail={:?}`).
// Printing one plainly, through `{}` or a derived `{:?}`, hands its
// paths to the report's regex fallback, which ends a prose path at `, ` and so
// shipped most of a file name holding a comma (cmdr-reports#103).
//
// The scanner has no type checker, so it sees a value's type only where the
// source spells it: a field, a parameter, or a `let` annotation. A field access
// (`failure.error`) counts when ANY scanned file declares that field name with a
// path-carrying type; a bare name (`e`) when its own file does. An `Err(e)`
// binding's type is invisible, and so is a struct that only HOLDS one (a derived
// `{:?}` of a whole `WriteFailure`), so this catches the shape, not every
// instance. Indexing the holders too flagged ~80 counters and states that merely
// sit beside an error in some struct.

// errorLogTextImplPattern finds `impl ErrorLogText for X` (generics and a path
// prefix allowed): group 1 is the impl's generics, group 2 the type.
var errorLogTextImplPattern = regexp.MustCompile(`\bimpl\s*(<[^{]*?>)?\s*(?:[A-Za-z_][\w:]*::)?ErrorLogText\s+for\s+&?(?:[a-z_][\w]*::)*([A-Z]\w*)`)

// genericParamPattern reads the parameter names out of an impl's generics.
var genericParamPattern = regexp.MustCompile(`[<,]\s*([A-Z]\w*)`)

// inherentImplPattern finds an inherent `impl X {` block.
var inherentImplPattern = regexp.MustCompile(`\bimpl\s*(?:<[^>{]*>)?\s*([A-Z]\w*)(?:<[^>{]*>)?\s*\{`)

var logDetailFnPattern = regexp.MustCompile(`\bfn\s+log_detail\s*\(`)

// typedNamePattern reads `name: Type` (a field, a parameter, a `let`), up to the
// next delimiter on its line. The type has to start like one, so `a::b` never
// reads as `a`.
var typedNamePattern = regexp.MustCompile(`\b([a-z_][a-z0-9_]*)[ \t]*:[ \t]*([&'A-Za-z_(][^,;)={}\n]*)`)

// typeWrappers hold their argument as-is for Debug (and, for a reference or a
// box, Display), so `Option<VolumeError>` is as path-carrying as `VolumeError`.
var typeWrappers = setOf("Option", "Vec", "Box", "Arc", "Rc", "Result", "VecDeque", "Cow")

// pathErrorIndex is what the scan knows about path-carrying errors across the
// scanned tree.
type pathErrorIndex struct {
	types  map[string]bool // the path-carrying types, by name
	fields map[string]bool // every name any file declares with one of them
}

// indexPathErrors builds the index from every scanned file: the types first
// (`ErrorLogText` impls and `log_detail` holders), then the names declared with
// them.
func indexPathErrors(sources []rustSourceFile) *pathErrorIndex {
	index := &pathErrorIndex{types: map[string]bool{}, fields: map[string]bool{}}
	for _, src := range sources {
		for _, m := range errorLogTextImplPattern.FindAllStringSubmatch(src.masked, -1) {
			// The blanket `impl<T: ErrorLogText> ErrorLogText for &T` names no type.
			generic := false
			for _, p := range genericParamPattern.FindAllStringSubmatch(m[1], -1) {
				generic = generic || p[1] == m[2]
			}
			if !generic {
				index.types[m[2]] = true
			}
		}
		for _, loc := range inherentImplPattern.FindAllStringSubmatchIndex(src.masked, -1) {
			end := matchBrace(src.masked, loc[1]-1)
			if end > 0 && logDetailFnPattern.MatchString(src.masked[loc[1]:end]) {
				index.types[src.masked[loc[2]:loc[3]]] = true
			}
		}
	}
	for _, src := range sources {
		for name := range index.bindingsIn(src.masked) {
			index.fields[name] = true
		}
	}
	return index
}

// bindingsIn returns the names a masked source declares with a path-carrying
// type.
func (x *pathErrorIndex) bindingsIn(masked string) map[string]bool {
	names := map[string]bool{}
	for _, m := range typedNamePattern.FindAllStringSubmatch(masked, -1) {
		if x.types[outerTypeName(m[2])] {
			names[m[1]] = true
		}
	}
	return names
}

// outerTypeName returns the type a declaration's type names once references,
// lifetimes, `mut`, `dyn`/`impl`, and the wrappers (`Option<…>`, `Arc<…>`) are
// peeled off: `&'a Option<WriteOperationError>` → `WriteOperationError`. A path
// names its last segment, so a variant (`VolumeError::NotSupported`) is not its
// enum.
func outerTypeName(ty string) string {
	t := strings.TrimSpace(ty)
	for {
		trimmed := strings.TrimLeft(t, "&( \t\n")
		if strings.HasPrefix(trimmed, "'") {
			end := 1
			for end < len(trimmed) && isRustIdentByte(trimmed[end]) {
				end++
			}
			trimmed = trimmed[end:]
		}
		for _, kw := range []string{"mut ", "dyn ", "impl "} {
			trimmed = strings.TrimPrefix(strings.TrimLeft(trimmed, " \t\n"), kw)
		}
		trimmed = strings.TrimLeft(trimmed, " \t\n")
		end := 0
		for end < len(trimmed) && (isRustIdentByte(trimmed[end]) || trimmed[end] == ':') {
			end++
		}
		path := trimmed[:end]
		name := path[strings.LastIndex(path, ":")+1:]
		rest := strings.TrimLeft(trimmed[end:], " \t\n")
		if typeWrappers[name] && strings.HasPrefix(rest, "<") {
			t = rest[1:]
			continue
		}
		return name
	}
}

// pathCarryingError returns the name an argument ends on when that name holds a
// path-carrying error, or "". A field or method (`failure.error`) is judged by
// the tree's field index, a bare name (`&e`, `*err`) by its own file's bindings.
func (f *logPrivateFile) pathCarryingError(expr string) string {
	if f.errors == nil {
		return ""
	}
	if last := lastChainCall(expr); last != "" {
		// A method's return type is invisible (`report.message()`), so only a
		// field access counts.
		if f.errors.fields[last] && !endsInCall(expr, last) {
			return last
		}
		return ""
	}
	// Only pass-through calls follow it (`e.clone()`), or nothing: judge the root.
	root, _, _ := strings.Cut(expr, ".")
	name := strings.TrimSpace(strings.TrimLeft(strings.TrimSpace(root), "&*"))
	if f.errorBindings[name] {
		return name
	}
	return ""
}

// endsInCall reports whether the chain segment `name` (the last one that isn't a
// pass-through) is a method call: `name` followed by `(` or a turbofish.
func endsInCall(expr, name string) bool {
	at := strings.LastIndex(expr, "."+name)
	if at < 0 {
		return false
	}
	rest := strings.TrimLeft(expr[at+1+len(name):], " \t\n")
	return strings.HasPrefix(rest, "(") || strings.HasPrefix(rest, "::<")
}
