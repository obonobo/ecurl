package ecurl

import (
	"bytes"
	"fmt"
	"io"
	"strings"
	"testing"
)

// Tests BufferedScanner.Read() with various inputs
func TestBufferedScannerRead(t *testing.T) {
	// Some basic buffer sizes to use for testing
	bufsizes := func() []int {
		return []int{
			1, 1 << 1, 1 << 2,
			1 << 4, 1 << 6, 1 << 10,
			1 << 20, 1 << 27,
		}
	}

	for _, tc := range []struct {
		name     string
		input    string
		bufsizes []int
	}{
		{
			name:     "empty",
			input:    "",
			bufsizes: bufsizes(),
		},
		{
			name:     "hello world",
			input:    "Hello world!",
			bufsizes: bufsizes(),
		},
		{
			name:     "lorem ipsum",
			bufsizes: bufsizes(),
			input: `
			Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
			incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
			nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
			Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
			fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
			culpa qui officia deserunt mollit anim id est laborum.
			`,
		},
		{
			name:     "big",
			input:    strings.Repeat("big!\n", 1000),
			bufsizes: bufsizes(),
		},

		{
			name:     "very big",
			input:    strings.Repeat("very big!\n", 1<<10),
			bufsizes: bufsizes(),
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			for _, size := range tc.bufsizes {
				size := size
				t.Run(fmt.Sprintf("size=%v", size), func(t *testing.T) {
					t.Parallel()
					r := bytes.NewBufferString(tc.input)
					scnr := &BufferedScanner{reader: r, buf: buffer{make([]byte, 0, size), 0}}
					red, err := io.ReadAll(scnr)
					if err != nil {
						t.Fatalf("Expected scanner not to return an error but got: %v", err)
					}
					if actual, expected := string(red), tc.input; actual != expected {
						t.Fatalf("Expected scanner to return '%v' but got '%v'", expected, actual)
					}
				})
			}
		})
	}
}
