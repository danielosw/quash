quash:
	cargo build
	# copy from build directory to .
	cp ./target/debug/quash .
clean:
	cargo clean
	rm quash
