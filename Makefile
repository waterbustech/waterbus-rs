.PHONY: signalling sfu build-docker

signalling:
	cargo run --bin signalling
sfu:
	cargo run --bin sfu
build-docker:
	sudo docker build --platform=linux/amd64 -t lambiengcode/waterbus-rs .
