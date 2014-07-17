all: torresmo doc

torresmo: torresmo.rs
	rustc $< -o $@

test-torresmo: torresmo.rs
	rustc -A dead_code --test $< -o $@

test: test-torresmo
	./test-torresmo

doc: torresmo.rs
	rustdoc $<

clean:
	rm test-torresmo torresmo
	rm -r doc
