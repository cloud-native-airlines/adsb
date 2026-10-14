use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Path, Query, State,
    },
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, PgPool, Postgres, QueryBuilder};
use std::str::FromStr;

#[derive(Clone)]
pub struct AppState {
    pub store: FlightStore,
    pub reports: ReportStore,
    pub airports: AirportStore,
}

#[derive(Clone)]
pub struct FlightStore {
    pool: PgPool,
}

impl FlightStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    pub async fn connect(database_url: &str) -> Result<Self, sqlx::Error> {
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect(database_url)
            .await?;
        Ok(Self::new(pool))
    }

    pub fn pool(&self) -> &PgPool {
        &self.pool
    }

    pub async fn ready(&self) -> Result<(), sqlx::Error> {
        sqlx::query("SELECT 1").execute(&self.pool).await?;
        Ok(())
    }

    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        sqlx::migrate!().run(&self.pool).await
    }

    pub async fn create(&self, flight: &Flight) -> Result<Flight, sqlx::Error> {
        sqlx::query_as::<_, Flight>(
            "INSERT INTO flights (flight_id, flight, source, destination, scheduled_departure, actual_departure, scheduled_arrival, actual_arrival, aircraft) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING flight_id, flight, source, destination, scheduled_departure, actual_departure, scheduled_arrival, actual_arrival, aircraft"
        )
        .bind(&flight.flight_id).bind(&flight.flight).bind(&flight.source).bind(&flight.destination)
        .bind(flight.scheduled_departure).bind(flight.actual_departure).bind(flight.scheduled_arrival)
        .bind(flight.actual_arrival).bind(&flight.aircraft).fetch_one(&self.pool).await
    }

    pub async fn upsert(&self, flight: &Flight) -> Result<(Flight, bool), sqlx::Error> {
        // PostgreSQL reports whether the insert or update branch ran through xmax.
        let row = sqlx::query_as::<_, FlightUpsertRow>(
            "INSERT INTO flights (flight_id, flight, source, destination, scheduled_departure, actual_departure, scheduled_arrival, actual_arrival, aircraft) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) \
             ON CONFLICT (flight_id) DO UPDATE SET flight=EXCLUDED.flight, source=EXCLUDED.source, destination=EXCLUDED.destination, \
             scheduled_departure=EXCLUDED.scheduled_departure, actual_departure=EXCLUDED.actual_departure, scheduled_arrival=EXCLUDED.scheduled_arrival, \
             actual_arrival=EXCLUDED.actual_arrival, aircraft=EXCLUDED.aircraft \
             RETURNING flight_id, flight, source, destination, scheduled_departure, actual_departure, scheduled_arrival, actual_arrival, aircraft, (xmax = 0) AS created"
        )
        .bind(&flight.flight_id).bind(&flight.flight).bind(&flight.source).bind(&flight.destination)
        .bind(flight.scheduled_departure).bind(flight.actual_departure).bind(flight.scheduled_arrival)
        .bind(flight.actual_arrival).bind(&flight.aircraft).fetch_one(&self.pool).await?;
        let created = row.created;
        Ok((row.into_flight(), created))
    }

    pub async fn search(&self, filters: &FlightSearch) -> Result<Vec<Flight>, sqlx::Error> {
        let mut query = QueryBuilder::<Postgres>::new(
            "SELECT flight_id, flight, source, destination, scheduled_departure, actual_departure, scheduled_arrival, actual_arrival, aircraft FROM flights WHERE TRUE"
        );
        if let Some(value) = &filters.source {
            query.push(" AND source = ").push_bind(value);
        }
        if let Some(value) = &filters.destination {
            query.push(" AND destination = ").push_bind(value);
        }
        if let Some(value) = &filters.flight {
            query.push(" AND flight = ").push_bind(value);
        }
        if let Some(value) = &filters.aircraft {
            query.push(" AND aircraft = ").push_bind(value);
        }
        if let Some(value) = filters.departure_from {
            query.push(" AND scheduled_departure >= ").push_bind(value);
        }
        if let Some(value) = filters.departure_to {
            query.push(" AND scheduled_departure <= ").push_bind(value);
        }
        if let Some(value) = filters.arrival_from {
            query.push(" AND scheduled_arrival >= ").push_bind(value);
        }
        if let Some(value) = filters.arrival_to {
            query.push(" AND scheduled_arrival <= ").push_bind(value);
        }
        query
            .push(" ORDER BY scheduled_departure, flight_id LIMIT ")
            .push_bind(filters.limit.unwrap_or(100));
        query
            .push(" OFFSET ")
            .push_bind(filters.offset.unwrap_or(0));
        query.build_query_as::<Flight>().fetch_all(&self.pool).await
    }
}

