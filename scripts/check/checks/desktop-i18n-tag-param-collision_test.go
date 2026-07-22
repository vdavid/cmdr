package checks

import (
	"strings"
	"testing"
)

func TestCollidingNamesFindsTagsThatShadowParams(t *testing.T) {
	cases := []struct {
		name    string
		message string
		want    []string
	}{
		{
			name:    "the real bug: same name as both tag and param",
			message: "The device is in use by <process>{process}</process>.",
			want:    []string{"process"},
		},
		{
			name:    "distinct names are fine, which is the fix shape",
			message: "The device is in use by <processName>{process}</processName>.",
			want:    nil,
		},
		{
			name:    "a tag with no same-named param is fine",
			message: "Read the <link>docs</link> for {count} more.",
			want:    nil,
		},
		{
			name:    "the collision still counts when the param sits outside its tag",
			message: "{process} is holding it, see <process>here</process>.",
			want:    []string{"process"},
		},
		{
			name:    "two collisions in one message are both reported",
			message: "<a>{a}</a> and <b>{b}</b>",
			want:    []string{"a", "b"},
		},
		{
			name:    "a self-closing tag collides too",
			message: "Restart <icon/> to free {icon}.",
			want:    []string{"icon"},
		},
		{
			name:    "no tags at all",
			message: "Moved {count} files.",
			want:    nil,
		},
		{
			name:    "ICU plural braces are not params",
			message: "<b>{count, plural, one {# file} other {# files}}</b>",
			want:    nil,
		},
	}

	for _, tc := range cases {
		t.Run(tc.name, func(t *testing.T) {
			got := collidingNames(tc.message)
			if strings.Join(got, ",") != strings.Join(tc.want, ",") {
				t.Errorf("collidingNames(%q) = %v, want %v", tc.message, got, tc.want)
			}
		})
	}
}

// The ICU `plural` / `select` argument name is a param name like any other, so a
// tag sharing it collides just the same.
func TestCollidingNamesCatchesAPluralArgumentName(t *testing.T) {
	got := collidingNames("<count>{count, plural, one {# file} other {# files}}</count>")
	if len(got) != 1 || got[0] != "count" {
		t.Errorf("expected the plural argument name to collide, got %v", got)
	}
}
