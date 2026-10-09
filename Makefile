.PHONY: build run test test-integration

# Regenerate Cargo.lock whenever Cargo.toml changes (e.g. a version bump) so it
# stays in sync. CI runs `cargo test --locked`, which fails on a stale lockfile;
# commit the resulting Cargo.lock alongside the Cargo.toml change.
Cargo.lock: Cargo.toml
	cargo update --workspace

build: Cargo.lock
	cargo build --release

run: Cargo.lock
	cargo run

test: Cargo.lock
	cargo test

test-integration: Cargo.lock
	ADSB_TEST_DATABASE_URL=postgres://adsb:adsb@localhost:5432/adsb?sslmode=disable cargo test --test flight_api_integration -- --nocapture