#[derive(sqlx::FromRow)]
struct FlightUpsertRow {
    flight_id: String,
    flight: String,
    source: String,
    destination: String,
    scheduled_departure: DateTime<Utc>,
    actual_departure: Option<DateTime<Utc>>,
    scheduled_arrival: DateTime<Utc>,
    actual_arrival: Option<DateTime<Utc>>,
    aircraft: Option<String>,
    created: bool,
}

impl FlightUpsertRow {
    fn into_flight(self) -> Flight {
        Flight {
            flight_id: self.flight_id,
            flight: self.flight,
            source: self.source,
            destination: self.destination,
            scheduled_departure: self.scheduled_departure,
            actual_departure: self.actual_departure,
            scheduled_arrival: self.scheduled_arrival,
            actual_arrival: self.actual_arrival,
            aircraft: self.aircraft,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq)]
pub struct Flight {
    pub flight_id: String,
    pub flight: String,
    pub source: String,
    pub destination: String,
    pub scheduled_departure: DateTime<Utc>,
    pub actual_departure: Option<DateTime<Utc>>,
    pub scheduled_arrival: DateTime<Utc>,
    pub actual_arrival: Option<DateTime<Utc>>,
    pub aircraft: Option<String>,
}

impl Flight {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.flight_id.trim().is_empty() {
            return Err("flight_id must not be empty");
        }
        if self.flight.trim().is_empty() {
            return Err("flight must not be empty");
        }
        if self.source.trim().is_empty() {
            return Err("source must not be empty");
        }
        if self.destination.trim().is_empty() {
            return Err("destination must not be empty");
        }
        if self.source.len() > 8 || self.destination.len() > 8 {
            return Err("airport codes must be at most 8 characters");
        }
        if self.scheduled_arrival <= self.scheduled_departure {
            return Err("scheduled_arrival must be after scheduled_departure");
        }
        if let (Some(departure), Some(arrival)) = (self.actual_departure, self.actual_arrival) {
            if arrival < departure {
                return Err("actual_arrival must not be before actual_departure");
            }
        }
        if self
            .aircraft
            .as_ref()
            .is_some_and(|id| id.trim().is_empty())
        {
            return Err("aircraft must not be empty when provided");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct FlightSearch {
    pub source: Option<String>,
    pub destination: Option<String>,
    pub flight: Option<String>,
    pub aircraft: Option<String>,
    pub departure_from: Option<DateTime<Utc>>,
    pub departure_to: Option<DateTime<Utc>>,
    pub arrival_from: Option<DateTime<Utc>>,
    pub arrival_to: Option<DateTime<Utc>>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

impl FlightSearch {
    fn validate(&self) -> Result<(i64, i64), &'static str> {
        for (start, end) in [
            (self.departure_from, self.departure_to),
            (self.arrival_from, self.arrival_to),
        ] {
            if start.zip(end).is_some_and(|(from, to)| from > to) {
                return Err("range start must not be after range end");
            }
        }
        let limit = self.limit.unwrap_or(100);
        let offset = self.offset.unwrap_or(0);
        if !(1..=500).contains(&limit) {
            return Err("limit must be between 1 and 500");
        }
        if offset < 0 {
            return Err("offset must be nonnegative");
        }
        Ok((limit, offset))
    }
}

#[derive(Clone)]
pub struct ReportStore {
    pool: PgPool,
}

/// Outcome of ingesting a report, keyed on (run_id, report_id).
pub enum Ingest {
    Created(Report),
    Duplicate(Report),
    Conflict,
}

impl ReportStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// Insert a report. Retries with identical content are idempotent; reusing a
    /// (run_id, report_id) with different content is a conflict.
    pub async fn ingest(&self, report: &Report) -> Result<Ingest, sqlx::Error> {
        let inserted = sqlx::query_as::<_, Report>(
            "INSERT INTO reports (run_id, report_id, tick_id, aircraft_id, flight_id, simulated_at, latitude, longitude, altitude_m, ground_speed_mps, heading_deg, status) \
             VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) \
             ON CONFLICT (run_id, report_id) DO NOTHING \
             RETURNING run_id, report_id, tick_id, aircraft_id, flight_id, simulated_at, latitude, longitude, altitude_m, ground_speed_mps, heading_deg, status, received_at"
        )
        .bind(&report.run_id).bind(&report.report_id).bind(report.tick_id)
        .bind(&report.aircraft_id).bind(&report.flight_id).bind(report.simulated_at)
        .bind(report.latitude).bind(report.longitude).bind(report.altitude_m)
        .bind(report.ground_speed_mps).bind(report.heading_deg).bind(&report.status)
        .fetch_optional(&self.pool).await?;

        if let Some(saved) = inserted {
            return Ok(Ingest::Created(saved));
        }

        // Key already exists: idempotent if the content matches, else a conflict.
        let existing = sqlx::query_as::<_, Report>(
            "SELECT run_id, report_id, tick_id, aircraft_id, flight_id, simulated_at, latitude, longitude, altitude_m, ground_speed_mps, heading_deg, status, received_at \
             FROM reports WHERE run_id = $1 AND report_id = $2"
        )
        .bind(&report.run_id).bind(&report.report_id)
        .fetch_one(&self.pool).await?;

        if existing.same_content(report) {
            Ok(Ingest::Duplicate(existing))
        } else {
            Ok(Ingest::Conflict)
        }
    }

    /// Latest position for an aircraft within a run.
    pub async fn latest(&self, run_id: &str, aircraft_id: &str) -> Result<Option<Report>, sqlx::Error> {
        sqlx::query_as::<_, Report>(
            "SELECT run_id, report_id, tick_id, aircraft_id, flight_id, simulated_at, latitude, longitude, altitude_m, ground_speed_mps, heading_deg, status, received_at \
             FROM reports WHERE run_id = $1 AND aircraft_id = $2 ORDER BY simulated_at DESC, tick_id DESC LIMIT 1"
        )
        .bind(run_id).bind(aircraft_id)
        .fetch_optional(&self.pool).await
    }

    /// Chronological history for a run, optionally filtered to one aircraft.
    pub async fn history(&self, query: &ReportQuery, limit: i64, offset: i64) -> Result<Vec<Report>, sqlx::Error> {
        let mut builder = QueryBuilder::<Postgres>::new(
            "SELECT run_id, report_id, tick_id, aircraft_id, flight_id, simulated_at, latitude, longitude, altitude_m, ground_speed_mps, heading_deg, status, received_at \
             FROM reports WHERE run_id = "
        );
        builder.push_bind(&query.run_id);
        if let Some(aircraft) = &query.aircraft_id {
            builder.push(" AND aircraft_id = ").push_bind(aircraft);
        }
        builder.push(" ORDER BY simulated_at, tick_id LIMIT ").push_bind(limit);
        builder.push(" OFFSET ").push_bind(offset);
        builder.build_query_as::<Report>().fetch_all(&self.pool).await
    }

    /// Latest position per aircraft in a run, optionally restricted to a map
    /// viewport (bbox) and/or a status. The latest report per aircraft is taken
    /// first, then filtered — so an aircraft appears only if its *current*
    /// position is inside the view.
    pub async fn latest_positions(
        &self,
        run_id: &str,
        bbox: Option<Bbox>,
        status: Option<&str>,
        limit: i64,
    ) -> Result<Vec<Report>, sqlx::Error> {
        let mut b = QueryBuilder::<Postgres>::new(
            "WITH latest AS (\
               SELECT DISTINCT ON (aircraft_id) run_id, report_id, tick_id, aircraft_id, flight_id, simulated_at, latitude, longitude, altitude_m, ground_speed_mps, heading_deg, status, received_at \
               FROM reports WHERE run_id = ",
        );
        b.push_bind(run_id);
        b.push(" ORDER BY aircraft_id, simulated_at DESC, tick_id DESC) SELECT * FROM latest WHERE TRUE");
        if let Some(bb) = bbox {
            b.push(" AND longitude BETWEEN ").push_bind(bb.min_lon).push(" AND ").push_bind(bb.max_lon);
            b.push(" AND latitude BETWEEN ").push_bind(bb.min_lat).push(" AND ").push_bind(bb.max_lat);
        }
        if let Some(s) = status {
            b.push(" AND status = ").push_bind(s);
        }
        b.push(" ORDER BY aircraft_id LIMIT ").push_bind(limit);
        b.build_query_as::<Report>().fetch_all(&self.pool).await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq)]
pub struct Report {
    pub report_id: String,
    pub run_id: String,
    pub tick_id: i64,
    pub aircraft_id: String,
    pub flight_id: String,
    pub simulated_at: DateTime<Utc>,
    pub latitude: f64,
    pub longitude: f64,
    pub altitude_m: f64,
    pub ground_speed_mps: f64,
    pub heading_deg: f64,
    pub status: String,
    /// Real ingestion time, assigned by ADS-B. Absent on incoming payloads.
    #[serde(default, skip_deserializing)]
    pub received_at: Option<DateTime<Utc>>,
}

impl Report {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.report_id.trim().is_empty() {
            return Err("report_id must not be empty");
        }
        if self.run_id.trim().is_empty() {
            return Err("run_id must not be empty");
        }
        if self.aircraft_id.trim().is_empty() {
            return Err("aircraft_id must not be empty");
        }
        if self.flight_id.trim().is_empty() {
            return Err("flight_id must not be empty");
        }
        if self.tick_id < 0 {
            return Err("tick_id must not be negative");
        }
        if !(-90.0..=90.0).contains(&self.latitude) {
            return Err("latitude must be between -90 and 90");
        }
        if !(-180.0..=180.0).contains(&self.longitude) {
            return Err("longitude must be between -180 and 180");
        }
        if !(0.0..360.0).contains(&self.heading_deg) {
            return Err("heading_deg must be in [0, 360)");
        }
        if !matches!(self.status.as_str(), "parked" | "flying" | "turnaround") {
            return Err("status must be parked, flying, or turnaround");
        }
        Ok(())
    }

