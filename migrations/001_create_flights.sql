CREATE TABLE flights (
    flight_id TEXT PRIMARY KEY,
    flight TEXT NOT NULL,
    source TEXT NOT NULL,
    destination TEXT NOT NULL,
    scheduled_departure TIMESTAMPTZ NOT NULL,
    actual_departure TIMESTAMPTZ,
    scheduled_arrival TIMESTAMPTZ NOT NULL,
    actual_arrival TIMESTAMPTZ,
    aircraft TEXT,
    CHECK (scheduled_arrival > scheduled_departure),
    CHECK (actual_arrival IS NULL OR actual_departure IS NULL OR actual_arrival >= actual_departure)
);

CREATE INDEX flights_source_departure_idx ON flights (source, scheduled_departure);
CREATE INDEX flights_destination_arrival_idx ON flights (destination, scheduled_arrival);
CREATE INDEX flights_aircraft_departure_idx ON flights (aircraft, scheduled_departure);
CREATE INDEX flights_designator_departure_idx ON flights (flight, scheduled_departure);
CREATE INDEX flights_pending_arrivals_idx ON flights (destination, scheduled_arrival) WHERE actual_arrival IS NULL;
