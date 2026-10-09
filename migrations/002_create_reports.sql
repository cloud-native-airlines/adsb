CREATE TABLE reports (
    run_id TEXT NOT NULL,
    report_id TEXT NOT NULL,
    tick_id BIGINT NOT NULL,
    aircraft_id TEXT NOT NULL,
    flight_id TEXT NOT NULL,
    simulated_at TIMESTAMPTZ NOT NULL,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    altitude_m DOUBLE PRECISION NOT NULL,
    ground_speed_mps DOUBLE PRECISION NOT NULL,
    heading_deg DOUBLE PRECISION NOT NULL,
    status TEXT NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    PRIMARY KEY (run_id, report_id)
);

-- Latest position per aircraft within a run, and chronological history.
CREATE INDEX reports_latest_idx ON reports (run_id, aircraft_id, simulated_at DESC, tick_id DESC);
CREATE INDEX reports_history_idx ON reports (run_id, aircraft_id, simulated_at, tick_id);