    /// Equality of the reported content, ignoring the server-assigned receipt time.
    fn same_content(&self, other: &Report) -> bool {
        self.tick_id == other.tick_id
            && self.aircraft_id == other.aircraft_id
            && self.flight_id == other.flight_id
            && self.simulated_at == other.simulated_at
            && self.latitude == other.latitude
            && self.longitude == other.longitude
            && self.altitude_m == other.altitude_m
            && self.ground_speed_mps == other.ground_speed_mps
            && self.heading_deg == other.heading_deg
            && self.status == other.status
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ReportQuery {
    pub run_id: String,
    pub aircraft_id: Option<String>,
    pub limit: Option<i64>,
    pub offset: Option<i64>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct LatestQuery {
    pub run_id: String,
    pub aircraft_id: String,
}

/// A geographic bounding box (a map viewport).
#[derive(Clone, Copy, Debug)]
pub struct Bbox {
    pub min_lon: f64,
    pub min_lat: f64,
    pub max_lon: f64,
    pub max_lat: f64,
}

/// Parse a `min_lon,min_lat,max_lon,max_lat` bbox string. Does not handle
/// antimeridian-crossing boxes (requires min_lon <= max_lon).
pub fn parse_bbox(value: &str) -> Result<Bbox, &'static str> {
    let parts: Vec<&str> = value.split(',').collect();
    if parts.len() != 4 {
        return Err("bbox must be min_lon,min_lat,max_lon,max_lat");
    }
    let mut nums = [0f64; 4];
    for (i, part) in parts.iter().enumerate() {
        nums[i] = part.trim().parse().map_err(|_| "bbox values must be numbers")?;
    }
    let bbox = Bbox { min_lon: nums[0], min_lat: nums[1], max_lon: nums[2], max_lat: nums[3] };
    if !(-180.0..=180.0).contains(&bbox.min_lon) || !(-180.0..=180.0).contains(&bbox.max_lon) {
        return Err("bbox longitudes must be in [-180, 180]");
    }
    if !(-90.0..=90.0).contains(&bbox.min_lat) || !(-90.0..=90.0).contains(&bbox.max_lat) {
        return Err("bbox latitudes must be in [-90, 90]");
    }
    if bbox.min_lon > bbox.max_lon {
        return Err("bbox min_lon must not exceed max_lon");
    }
    if bbox.min_lat > bbox.max_lat {
        return Err("bbox min_lat must not exceed max_lat");
    }
    Ok(bbox)
}

#[derive(Clone, Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PositionsQuery {
    pub run_id: String,
    pub bbox: Option<String>,
    pub status: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Clone)]
pub struct AirportStore {
    pool: PgPool,
}

impl AirportStore {
    pub fn new(pool: PgPool) -> Self {
        Self { pool }
    }

