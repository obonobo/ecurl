package main

import (
	"bytes"
	"context"
	"fmt"
	"io"
	"log"
	"net/http"
	"time"

	"github.com/obonobo/ecurl/ecurl"
)

type BufferWrapper struct {
	*bytes.Buffer
}

func main() {
	handleErr := func(err error) {
		if err != nil {
			log.Fatal(err)
		}
	}

	// Start up the echo server
	fmt.Println("Starting echo server on port 8080...")
	cancel, _ := EchoServer(":8080")
	defer cancel()

	// Send it a request
	req, err := ecurl.NewRequest("GET", "http://golang.org/", nil)
	handleErr(err)

	resp, err := ecurl.Do(req)
	handleErr(err)

	fmt.Println(resp)

	// conn, err := net.Dial("tcp", "golang.org:80")
	// if err != nil {
	// 	log.Fatalf("%v", err)
	// }

	// fmt.Fprintf(conn, "GET / HTTP/1.1\r\nHost: golang.org\r\nUser-Agent: curl/7.68.0\r\nAccept: */*\r\n\r\n")

	// res, err := io.ReadAll(conn)
	// if err != nil {
	// 	fmt.Println("Got an error reading input")
	// 	fmt.Println(err)
	// }
	// fmt.Println(string(res))

	// scanner := bufio.NewScanner(conn)
	// for scanner.Scan() {
	// 	line := scanner.Text()
	// 	fmt.Println(line)
	// 	if err := scanner.Err(); err != nil {
	// 		fmt.Println("End of message")
	// 		break
	// 	}
	// }
}

// Spins up a server that responds to requests by echoing back the request. The
// server response will contain a JSON document describing the request.
//
// addr parameter is optional, the first addr specified will be used if provided
//
// Use cancel() to shutdown the server gracefully, read from errc to get the
// result of srv.ListenAndServe
func EchoServer(addr ...string) (cancel func(), errc <-chan error) {
	address := ":8080"
	if len(addr) > 0 {
		address = addr[0]
	}

	readyc, errcc := make(chan struct{}, 1), make(chan error, 1)
	ctx, cancel := context.WithCancel(context.TODO())
	srv := &http.Server{
		Addr: address,
		Handler: http.HandlerFunc(func(rw http.ResponseWriter, r *http.Request) {
			rw.WriteHeader(http.StatusOK)
			rw.Write([]byte(fmt.Sprintf("%v %v %v\r\n", r.Method, r.URL.Path, r.Proto)))
			rw.Write([]byte(fmt.Sprintf("Host: %v\r\n", r.Host)))
			r.Header.Write(rw)
			rw.Write([]byte("\r\n"))
			io.Copy(rw, r.Body)
		}),
	}

	// Start the server in the background
	go func() {
		e := make(chan error, 1)
		go func() { e <- srv.ListenAndServe() }()
		readyc <- struct{}{}
		select {
		case <-ctx.Done():
			ctx, cancel := context.WithTimeout(context.TODO(), 30*time.Second)
			errcc <- srv.Shutdown(ctx)
			cancel()
		case errcc <- <-e:
		}
	}()

	<-readyc
	return cancel, errcc
}
