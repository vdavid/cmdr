package checks

import (
	"fmt"
	"go/ast"
	"go/parser"
	"go/token"
	"os"
	"path/filepath"
	"sort"
	"strings"
)

// Which of the runner's own files can change a given check's verdict?
//
// `GlobalInputs` answers the part that's true for everyone (the runner core: the
// executor, the cache, the registry, the shared context). This file answers the
// rest, per check: the implementation files that check's `Run` function can
// actually reach. Editing `lock-poison.go` cannot change what `cargo nextest`
// reports, and before this existed it re-ran the whole ~250 s Rust battery anyway.
//
// The whole runner is ONE Go package, so package-level analysis buys nothing; the
// attribution has to be file-level. It works on the AST alone, with two rules:
//
//   - A declaration reaches every package-level NAME its body or initializer
//     mentions as an identifier. Selector names (`x.Foo`) are deliberately not
//     resolved by name: half the package has a `String` method, and matching on
//     the method name alone made every check reach every file.
//   - Reaching a TYPE reaches every method declared on it, wherever those live.
//     That's what covers method calls, method values, and interface dispatch
//     without any type information: a value can only exist if something in the
//     closure names its type (a constructor's signature, a composite literal, a
//     field), and once the type is in, its behavior travels with it.
//
// An `init()` is attributed to the package-level variables it assigns, so a check
// that reads one of those variables reaches the init's file too. `initAssignedNames`
// refuses any other init, because an init that registers itself into somebody
// else's table would be invisible here.
//
// ❗ Fail closed. Anything the analysis can't answer (a parse error, a registry it
// can't read, a check whose `Run` it can't resolve, an init it can't attribute)
// makes the WHOLE index fall back to `scripts/check/**` for every check, which is
// exactly what every check carried before this existed. A too-wide input set
// costs cache speed; a too-narrow one reports a green describing code it never
// ran.

// runnerChecksDirParts is where the check implementations live, as path segments
// (joined for the filesystem, slash-joined for the repo-relative input globs).
var runnerChecksDirParts = []string{"scripts", "check", "checks"}

// runnerTreeFallback is what a check fingerprints when the analysis can't answer
// for it: the entire runner tree, the conservative pre-attribution behavior.
var runnerTreeFallback = []string{"scripts/check/**"}

// RunnerSourceIndex maps each check to the runner implementation files it can
// reach. Build it with `LoadRunnerSources`; ask it with `For`.
type RunnerSourceIndex struct {
	byCheck map[string][]string
	// Err is why the analysis gave up, if it did. Non-nil means every lookup
	// answers with the whole runner tree.
	Err error
}

// For returns the runner source globs check id must fingerprint. An unknown id
// (a synthetic definition like the E2E build's, or a registered check the
// registry parse didn't reach) gets the whole runner tree.
// `TestEveryCheckGetsItsOwnRunnerSources` proves no registered check lands there.
func (idx *RunnerSourceIndex) For(id string) []string {
	if idx == nil || idx.Err != nil {
		return runnerTreeFallback
	}
	if files, ok := idx.byCheck[id]; ok {
		return files
	}
	return runnerTreeFallback
}

// LoadRunnerSources analyzes the runner's own sources at rootDir. It never
// returns nil and never fails the caller: a failed analysis is recorded on the
// index and answers every lookup with the whole runner tree.
func LoadRunnerSources(rootDir string) *RunnerSourceIndex {
	byCheck, err := analyzeRunnerSources(filepath.Join(rootDir, filepath.Join(runnerChecksDirParts...)))
	return &RunnerSourceIndex{byCheck: byCheck, Err: err}
}