    /// List airports, optionally restricted to a map viewport (bbox).
    pub async fn list(&self, bbox: Option<Bbox>) -> Result<Vec<Airport>, sqlx::Error> {
        let mut b = QueryBuilder::<Postgres>::new(
            "SELECT code, name, latitude, longitude, elevation_m, icao, country FROM airports WHERE TRUE",
        );
        if let Some(bb) = bbox {
            b.push(" AND longitude BETWEEN ").push_bind(bb.min_lon).push(" AND ").push_bind(bb.max_lon);
            b.push(" AND latitude BETWEEN ").push_bind(bb.min_lat).push(" AND ").push_bind(bb.max_lat);
        }
        b.push(" ORDER BY code");
        b.build_query_as::<Airport>().fetch_all(&self.pool).await
    }

    pub async fn get(&self, code: &str) -> Result<Option<Airport>, sqlx::Error> {
        sqlx::query_as::<_, Airport>(
            "SELECT code, name, latitude, longitude, elevation_m, icao, country FROM airports WHERE code = $1",
        )
        .bind(code)
        .fetch_optional(&self.pool)
        .await
    }
}

#[derive(Clone, Debug, Serialize, Deserialize, sqlx::FromRow, PartialEq)]
pub struct Airport {
    pub code: String,
    pub name: String,
    pub latitude: f64,
    pub longitude: f64,
    #[serde(default)]
    pub elevation_m: Option<f64>,
    #[serde(default)]
    pub icao: Option<String>,
    #[serde(default)]
    pub country: Option<String>,
}

impl Airport {
    pub fn validate(&self) -> Result<(), &'static str> {
        if self.code.trim().is_empty() || self.code.len() > 8 {
            return Err("code must be between 1 and 8 characters");
        }
        if self.name.trim().is_empty() {
            return Err("name must not be empty");
        }
        if !(-90.0..=90.0).contains(&self.latitude) {
            return Err("latitude must be between -90 and 90");
        }
        if !(-180.0..=180.0).contains(&self.longitude) {
            return Err("longitude must be between -180 and 180");
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AirportQuery {
    pub bbox: Option<String>,
}

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    Invalid(&'static str),
    #[error("flight_id already exists")]
    Conflict,
    #[error("report id already exists with different content")]
    ReportConflict,
    #[error("{0}")]
    NotFound(&'static str),
    #[error("database operation failed")]
    Database,
}

#[derive(Serialize)]
struct ErrorBody {
    error: ErrorDetail,
}
#[derive(Serialize)]
struct ErrorDetail {
    code: &'static str,
    message: String,
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, code) = match &self {
            ApiError::Invalid(_) => (StatusCode::BAD_REQUEST, "invalid_request"),
            ApiError::Conflict => (StatusCode::CONFLICT, "flight_conflict"),
            ApiError::ReportConflict => (StatusCode::CONFLICT, "report_conflict"),
            ApiError::NotFound(_) => (StatusCode::NOT_FOUND, "not_found"),
            ApiError::Database => (StatusCode::INTERNAL_SERVER_ERROR, "database_error"),
        };
        (
            status,
            Json(ErrorBody {
                error: ErrorDetail {
                    code,
                    message: self.to_string(),
                },
            }),
        )
            .into_response()
    }
}

pub fn app(state: AppState) -> Router {
    Router::new()
        .route("/healthz", get(health))
        .route("/readyz", get(ready))
        .route("/api/v1/flight", post(create_flight).put(upsert_flight))
        .route("/api/v1/flight/search", get(search_flights))
        .route("/api/v1/reports", post(ingest_report).get(list_reports))
        .route("/api/v1/reports/latest", get(latest_report))
        .route("/api/v1/positions", get(list_positions))
        .route("/api/v1/airports", get(list_airports))
        .route("/api/v1/airports/{code}", get(get_airport))
        .fallback(not_found)
        .method_not_allowed_fallback(method_not_allowed)
        .with_state(state)
}

async fn not_found() -> impl IntoResponse {
    (
        StatusCode::NOT_FOUND,
        Json(serde_json::json!({"error":{"code":"not_found","message":"route not found"}})),
    )
}

async fn method_not_allowed() -> impl IntoResponse {
    (
        StatusCode::METHOD_NOT_ALLOWED,
        Json(
            serde_json::json!({"error":{"code":"method_not_allowed","message":"method is not supported for this route"}}),
        ),
    )
}

async fn health() -> Json<serde_json::Value> {
    Json(serde_json::json!({"status":"ok"}))
}

async fn ready(
    State(state): State<AppState>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    match state.store.ready().await {
        Ok(()) => Ok(Json(serde_json::json!({"status":"ready"}))),
        Err(_) => Err((
            StatusCode::SERVICE_UNAVAILABLE,
            Json(
                serde_json::json!({"error":{"code":"not_ready","message":"database is unavailable"}}),
            ),
        )),
    }
}

async fn create_flight(
    State(state): State<AppState>,
    payload: Result<Json<Flight>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(flight) = payload.map_err(|_| ApiError::Invalid("body must be valid flight JSON"))?;
    flight.validate().map_err(ApiError::Invalid)?;
    match state.store.create(&flight).await {
        Ok(saved) => Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({"flight":saved})),
        )),
        Err(error) if is_unique_violation(&error) => Err(ApiError::Conflict),
        Err(_) => Err(ApiError::Database),
    }
}

