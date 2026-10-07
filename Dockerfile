FROM rust:1-bookworm AS build
WORKDIR /src
COPY Cargo.toml Cargo.lock ./
COPY src ./src
COPY migrations ./migrations
RUN cargo fetch --locked
RUN cargo build --release --locked --bin adsb

FROM gcr.io/distroless/cc-debian12:nonroot
WORKDIR /
COPY --from=build /src/target/release/adsb /adsb
EXPOSE 8080
USER nonroot:nonroot
ENTRYPOINT ["/adsb"]
