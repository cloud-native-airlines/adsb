use axum::{
    extract::{
        rejection::{JsonRejection, QueryRejection},
        Query, State,
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

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("invalid request: {0}")]
    Invalid(&'static str),
    #[error("flight_id already exists")]
    Conflict,
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
}
