use adsb::{app, AppState, FlightStore};
use axum::{
    body::Body,
    http::{Request, StatusCode},
    Router,
};
use http_body_util::BodyExt;
use serde_json::{json, Value};
use sqlx::{
    postgres::{PgConnectOptions, PgPoolOptions},
    PgPool,
};
use std::{
    env,
    str::FromStr,
    time::{SystemTime, UNIX_EPOCH},
};
use tower::ServiceExt;

struct TestDb {
    store: FlightStore,
    admin: PgPool,
    schema: String,
}

impl TestDb {
    async fn setup() -> Option<Self> {
        let url = match env::var("ADSB_TEST_DATABASE_URL") {
            Ok(url) => url,
            Err(_) => return None,
        };
        let admin = PgPoolOptions::new()
            .max_connections(2)
            .connect(&url)
            .await
            .expect("connect to test PostgreSQL");
        let suffix = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let schema = format!("adsb_test_{}_{}", std::process::id(), suffix);
        sqlx::query(&format!("CREATE SCHEMA {schema}"))
            .execute(&admin)
            .await
            .expect("create isolated test schema");
        let options = PgConnectOptions::from_str(&url)
            .expect("parse test database URL")
            .options([("search_path", schema.as_str())]);
        let pool = PgPoolOptions::new()
            .max_connections(10)
            .connect_with(options)
            .await
            .expect("connect to isolated test schema");
        let store = FlightStore::new(pool);
        store.migrate().await.expect("run test migrations");
        Some(Self {
            store,
            admin,
            schema,
        })
    }

    async fn cleanup(self) {
        self.store.pool().close().await;
        sqlx::query(&format!("DROP SCHEMA {} CASCADE", self.schema))
            .execute(&self.admin)
            .await
            .expect("drop isolated test schema");
        self.admin.close().await;
    }
}

#[tokio::test]
async fn flight_api_persists_updates_searches_and_health() {
    let Some(db) = TestDb::setup().await else {
        eprintln!("skipping PostgreSQL API test; set ADSB_TEST_DATABASE_URL");
        return;
    };
    let router = app(AppState {
        store: db.store.clone(),
    });

    let (status, body) = send(&router, "GET", "/healthz", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"status":"ok"}));
    let (status, body) = send(&router, "GET", "/readyz", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body, json!({"status":"ready"}));
    let (status, body) = send(&router, "GET", "/unknown", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(body["error"]["code"], "not_found");
    let (status, body) = send(&router, "DELETE", "/healthz", None).await;
    assert_eq!(status, StatusCode::METHOD_NOT_ALLOWED);
    assert_eq!(body["error"]["code"], "method_not_allowed");

    let first = sample("flight-1", "N100CA", "ORD");
    let (status, body) = send(&router, "POST", "/api/v1/flight", Some(first.clone())).await;
    assert_eq!(status, StatusCode::CREATED);
    assert_eq!(body["flight"]["flight_id"], "flight-1");

    let (status, body) = send(&router, "POST", "/api/v1/flight", Some(first.clone())).await;
    assert_eq!(status, StatusCode::CONFLICT);
    assert_eq!(body["error"]["code"], "flight_conflict");

    let mut updated = first;
    updated["actual_departure"] = json!("2026-10-06T08:03:00Z");
    updated["actual_arrival"] = json!("2026-10-06T09:25:00Z");
    let (status, body) = send(&router, "PUT", "/api/v1/flight", Some(updated)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["created"], false);
    assert_eq!(body["flight"]["actual_departure"], "2026-10-06T08:03:00Z");

    let second = sample("flight-2", "N200CA", "ORD");
    let (status, _) = send(&router, "PUT", "/api/v1/flight", Some(second)).await;
    assert_eq!(status, StatusCode::CREATED);

    let (status, body) = send(
        &router,
        "GET",
        "/api/v1/flight/search?pending_arrivals_at=ORD",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["flights"].as_array().unwrap().len(), 1);
    assert_eq!(body["flights"][0]["flight_id"], "flight-2");

    let (status, body) = send(
        &router,
        "GET",
        "/api/v1/flight/search?aircraft=N100CA&departure_from=2026-10-06T07%3A00%3A00Z&limit=1",
        None,
    )
    .await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(body["pagination"]["returned"], 1);
    assert_eq!(body["flights"][0]["flight_id"], "flight-1");

    let invalid = sample("flight-bad", "N100CA", "ORD");
    let mut invalid = invalid;
    invalid["scheduled_arrival"] = json!("2026-10-06T07:00:00Z");
    let (status, body) = send(&router, "POST", "/api/v1/flight", Some(invalid)).await;
    assert_eq!(status, StatusCode::BAD_REQUEST);
    assert_eq!(body["error"]["code"], "invalid_request");

    let persisted = db
        .store
        .search(&adsb::FlightSearch {
            aircraft: Some("N100CA".into()),
            ..Default::default()
        })
        .await
        .unwrap();
    assert_eq!(persisted.len(), 1);
    assert_eq!(
        persisted[0].actual_departure.unwrap().to_rfc3339(),
        "2026-10-06T08:03:00+00:00"
    );
    db.cleanup().await;
}

fn sample(id: &str, aircraft: &str, destination: &str) -> Value {
    json!({
        "flight_id":id,
        "flight":"DL1234",
        "source":"MSP",
        "destination":destination,
        "scheduled_departure":"2026-10-06T08:00:00Z",
        "actual_departure":null,
        "scheduled_arrival":"2026-10-06T09:30:00Z",
        "actual_arrival":null,
        "aircraft":aircraft
    })
}

async fn send(
    router: &Router,
    method: &str,
    path: &str,
    body: Option<Value>,
) -> (StatusCode, Value) {
    let builder = Request::builder().method(method).uri(path);
    let request = match body {
        Some(value) => builder
            .header("content-type", "application/json")
            .body(Body::from(value.to_string()))
            .unwrap(),
        None => builder.body(Body::empty()).unwrap(),
    };
    let response = router.clone().oneshot(request).await.unwrap();
    let status = response.status();
    let body = response.into_body().collect().await.unwrap().to_bytes();
    (status, serde_json::from_slice(&body).unwrap_or(Value::Null))
}