async fn upsert_flight(
    State(state): State<AppState>,
    payload: Result<Json<Flight>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(flight) = payload.map_err(|_| ApiError::Invalid("body must be valid flight JSON"))?;
    flight.validate().map_err(ApiError::Invalid)?;
    let (saved, created) = state
        .store
        .upsert(&flight)
        .await
        .map_err(|_| ApiError::Database)?;
    let status = if created {
        StatusCode::CREATED
    } else {
        StatusCode::OK
    };
    Ok((
        status,
        Json(serde_json::json!({"flight":saved,"created":created})),
    ))
}

async fn search_flights(
    State(state): State<AppState>,
    query: Result<Query<FlightSearch>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(mut filters) =
        query.map_err(|_| ApiError::Invalid("query parameters are invalid"))?;
    let (limit, offset) = filters.validate().map_err(ApiError::Invalid)?;
    filters.limit = Some(limit);
    filters.offset = Some(offset);
    let flights = state
        .store
        .search(&filters)
        .await
        .map_err(|_| ApiError::Database)?;
    let returned = flights.len();
    Ok(Json(
        serde_json::json!({"flights":flights,"pagination":{"limit":limit,"offset":offset,"returned":returned}}),
    ))
}

async fn ingest_report(
    State(state): State<AppState>,
    payload: Result<Json<Report>, JsonRejection>,
) -> Result<impl IntoResponse, ApiError> {
    let Json(report) = payload.map_err(|_| ApiError::Invalid("body must be valid report JSON"))?;
    report.validate().map_err(ApiError::Invalid)?;
    match state.reports.ingest(&report).await {
        Ok(Ingest::Created(saved)) => Ok((
            StatusCode::CREATED,
            Json(serde_json::json!({"report":saved,"created":true})),
        )),
        Ok(Ingest::Duplicate(saved)) => Ok((
            StatusCode::OK,
            Json(serde_json::json!({"report":saved,"created":false})),
        )),
        Ok(Ingest::Conflict) => Err(ApiError::ReportConflict),
        Err(_) => Err(ApiError::Database),
    }
}

