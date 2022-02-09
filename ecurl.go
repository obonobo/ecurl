package main

import "github.com/obonobo/ecurl/cmd"

// This main function just runs the CLI
func main() {
	// con, err := net.Dial("tcp", "golang.org:80")
	// if err != nil {
	// 	panic(err)
	// }

	// _, err = con.Write([]byte("GET / HTTP/1.1\r\nUser-Agent: ecurl/0.1.0\r\nAccept: */*\r\nConnection: close\r\n\r\n"))
	// if err != nil {
	// 	panic(err)
	// }

	// res, err := ioutil.ReadAll(con)
	// if err != nil {
	// 	panic(err)
	// }

	// fmt.Println(string(res))
	cmd.RunAndExit()
}
