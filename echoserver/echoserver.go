package echoserver

import (
	"bufio"
	"context"
	"fmt"
	"io"
	"log"
	"net"
	"net/http"
	"runtime"
	"strconv"
	"strings"
	"time"
)

// A namespace for some middleware functions
var middleware = struct {
	recovery func(*log.Logger) func(http.Handler) http.Handler
	logging  func(*log.Logger) func(http.Handler) http.Handler
}{
	// Catches panicking request handlers and logs the error + stack trace
	recovery: func(logger *log.Logger) func(h http.Handler) http.Handler {
		return func(handler http.Handler) http.Handler {
			return http.HandlerFunc(func(rw http.ResponseWriter, r *http.Request) {
				defer func() {
					err := recover()
					if err == nil || logger == nil {
						return
					}
					rw.WriteHeader(http.StatusInternalServerError)
					logger.Printf("EchoServer caught panic: %v\n", err)
					buf := make([]byte, 1<<20)
					n := runtime.Stack(buf, true)
					logger.Printf("%s\n", buf[:n])
				}()
				handler.ServeHTTP(rw, r)
			})
		}
	},

	// Logs information about the request and response
	logging: func(logger *log.Logger) func(handler http.Handler) http.Handler {
		if logger == nil {
			return func(handler http.Handler) http.Handler { return handler }
		}

		return func(handler http.Handler) http.Handler {
			// Returns only the hostname/ip from an address
			trimAddress := func(addr string) string {
				if !strings.Contains(addr, ":") {
					return addr
				}
				return strings.TrimRight(
					strings.TrimRightFunc(
						addr, func(r rune) bool { return r != ':' }), ":")
			}

			const (
				LEFT = iota - 1
				MIDDLE
				RIGHT
			)

			// Pads the string with spaces so that it is totalWidth long
			pad := func(s string, width, position int) string {
				spaces := width - len(s)
				if spaces < 0 {
					spaces = 0
				}
				switch {
				case position <= LEFT: // PAD LEFT
					return s + strings.Repeat(" ", spaces)
				case position >= RIGHT: // PAD RIGHT
					return strings.Repeat(" ", spaces) + s
				case position == MIDDLE: // PAD CENTER
					roundingError := spaces - (spaces/2)*2
					return strings.Repeat(" ", roundingError+(spaces/2)) +
						s + strings.Repeat(" ", spaces/2)
				default:
					return s
				}
			}

			return http.HandlerFunc(func(rw http.ResponseWriter, r *http.Request) {
				recorder := &responseRecorder{rw, 0}

				start := time.Now()
				handler.ServeHTTP(recorder, r)
				timeTaken := time.Since(start)

				log.Printf(""+
					"%v | %v | %v | %v | %v\n",
					pad("EchoServer", 0, MIDDLE),
					recorder.status,
					pad(fmt.Sprintf("%v", timeTaken), 12, RIGHT),
					pad(fmt.Sprintf(
						"%v → %v",
						trimAddress(r.RemoteAddr),
						trimAddress(r.Host)),
						29, MIDDLE),
					fmt.Sprintf("%v %v", strings.ToUpper(r.Method), r.URL),
				)
			})
		}
	},
}

// An http.ResponseWriter wrapper that records the status code of the response
type responseRecorder struct {
	http.ResponseWriter
	status int
}

func (r *responseRecorder) WriteHeader(status int) {
	r.status = status
	r.ResponseWriter.WriteHeader(status)
}

// Runs an EchoServer with an optional access log showing requests made to the
// server as well as some information about the response
func EchoServerWithAccessLogs(
	addr string,
	logger *log.Logger,
) (
	cancel func(),
	errc <-chan error,
) {
	address := ":8080" // default port binding
	if len(addr) > 0 {
		address = addr
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

	// If logs are enabled, then add a middleware that provides access logs,
	// also add a recovery middleware to catch panics
	srv.Handler = middleware.
		logging(logger)(middleware.
		recovery(logger)(srv.Handler))

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

// Spins up a server that responds to requests by echoing back the request.
//
// addr parameter is optional, the first addr specified will be used if provided
//
// Use cancel() to shutdown the server gracefully, read from errc to get the
// result of srv.ListenAndServe
func EchoServer(addr ...string) (cancel func(), errc <-chan error) {
	address := ""
	if len(addr) > 0 {
		address = addr[0]
	}

	return EchoServerWithAccessLogs(address, nil)
}

// An echo server that uses raw TCP sockets. Use ctx and run this function in a
// go routine if you want to be able to preempt the server and shut it down
// gracefully. This is a different mode of operation to the EchoServer above ^^^
func EchoServerRaw(ctx context.Context) error {
	l, err := net.Listen("tcp", ":8080")
	if err != nil {
		return fmt.Errorf("net.Listen() got an error: %w", err)
	}
	defer l.Close()

	for {
		select {
		case <-ctx.Done():
			return fmt.Errorf("received shutdown signal from ctx")
		default:
		}

		conn, err := l.Accept()
		if err != nil {
			// Server fails if listener.Accept() fails
			return fmt.Errorf("listener.Accept() got an error: %w", err)
		}

		go func() {
			defer conn.Close()
			conn.SetDeadline(time.Now().Add(60 * time.Second))

			// Accept every kind of request
			fmt.Fprintf(conn, "HTTP/1.1 200 OK\r\n")

			var headers string
			var contentLength int
			scnr := bufio.NewScanner(conn)
			for scnr.Scan() {
				line := scnr.Text()
				if line == "" {
					break
				}
				split := strings.Split(line, ":")
				if len(split) > 1 && strings.ToLower(split[0]) == "content-length" {
					contentLength, err = strconv.Atoi(strings.Trim(split[1], " "))
					if err != nil {
						fmt.Println("Got an error parsing Content-Length: " + err.Error())
						return
					}
				}
				headers += line + "\r\n"
			}
			fmt.Fprintf(conn, "Content-Length: %v\r\n\r\n", len(headers)+contentLength+2)
			fmt.Fprintf(conn, headers+"\r\n")

			var red int
			for red < contentLength && scnr.Scan() {
				line := scnr.Text()
				red += len(line) + 1
				fmt.Fprintf(conn, "%v\n", line)
			}
		}()
	}
}
