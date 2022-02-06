package testutils

// INLINED FILE: `unterminatedcomments.src`
const UNTERMINATEDCOMMENTS_SRC = `
// this is an inline comment

/* this is a single line block comment
`

// INLINED FILE: `unterminatedcomments.src`
const UNTERMINATEDCOMMENTS2_SRC = `
/* this is an imbricated
/* block comment
`

// INLINED FILE: `lexpositivegrading.src`
const LEX_POSITIVE_GRADING_SRC = `
==	+	|	(	;	if 	public	read
<>	-	&	)	,	then	private	write
<	*	!	{	.	else	func	return
>	/		}	:	integer	var	self
<=	=		[	::	float	struct	inherits
>=			]	->	void	while	let
						func	impl





0
1
10
12
123
12345

1.23
12.34
120.34e10
12345.6789e-123

abc
abc1
a1bc
abc_1abc
abc1_abc

// this is an inline comment

/* this is a single line block comment */

/* this is a
multiple line
block comment
*/

/* this is an imbricated
/* block comment
*/
*/




`

// INLINED FILE: `lexnegativegrading.src`
const LEX_NEGATIVE_GRADING_SRC = `
@ # $ ' \ ~

00
01
010
0120
01230
0123450

01.23
012.34
12.340
012.340

012.34e10
12.34e010

_abc
1abc
_1abc

`

// INLINED FILE: `helloworld.src`
const LEX_HELLOWORLD_SRC = `
/*
This is an imaginary program with a made up syntax.

Let us see how the parser handles it...
*/

// C-style struct
struct Student {
    float age;
    integer id;
};

public func main() {

    // x is my integer variable
    let x = 10;

    /*
    y is equal to x
    */
    var y = x;

    // Equality check
    if (y == x) then {
        var out integer[10] = {x, y, 69, 200, 89};
        write(out);  // Assume we have a function called 'write'
    }
}
`

// INLINED FILE: `strings.src`
const LEX_STRINGS_SRC = `
var x = "this is not valid";
`

// INLINED FILE: `somethingelse.src`
const LEX_SOMETHING_ELSE_SRC = `
package main

import (
	"encoding/json"
	"fmt"
	"io"
	"log"
	"net/http"
)

func main() {
	resp, err := http.Get("https://www.google.com")
	if err != nil {
		log.Fatalf("Request failed: %v", err)
	}

	headers, err := json.MarshalIndent(resp.Header, "", "    ")
	if err != nil {
		log.Fatalf("Failed to serialize response headers: %v", err)
	}
	fmt.Println(string(headers))

	bod, err := io.ReadAll(resp.Body)
	if err != nil {
		log.Fatalf("Failed to read body")
	}
	defer resp.Body.Close()

	fmt.Println(string(bod))
}
`
