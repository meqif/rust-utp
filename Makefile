all: torresmo doc test

lib: src/lib/utp.rs
	rustc --crate-type=dylib $<

torresmo: src/main.rs lib
	rustc -L . src/main.rs -o $@

test-torresmo: src/lib/utp.rs
	rustc -A dead_code --test $< -o $@

test: test-torresmo
	./test-torresmo --color always

doc: src/lib/utp.rs
	rustdoc $<

clean:
	rm test-torresmo torresmo *.dylib
	rm -r doc

.PHONY: lib
