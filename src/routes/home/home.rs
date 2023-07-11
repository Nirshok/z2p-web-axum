use axum::response::{IntoResponse, Response};
use hyper::StatusCode;

pub async fn home() -> impl IntoResponse {
    Response::builder()
        .status(StatusCode::OK)
        .header("Content-Type", "text/html")
        .body(include_str!("home.html").to_owned())
        .unwrap()
}