// runnerDecls is the file-level reference graph of the checks package.
type runnerDecls struct {
	// files maps a package-level name to the files that declare it. A method is
	// keyed as "Type.Method"; an `init()` is folded into the names it assigns.
	files map[string]map[string]bool
	// nodes maps a package-level name to the AST nodes declaring it.
	nodes map[string][]ast.Node
	// methods maps a type name to the keys of the methods declared on it.
	methods map[string][]string
	// imports holds the import names in scope, so `strings.Split` is read as one
	// reference to the `strings` package rather than to a local `Split`.
	imports map[string]bool
	// refs memoizes the names a declaration node mentions.
	refs map[ast.Node]map[string]bool
}

// analyzeRunnerSources returns check ID → the repo-relative runner files that
// check reaches, or an error if anything about the tree defeats the analysis.
func analyzeRunnerSources(checksDir string) (map[string][]string, error) {
	parsed, err := parseRunnerFiles(checksDir)
	if err != nil {
		return nil, err
	}

	decls, err := collectRunnerDecls(parsed)
	if err != nil {
		return nil, err
	}

	roots, err := collectCheckRunRoots(parsed)
	if err != nil {
		return nil, err
	}

	out := make(map[string][]string, len(roots))
	for id, names := range roots {
		files := decls.closure(names)
		if len(files) == 0 {
			return nil, fmt.Errorf("check %q reaches no runner file at all; the analysis is broken, not the tree", id)
		}
		out[id] = files
	}
	return out, nil
}

// parseRunnerFiles parses every non-test source of the checks package, keyed by
// file base name. Build constraints are ignored on purpose: a file this platform
// wouldn't compile still counts as an input, which is the conservative direction.
func parseRunnerFiles(checksDir string) (map[string]*ast.File, error) {
	entries, err := os.ReadDir(checksDir)
	if err != nil {
		return nil, fmt.Errorf("reading %s: %w", checksDir, err)
	}
	fset := token.NewFileSet()
	out := map[string]*ast.File{}
	for _, entry := range entries {
		name := entry.Name()
		if entry.IsDir() || !strings.HasSuffix(name, ".go") || strings.HasSuffix(name, "_test.go") {
			continue
		}
		file, err := parser.ParseFile(fset, filepath.Join(checksDir, name), nil, 0)
		if err != nil {
			return nil, fmt.Errorf("parsing %s: %w", name, err)
		}
		if file.Name.Name != "checks" {
			return nil, fmt.Errorf("%s declares package %s, not `checks`", name, file.Name.Name)
		}
		out[name] = file
	}
	if len(out) == 0 {
		return nil, fmt.Errorf("no runner sources under %s", checksDir)
	}
	return out, nil
}

// collectRunnerDecls indexes every package-level declaration by name and file.
func collectRunnerDecls(parsed map[string]*ast.File) (*runnerDecls, error) {
	d := &runnerDecls{
		files:   map[string]map[string]bool{},
		nodes:   map[string][]ast.Node{},
		methods: map[string][]string{},
		imports: map[string]bool{},
		refs:    map[ast.Node]map[string]bool{},
	}
	add := func(name, file string, node ast.Node) {
		if d.files[name] == nil {
			d.files[name] = map[string]bool{}
		}
		d.files[name][file] = true
		d.nodes[name] = append(d.nodes[name], node)
	}

	var inits []*ast.FuncDecl
	initFiles := map[*ast.FuncDecl]string{}
	for base, file := range parsed {
		for _, imp := range file.Imports {
			name := filepath.Base(strings.Trim(imp.Path.Value, `"`))
			if imp.Name != nil {
				name = imp.Name.Name
			}
			d.imports[name] = true
		}
		for _, decl := range file.Decls {
			isInit, err := d.index(decl, base, add)
			if err != nil {
				return nil, err
			}
			if isInit {
				fn := decl.(*ast.FuncDecl)
				inits = append(inits, fn)
				initFiles[fn] = base
			}
		}
	}

	// An init() has no name to reach, so attribute it to what it assigns: reaching
	// that variable now reaches the init's file and everything the init mentions.
	for _, decl := range inits {
		assigned, err := initAssignedNames(decl, initFiles[decl])
		if err != nil {
			return nil, err
		}
		for _, name := range assigned {
			if d.files[name] == nil {
				return nil, fmt.Errorf("%s: init() assigns %s, which isn't declared at package level", initFiles[decl], name)
			}
			add(name, initFiles[decl], decl)
		}
	}
	return d, nil
}