async fn latest_report(
    State(state): State<AppState>,
    query: Result<Query<LatestQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::Invalid("run_id and aircraft_id are required"))?;
    match state
        .reports
        .latest(&q.run_id, &q.aircraft_id)
        .await
        .map_err(|_| ApiError::Database)?
    {
        Some(report) => Ok(Json(serde_json::json!({ "report": report }))),
        None => Err(ApiError::NotFound("no reports for that run and aircraft")),
    }
}

async fn list_reports(
    State(state): State<AppState>,
    query: Result<Query<ReportQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::Invalid("query parameters are invalid"))?;
    if q.run_id.trim().is_empty() {
        return Err(ApiError::Invalid("run_id is required"));
    }
    let limit = q.limit.unwrap_or(1000);
    let offset = q.offset.unwrap_or(0);
    if !(1..=5000).contains(&limit) {
        return Err(ApiError::Invalid("limit must be between 1 and 5000"));
    }
    if offset < 0 {
        return Err(ApiError::Invalid("offset must be nonnegative"));
    }
    let reports = state
        .reports
        .history(&q, limit, offset)
        .await
        .map_err(|_| ApiError::Database)?;
    let returned = reports.len();
    Ok(Json(serde_json::json!({
        "reports": reports,
        "pagination": {"limit": limit, "offset": offset, "returned": returned}
    })))
}

