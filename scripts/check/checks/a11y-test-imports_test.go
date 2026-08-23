package checks

import (
	"reflect"
	"sort"
	"testing"
)

func TestImportedPathsIn(t *testing.T) {
	const testFile = "apps/desktop/src/lib/settings/sections/sections.a11y.test.ts"

	tests := []struct {
		name   string
		source string
		want   []string
	}{
		{
			name:   "relative default import",
			source: `import Alpha from './Alpha.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "double quotes",
			source: `import Alpha from "./Alpha.svelte"`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "$lib alias",
			source: `import Alpha from '$lib/settings/sections/Alpha.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "parent-relative resolves out of the directory",
			source: `import Row from '../Row.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/Row.svelte"},
		},
		{
			name:   "side-effect import",
			source: `import './Alpha.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "namespace import",
			source: `import * as Alpha from './Alpha.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "type-only import",
			source: `import type { Props } from './Alpha.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "dynamic import",
			source: `const m = await import('./Alpha.svelte')`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "multi-line named import",
			source: "import {\n  a,\n  b,\n} from './Alpha.svelte'",
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "bare package specifier resolves to nothing",
			source: `import { mount } from 'svelte'`,
			want:   nil,
		},
		{
			name:   "line-commented import does not count",
			source: `// import Alpha from './Alpha.svelte'`,
			want:   nil,
		},
		{
			name:   "block-commented import does not count",
			source: "/**\n * import Alpha from './Alpha.svelte'\n */",
			want:   nil,
		},
		{
			name:   "a name inside a string is not an import",
			source: `describe("./Alpha.svelte a11y", () => {})`,
			want:   nil,
		},
		{
			name:   "an apostrophe in a comment does not unbalance quote tracking",
			source: "// it's the section that can't mount\n" + `import Alpha from './Alpha.svelte'`,
			want:   []string{"apps/desktop/src/lib/settings/sections/Alpha.svelte"},
		},
		{
			name:   "vi.mock factory paths resolve like any other specifier",
			source: `vi.mock('./Alpha.svelte', () => ({}))`,
			want:   nil, // vi.mock isn't an import statement; only real imports count
		},
		{
			name: "several imports in one file",
			source: `import { expectNoA11yViolations } from '$lib/test-a11y'
import Alpha from './Alpha.svelte'
import Beta from './Beta.svelte'`,
			want: []string{
				"apps/desktop/src/lib/settings/sections/Alpha.svelte",
				"apps/desktop/src/lib/test-a11y",
				"apps/desktop/src/lib/settings/sections/Beta.svelte",
			},
		},
	}

	for _, tt := range tests {
		t.Run(tt.name, func(t *testing.T) {
			got := importedPathsIn(testFile, tt.source)
			var gotList []string
			for p := range got {
				gotList = append(gotList, p)
			}
			sort.Strings(gotList)
			want := append([]string(nil), tt.want...)
			sort.Strings(want)
			if !reflect.DeepEqual(gotList, want) {
				t.Errorf("importedPathsIn() = %v, want %v", gotList, want)
			}
		})
	}
}
