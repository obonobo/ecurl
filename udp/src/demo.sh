# Router no drop
./router.go --drop-rate 0.5 --max-delay 5ms


# Router drop
./router.go --drop-rate 0.02 --max-delay 1ms


# Router big drop
./router.go --drop-rate 0.5 --max-delay 5ms



# GET MAKEFILE:
make; make copy; ./client --verbose --proxy 127.0.0.1:3000 --get 127.0.0.1:8080/Makefile


# LIST DIRECTORY:
make; make copy; ./client --verbose --proxy 127.0.0.1:3000 --get 127.0.0.1:8080/


# POST INLINE DATA:
make; make copy; ./client --verbose --proxy 127.0.0.1:3000 --inline-data 'Hello world!' --post 127.0.0.1:8080/some_file.txt


# POST FILE DATA:
echo 'Hello world!

Lorem ipsum dolor sit amet, consectetur adipiscing elit, sed do eiusmod tempor
incididunt ut labore et dolore magna aliqua. Ut enim ad minim veniam, quis
nostrud exercitation ullamco laboris nisi ut aliquip ex ea commodo consequat.
Duis aute irure dolor in reprehenderit in voluptate velit esse cillum dolore eu
fugiat nulla pariatur. Excepteur sint occaecat cupidatat non proident, sunt in
culpa qui officia deserunt mollit anim id est laborum.
' > upload_me.txt && make; make copy; ./client --verbose --proxy 127.0.0.1:3000 --file 'upload_me.txt' --post 127.0.0.1:8080/some_file.txt && echo -e "\n\nSUCCESS\nPrinting file:" && cat some_file.txt