async fn list_positions(
    State(state): State<AppState>,
    query: Result<Query<PositionsQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::Invalid("query parameters are invalid"))?;
    if q.run_id.trim().is_empty() {
        return Err(ApiError::Invalid("run_id is required"));
    }
    let bbox = match &q.bbox {
        Some(value) => Some(parse_bbox(value).map_err(ApiError::Invalid)?),
        None => None,
    };
    let limit = q.limit.unwrap_or(5000);
    if !(1..=10000).contains(&limit) {
        return Err(ApiError::Invalid("limit must be between 1 and 10000"));
    }
    let positions = state
        .reports
        .latest_positions(&q.run_id, bbox, q.status.as_deref(), limit)
        .await
        .map_err(|_| ApiError::Database)?;
    let returned = positions.len();
    Ok(Json(serde_json::json!({
        "positions": positions,
        "pagination": {"limit": limit, "returned": returned}
    })))
}

async fn list_airports(
    State(state): State<AppState>,
    query: Result<Query<AirportQuery>, QueryRejection>,
) -> Result<Json<serde_json::Value>, ApiError> {
    let Query(q) = query.map_err(|_| ApiError::Invalid("query parameters are invalid"))?;
    let bbox = match &q.bbox {
        Some(value) => Some(parse_bbox(value).map_err(ApiError::Invalid)?),
        None => None,
    };
    let airports = state.airports.list(bbox).await.map_err(|_| ApiError::Database)?;
    let returned = airports.len();
    Ok(Json(serde_json::json!({
        "airports": airports,
        "pagination": {"returned": returned}
    })))
}

async fn get_airport(
    State(state): State<AppState>,
    Path(code): Path<String>,
) -> Result<Json<serde_json::Value>, ApiError> {
    match state.airports.get(&code).await.map_err(|_| ApiError::Database)? {
        Some(airport) => Ok(Json(serde_json::json!({ "airport": airport }))),
        None => Err(ApiError::NotFound("airport not found")),
    }
}

fn is_unique_violation(error: &sqlx::Error) -> bool {
    error
        .as_database_error()
        .and_then(|e| e.code())
        .is_some_and(|code| code == "23505")
}

pub fn parse_database_url(value: &str) -> Result<(), &'static str> {
    if value.starts_with("postgres://") || value.starts_with("postgresql://") {
        Ok(())
    } else {
        Err("DATABASE_URL must be a PostgreSQL URL")
    }
}

