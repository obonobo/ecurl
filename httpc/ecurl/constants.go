package ecurl

const (
	HTTP  = "http"  // Acceptable protocol #1
	HTTPS = "https" // Acceptable protocol #2

	GET  = "GET"  // Acceptable method #1
	POST = "POST" // Acceptable method #1
)

func isAcceptableProto(proto string) bool {
	return proto == HTTP || proto == HTTPS
}

func isAcceptableMethod(method string) bool {
	return method == GET || method == POST
}
