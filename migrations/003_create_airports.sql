CREATE TABLE airports (
    code TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    latitude DOUBLE PRECISION NOT NULL,
    longitude DOUBLE PRECISION NOT NULL,
    elevation_m DOUBLE PRECISION,
    icao TEXT,
    country TEXT
);

CREATE INDEX airports_location_idx ON airports (longitude, latitude);
