package main

import (
	"fmt"
	"io"
	"log"
	"net"
)

func main() {
	conn, err := net.Dial("tcp", "golang.org:80")
	if err != nil {
		log.Fatalf("%v", err)
	}

	fmt.Fprintf(conn, "GET / HTTP/1.1\r\nHost: golang.org\r\nUser-Agent: curl/7.68.0\r\nAccept: */*\r\n\r\n")

	res, err := io.ReadAll(conn)
	if err != nil {
		fmt.Println("Got an error reading input")
		fmt.Println(err)
	}
	fmt.Println(string(res))

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
