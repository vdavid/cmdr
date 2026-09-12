package checks

import "testing"

func TestFormatThousands(t *testing.T) {
	tests := []struct {
		n    int
		want string
	}{
		{0, "0"},
		{800, "800"},
		{999, "999"},
		{1000, "1,000"},
		{1200, "1,200"},
		{1000000, "1,000,000"},
		{-1234, "-1,234"},
	}
	for _, tt := range tests {
		if got := formatThousands(tt.n); got != tt.want {
			t.Errorf("formatThousands(%d) = %q, want %q", tt.n, got, tt.want)
		}
	}
}