pub fn validate_listen_addr(value: &str) -> Result<std::net::SocketAddr, std::net::AddrParseError> {
    std::net::SocketAddr::from_str(value)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn flight() -> Flight {
        Flight {
            flight_id: "flight-1".into(),
            flight: "DL1234".into(),
            source: "MSP".into(),
            destination: "ORD".into(),
            scheduled_departure: "2026-10-06T08:00:00Z".parse().unwrap(),
            actual_departure: None,
            scheduled_arrival: "2026-10-06T09:30:00Z".parse().unwrap(),
            actual_arrival: None,
            aircraft: Some("N100CA".into()),
        }
    }

    #[test]
    fn validates_required_fields_and_schedule_order() {
        let mut value = flight();
        assert!(value.validate().is_ok());
        value.flight_id.clear();
        assert_eq!(value.validate(), Err("flight_id must not be empty"));
        let mut value = flight();
        value.scheduled_arrival = value.scheduled_departure;
        assert_eq!(
            value.validate(),
            Err("scheduled_arrival must be after scheduled_departure")
        );
        let mut value = flight();
        value.actual_departure = Some("2026-10-06T10:00:00Z".parse().unwrap());
        value.actual_arrival = Some("2026-10-06T09:00:00Z".parse().unwrap());
        assert_eq!(
            value.validate(),
            Err("actual_arrival must not be before actual_departure")
        );
    }

    fn report() -> Report {
        Report {
            report_id: "run-1-N100CA-000006".into(),
            run_id: "run-1".into(),
            tick_id: 6,
            aircraft_id: "N100CA".into(),
            flight_id: "CNA100".into(),
            simulated_at: "2026-10-06T14:06:00Z".parse().unwrap(),
            latitude: 43.45,
            longitude: -90.55,
            altitude_m: 10668.0,
            ground_speed_mps: 199.0,
            heading_deg: 127.0,
            status: "flying".into(),
            received_at: None,
        }
    }

    #[test]
    fn validates_report_fields() {
        let mut value = report();
        assert!(value.validate().is_ok());

        value.status = "landing".into();
        assert_eq!(
            value.validate(),
            Err("status must be parked, flying, or turnaround")
        );

        let mut value = report();
        value.heading_deg = 360.0;
        assert_eq!(value.validate(), Err("heading_deg must be in [0, 360)"));

        let mut value = report();
        value.report_id.clear();
        assert_eq!(value.validate(), Err("report_id must not be empty"));

        let mut value = report();
        value.latitude = 99.0;
        assert_eq!(value.validate(), Err("latitude must be between -90 and 90"));
    }

    #[test]
    fn same_content_ignores_received_at() {
        let a = report();
        let mut b = report();
        b.received_at = Some("2026-10-06T19:00:00Z".parse().unwrap());
        assert!(a.same_content(&b));
        b.altitude_m = 10000.0;
        assert!(!a.same_content(&b));
    }

    #[test]
    fn validates_search_pagination_and_ranges() {
        assert_eq!(FlightSearch::default().validate().unwrap(), (100, 0));
        assert!(FlightSearch {
            limit: Some(501),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(FlightSearch {
            offset: Some(-1),
            ..Default::default()
        }
        .validate()
        .is_err());
        assert!(FlightSearch {
            departure_from: Some("2026-10-07T00:00:00Z".parse().unwrap()),
            departure_to: Some("2026-10-06T00:00:00Z".parse().unwrap()),
            ..Default::default()
        }
        .validate()
        .is_err());
    }

    #[test]
    fn parses_and_validates_bbox() {
        let bb = parse_bbox("-104,36,-80,49").unwrap();
        assert_eq!((bb.min_lon, bb.min_lat, bb.max_lon, bb.max_lat), (-104.0, 36.0, -80.0, 49.0));
        assert!(parse_bbox("1,2,3").is_err()); // too few
        assert!(parse_bbox("a,2,3,4").is_err()); // not numbers
        assert!(parse_bbox("10,2,-10,4").is_err()); // min_lon > max_lon
        assert!(parse_bbox("-200,2,10,4").is_err()); // longitude out of range
    }

    #[test]
    fn validates_airport_fields() {
        let good = Airport {
            code: "MSP".into(),
            name: "Minneapolis–Saint Paul".into(),
            latitude: 44.8848,
            longitude: -93.2223,
            elevation_m: None,
            icao: None,
            country: None,
        };
        assert!(good.validate().is_ok());

        let mut bad = good.clone();
        bad.code = String::new();
        assert_eq!(bad.validate(), Err("code must be between 1 and 8 characters"));

        let mut bad = good.clone();
        bad.latitude = 91.0;
        assert_eq!(bad.validate(), Err("latitude must be between -90 and 90"));
    }
}
