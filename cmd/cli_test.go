package cmd

import (
	"testing"

	"github.com/obonobo/ecurl/internal/testutils"
)

func mockStdoutStderr(t *testing.T) (output func() string) {
	output, err := testutils.MockStdoutStderr()
	if err != nil {
		t.Fatal(err)
	}
	return output
}
