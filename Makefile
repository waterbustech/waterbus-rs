.PHONY: signalling sfu build-docker

signalling:
	cargo run --bin signalling --release
sfu:
	cargo run --bin sfu --release
build-signalling:
	sudo docker build --platform=linux/amd64 -f docker/dockerfile.signalling -t lambiengcode/waterbus-signalling .
build-sfu:
	sudo docker build --platform=linux/amd64 -f docker/dockerfile.sfu -t lambiengcode/waterbus-sfu .
