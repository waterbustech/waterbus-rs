signalling:
	cargo run --bin signalling
sfu:
	cargo run --bin sfu
build-proto:
	cargo build -p waterbus-proto 
build-signalling:
	podman build --platform=linux/amd64 -f docker/dockerfile.signalling -t docker.io/lambiengcode/waterbus-signalling .
build-sfu:
	podman build --platform=linux/amd64 -f docker/dockerfile.sfu -t docker.io/lambiengcode/waterbus-sfu .
clippy:
	cargo clippy --all-targets --all-features -- -D warnings