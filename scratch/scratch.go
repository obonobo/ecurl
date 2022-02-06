package main

import (
	"fmt"
	"log"
	"net/http"

	"github.com/obonobo/ecurl/echoserver"
	"github.com/obonobo/ecurl/ecurl"
)

func main() {
	fmt.Println("Starting echo server on port 8080...")
	echoServer()
	// funkyServer()
	// panic(echoserver.EchoServerRaw(context.Background()))
}

func funkyServer() {
	fmt.Println("Starting echo server on port 8080...")
	http.HandleFunc("/", func(rw http.ResponseWriter, r *http.Request) {
		rw.Header().Del("Content-Length")
		rw.Header().Add("Content-Length", fmt.Sprintf("%v", 30))
		rw.Header().Add("Content-Type", "text/plain; charset=utf-8")
		rw.WriteHeader(http.StatusOK)

		// body := strings.Repeat("Hello World! too short...\r\n\r\n", 1024)
		body := "Hello World! too short...\r\n\r\n"

		// _, err := rw.Write([]byte(body))
		// if err != nil {
		// 	fmt.Println(err)
		// 	return
		// }

		for i := 0; i < 100000; i++ {
			_, err := rw.Write([]byte(body))
			if err != nil {
				fmt.Println(err)
			}
		}

		// time.Sleep(5 * time.Second)
	})
	http.ListenAndServe(":8080", nil)
}

func echoServer() {
	// cancel, err := echoserver.EchoServer()
	cancel, err := echoserver.EchoServerWithAccessLogs(":8080", log.Default())
	defer cancel()
	panic(<-err)
}

func oldMain() {
	handleErr := func(err error) {
		if err != nil {
			log.Fatal(err)
		}
	}

	// Start up the echo server
	fmt.Println("Starting echo server on port 8080...")
	cancel, _ := echoserver.EchoServer(":8080")
	defer cancel()

	// Send it a request
	// req, err := ecurl.NewRequest("GET", "http://localhost:8080/", nil)
	// handleErr(err)

	// resp, err := ecurl.Do(req)
	// handleErr(err)

	r, err := ecurl.Get("http://localhost:8080/")
	handleErr(err)

	fmt.Println(r)
}
