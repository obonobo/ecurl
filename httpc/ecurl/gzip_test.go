package ecurl

import (
	"bytes"
	"io"
	"strings"
	"testing"

	"github.com/obonobo/ecurl/internal/testutils"
)

// Tests decoding a basic gzip encoded string
func TestDecodeBasic(t *testing.T) {
	for _, tc := range []struct {
		name  string
		input string
	}{
		{
			name:  "empty",
			input: "",
		},
		{
			name:  "hello world",
			input: "Hello World!",
		},
		{
			name:  "asd123",
			input: "asd123",
		},
		{
			name:  "big!",
			input: strings.Repeat("big!\r\n\t", 1024),
		},
	} {
		tc := tc
		t.Run(tc.name, func(t *testing.T) {
			t.Parallel()
			zipped, err := testutils.Gzipup(tc.input)
			if err != nil {
				t.Fatalf("Failed to gzip string '%v': %v", tc.input, err)
			}
			gzipper, err := NewGzipper(io.NopCloser(bytes.NewBufferString(zipped)))
			if err != nil {
				t.Fatalf("Failed to create gzipper: %v", err)
			}
			red, err := io.ReadAll(gzipper)
			if err != nil {
				t.Fatalf("Failed to read from gzipper: %v", err)
			}
			if expected, actual := tc.input, string(red); expected != actual {
				t.Fatalf("Expected '%v' but got '%v'", expected, actual)
			}
		})
	}
}
