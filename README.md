# Cloud Native Airlines ADS-B

The service uses Rust, Axum, and PostgreSQL. This phase implements health and flight APIs from [api.md](api.md). Report ingestion, position history, airport management, and aircraft registry APIs are later phases. Instrumentation is intentionally omitted for now.

## Run

```sh
docker compose up --build
```

The service listens at `http://localhost:8080`; PostgreSQL is exposed on port 5432 with demo credentials from `docker-compose.yml`. Versioned SQLx migrations run at startup.

To run directly, set `DATABASE_URL` (default `postgres://adsb:adsb@localhost:5432/adsb?sslmode=disable`) and `LISTEN_ADDR` (default `0.0.0.0:8080`), then run `cargo run`.

## API

### Health

- `GET /healthz` checks process liveness and returns `200 {"status":"ok"}`.
- `GET /readyz` checks PostgreSQL and returns `200 {"status":"ready"}` or `503` with a JSON error.

### Flight

`POST /api/v1/flight` creates a flight. It returns `201` with `{"flight": ...}` and `409 Conflict` when `flight_id` already exists.

`PUT /api/v1/flight` upserts the complete flight record by `flight_id`. It returns `201` with `created: true` for a new record and `200` with `created: false` for an update. Both methods accept:

```json
{
  "flight_id": "cna-run-42-flight-1",
  "flight": "CNA1234",
  "source": "MSP",
  "destination": "ORD",
  "scheduled_departure": "2026-10-06T08:00:00Z",
  "actual_departure": null,
  "scheduled_arrival": "2026-10-06T09:30:00Z",
  "actual_arrival": null,
  "aircraft": "N100CA"
}
```

All fields except actual times and `aircraft` are required. Times use RFC 3339. Scheduled arrival must follow scheduled departure; if both actual times are set, actual arrival cannot precede actual departure. Airport codes must be nonempty and at most 8 characters.

`GET /api/v1/flight/search` supports exact filters `source`, `destination`, `flight`, `aircraft`, and `pending_arrivals_at`. For example, `pending_arrivals_at=ORD` selects flights with destination ORD and no actual arrival. Inclusive scheduled time range filters are `departure_from`, `departure_to`, `arrival_from`, and `arrival_to` in RFC 3339. Pagination uses `limit` (default 100, maximum 500) and `offset` (default 0). Results are ordered by scheduled departure and flight ID and returned as `{"flights":[...],"pagination":{"limit":100,"offset":0,"returned":1}}`.

All errors use `{"error":{"code":"...","message":"..."}}`. Invalid bodies and filters return `400`; duplicate POST returns `409`; database failures return `500`.

## Example

```sh
curl -i -X POST http://localhost:8080/api/v1/flight \
  -H 'Content-Type: application/json' \
  --data '{"flight_id":"cna-run-42-flight-1","flight":"DL1234","source":"MSP","destination":"ORD","scheduled_departure":"2026-10-06T08:00:00Z","scheduled_arrival":"2026-10-06T09:30:00Z","aircraft":"N100CA"}'

curl -s 'http://localhost:8080/api/v1/flight/search?pending_arrivals_at=ORD&aircraft=N100CA'
```

## Tests

```sh
cargo test
docker compose up -d postgres
ADSB_TEST_DATABASE_URL=postgres://adsb:adsb@localhost:5432/adsb?sslmode=disable cargo test --test flight_api_integration -- --nocapture
```

Unit tests cover flight validation and search pagination. The integration test creates and removes an isolated PostgreSQL schema and checks health, readiness, create, duplicate conflict, update, search filters, validation, and persisted values.
