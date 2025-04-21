run:
	cargo run
build-docker:
	sudo docker build --platform=linux/amd64 -t lambiengcode/waterbus-rs .
