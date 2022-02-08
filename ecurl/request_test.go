package ecurl

import (
	"bytes"
	"fmt"
	"net/http"
	"strings"
	"testing"

	"github.com/obonobo/ecurl/internal/testutils"
)

const (
	url  = "http://localhost:8181/"
	addr = ":8181"
)

// Tests the `readResponse` function with various buffer sizes
func TestReadResponseVariousBufSizes(t *testing.T) {
	close := testutils.MustBackgroundServer(t)
	defer close()

	for _, tc := range []struct {
		name string
		size int
	}{
		{
			name: "zero should use default",
			size: 0,
		},
		{
			name: "negative should use default",
			size: -999,
		},
		{
			name: "too big should use default",
			size: 1 << 30,
		},

		{
			name: "tiny buffer",
			size: 50,
		},

		{
			name: "1KB",
			size: 1 << 10,
		},
		{
			name: "64KB",
			size: 1 << 16,
		},
		{
			name: "128KB",
			size: 1 << 17,
		},
		{
			name: "512KB",
			size: 1 << 19,
		},
		{
			name: "1MB",
			size: 1 << 20,
		},
		{
			name: "1MB",
			size: 1 << 20,
		},
		{
			name: "16MB",
			size: 1 << 24,
		},
		{
			name: "max (128MB)",
			size: 1 << 27,
		},
	} {
		t.Run(tc.name, func(t *testing.T) {
			// The request needs to have a decent body so that we can verify the
			// smaller buffers are still able to read the response (which will
			// also contain said body) properly
			body := strings.Repeat(fmt.Sprintln("Hello world!"), 20)
			req, err := NewRequest(POST, url, bytes.NewBufferString(body))
			if err != nil {
				t.Fatalf("Failed to create request: %v", err)
			}

			resp, err := do(req, tc.size)
			if err != nil {
				t.Fatalf("Request failed: %v", err)
			}
			defer resp.Body.Close()
			if resp.StatusCode != http.StatusOK {
				t.Errorf(
					"Expected status code %v but got %v",
					http.StatusOK,
					resp.StatusCode)
			}
		})
	}
}
