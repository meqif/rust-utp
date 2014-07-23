all: lib examples doc test

lib: src/utp/lib.rs
	rustc --crate-type=dylib $<

examples: utp-cat

utp-cat: examples/utp-cat/main.rs lib
	rustc -L . examples/utp-cat/main.rs -o $@

test-utp: src/utp/lib.rs
	rustc -A dead_code --test $< -o $@

test: test-utp
	./test-utp --color always

doc: src/utp/lib.rs
	rustdoc $<

clean:
	rm -rf -- test-utp utp-cat *.dylib
	rm -rf doc

.PHONY: lib
