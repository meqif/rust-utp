all: utp doc test

lib: src/lib/utp.rs
	rustc --crate-type=dylib $<

utp: src/main.rs lib
	rustc -L . src/main.rs -o $@

test-utp: src/lib/utp.rs
	rustc -A dead_code --test $< -o $@

test: test-utp
	./test-utp --color always

doc: src/lib/utp.rs
	rustdoc $<

clean:
	rm -rf -- test-utp utp *.dylib
	rm -rf doc

.PHONY: lib
