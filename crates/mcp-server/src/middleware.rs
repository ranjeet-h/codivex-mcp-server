use std::time::Instant;

use axum::{
    body::Body,
    http::{Request, header::HeaderName},
    middleware::Next,
    response::Response,
};
use tracing::info;
use uuid::Uuid;

const X_CORRELATION_ID: &str = "x-correlation-id";

pub async fn trace_with_correlation(mut req: Request<Body>, next: Next) -> Response {
    let started = Instant::now();
    let correlation = Uuid::new_v4().to_string();
    if let Ok(name) = HeaderName::from_lowercase(X_CORRELATION_ID.as_bytes()) {
        if let Ok(value) = correlation.parse() {
            req.headers_mut().insert(name.clone(), value);
        }
    }

    let method = req.method().clone();
    let path = req.uri().path().to_string();
    let mut res = next.run(req).await;
    let elapsed_ms = started.elapsed().as_millis();

    if let Ok(name) = HeaderName::from_lowercase(X_CORRELATION_ID.as_bytes()) {
        if let Ok(value) = correlation.parse() {
            res.headers_mut().insert(name, value);
        }
    }

    info!(
        correlation_id = correlation,
        method = %method,
        path = %path,
        status = res.status().as_u16(),
        elapsed_ms = elapsed_ms as u64,
        "request_complete"
    );
    res
}
