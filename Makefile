.PHONY: build run test test-integration

build:
	cargo build --release

run:
	cargo run

test:
	cargo test

test-integration:
	ADSB_TEST_DATABASE_URL=postgres://adsb:adsb@localhost:5432/adsb?sslmode=disable cargo test --test flight_api_integration -- --nocapture