// index records one package-level declaration, and reports whether it was an
// `init()` (which has no name to record and is attributed separately).
func (d *runnerDecls) index(decl ast.Decl, file string, add func(name, file string, node ast.Node)) (bool, error) {
	switch decl := decl.(type) {
	case *ast.FuncDecl:
		switch {
		case decl.Recv != nil:
			recv := receiverTypeName(decl)
			if recv == "" {
				return false, fmt.Errorf("%s: can't read the receiver type of %s", file, decl.Name.Name)
			}
			key := recv + "." + decl.Name.Name
			add(key, file, decl)
			d.methods[recv] = append(d.methods[recv], key)
		case decl.Name.Name == "init":
			return true, nil
		default:
			add(decl.Name.Name, file, decl)
		}
	case *ast.GenDecl:
		for _, spec := range decl.Specs {
			switch spec := spec.(type) {
			case *ast.ValueSpec:
				for _, name := range spec.Names {
					add(name.Name, file, spec)
				}
			case *ast.TypeSpec:
				add(spec.Name.Name, file, spec)
			}
		}
	}
	return false, nil
}

// initAssignedNames returns the package-level variables an `init()` assigns, and
// refuses an init that does anything else. An init that called a registration
// function instead would change a check's behavior with nothing naming it, which
// is precisely the shape file-level analysis can't see.
func initAssignedNames(decl *ast.FuncDecl, file string) ([]string, error) {
	var names []string
	for _, stmt := range decl.Body.List {
		assign, ok := stmt.(*ast.AssignStmt)
		if !ok || assign.Tok != token.ASSIGN {
			return nil, fmt.Errorf("%s: init() does something other than assign to a package variable; "+
				"the input-fingerprint analysis can't attribute that (see runner-sources.go)", file)
		}
		for _, lhs := range assign.Lhs {
			ident, ok := lhs.(*ast.Ident)
			if !ok {
				return nil, fmt.Errorf("%s: init() assigns to something that isn't a plain package variable", file)
			}
			names = append(names, ident.Name)
		}
	}
	return names, nil
}

// receiverTypeName unwraps a method receiver down to its type name.
func receiverTypeName(decl *ast.FuncDecl) string {
	if len(decl.Recv.List) == 0 {
		return ""
	}
	expr := decl.Recv.List[0].Type
	for {
		switch typed := expr.(type) {
		case *ast.StarExpr:
			expr = typed.X
		case *ast.IndexExpr: // generic receiver, `Foo[T]`
			expr = typed.X
		case *ast.IndexListExpr:
			expr = typed.X
		case *ast.Ident:
			return typed.Name
		default:
			return ""
		}
	}
}

// referencedNames returns the package-level names a declaration mentions.
// Selector names are skipped (see the file comment); a composite literal's field
// keys are skipped too, because `Inputs:` names a field, not the package-level
// `inputs` helper. Map keys, which are values rather than field names, are kept.
func (d *runnerDecls) referencedNames(node ast.Node) map[string]bool {
	if cached, ok := d.refs[node]; ok {
		return cached
	}
	out := map[string]bool{}
	var walk func(ast.Node)
	walk = func(root ast.Node) {
		ast.Inspect(root, func(n ast.Node) bool {
			switch n := n.(type) {
			case *ast.SelectorExpr:
				if ident, ok := n.X.(*ast.Ident); ok && d.imports[ident.Name] {
					return false // `pkg.Symbol`: another package's, not ours
				}
				walk(n.X)
				return false
			case *ast.KeyValueExpr:
				if ident, ok := n.Key.(*ast.Ident); ok && d.files[ident.Name] == nil {
					// A field name that names nothing at package level: skip it.
					// A key that DOES name something (an enum constant used as a map
					// key) stays, on the conservative side of the ambiguity.
					walk(n.Value)
					return false
				}
			case *ast.Ident:
				out[n.Name] = true
			}
			return true
		})
	}
	walk(node)
	d.refs[node] = out
	return out
}

