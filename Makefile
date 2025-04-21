run:
	cargo run
build-docker:
	docker buildx build \
		--platform linux/amd64,linux/arm64 \
		-t lambiengcode/waterbus-rs:latest \
		--push .
