torresmo: torresmo.rs
	rustc $< -o $@

test-torresmo: torresmo.rs
	rustc -A dead_code --test $< -o $@

test: test-torresmo
	./test-torresmo

clean:
	rm test-torresmo torresmo