// closure walks the reference graph from the given root names and returns the
// repo-relative paths of every runner file it reaches, sorted.
func (d *runnerDecls) closure(roots []string) []string {
	files := map[string]bool{}
	seen := map[string]bool{}
	queue := append([]string(nil), roots...)
	for len(queue) > 0 {
		name := queue[0]
		queue = queue[1:]
		if seen[name] {
			continue
		}
		seen[name] = true
		for file := range d.files[name] {
			files[file] = true
		}
		queue = append(queue, d.methods[name]...) // a type carries its behavior
		for _, node := range d.nodes[name] {
			for ref := range d.referencedNames(node) {
				if !seen[ref] {
					queue = append(queue, ref)
				}
			}
		}
	}
	prefix := strings.Join(runnerChecksDirParts, "/")
	out := make([]string, 0, len(files))
	for file := range files {
		out = append(out, prefix+"/"+file)
	}
	sort.Strings(out)
	return out
}

// collectCheckRunRoots reads the `AllChecks` literal and returns, per check ID,
// the identifiers its `Run` field names. It's the AST's view of the registry;
// `TestRunnerSourcesAgreeWithTheLinkedBinary` proves it matches the real one.
func collectCheckRunRoots(parsed map[string]*ast.File) (map[string][]string, error) {
	var literal *ast.CompositeLit
	for _, file := range parsed {
		ast.Inspect(file, func(n ast.Node) bool {
			spec, ok := n.(*ast.ValueSpec)
			if !ok || len(spec.Names) != 1 || spec.Names[0].Name != "AllChecks" || len(spec.Values) != 1 {
				return true
			}
			if lit, ok := spec.Values[0].(*ast.CompositeLit); ok {
				literal = lit
			}
			return false
		})
	}
	if literal == nil {
		return nil, fmt.Errorf("couldn't find the `AllChecks` literal")
	}

	roots := map[string][]string{}
	for _, element := range literal.Elts {
		entry, ok := element.(*ast.CompositeLit)
		if !ok {
			return nil, fmt.Errorf("an `AllChecks` element isn't a check literal")
		}
		id, names, err := checkEntryRunRoots(entry)
		if err != nil {
			return nil, err
		}
		roots[id] = names
	}
	return roots, nil
}

// checkEntryRunRoots reads one check literal's ID and the identifiers its `Run`
// field names.
func checkEntryRunRoots(entry *ast.CompositeLit) (string, []string, error) {
	var id string
	var names []string
	for _, field := range entry.Elts {
		kv, ok := field.(*ast.KeyValueExpr)
		if !ok {
			continue
		}
		key, ok := kv.Key.(*ast.Ident)
		if !ok {
			continue
		}
		switch key.Name {
		case "ID":
			lit, ok := kv.Value.(*ast.BasicLit)
			if !ok || lit.Kind != token.STRING {
				return "", nil, fmt.Errorf("a check's ID isn't a string literal")
			}
			id = strings.Trim(lit.Value, `"`)
		case "Run":
			ast.Inspect(kv.Value, func(n ast.Node) bool {
				if ident, ok := n.(*ast.Ident); ok {
					names = append(names, ident.Name)
				}
				return true
			})
		}
	}
	if id == "" {
		return "", nil, fmt.Errorf("an `AllChecks` element has no ID")
	}
	if len(names) == 0 {
		return "", nil, fmt.Errorf("check %q names no Run function the analysis can follow", id)
	}
	return id, names, nil
}
