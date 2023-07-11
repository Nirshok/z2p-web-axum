use std::{sync::Arc};

use crate::{
    routes::{home, blog, reviews, subscribe, health_check, confirm, publish_newsletter, login_form, login},
    email_client::EmailClient
};
use axum::{
    routing::{get, post, IntoMakeService},
    Router, Extension,
};
use secrecy::Secret;
use sqlx::PgPool;
use std::net::TcpListener;
use crate::utils::handler_404;
use crate::telemetry::request_id;
use tower_http::trace::TraceLayer;
use tower_request_id::RequestIdLayer;
use hyper::{Body, http::Request, server::conn::AddrIncoming};
use crate::configuration::{Settings, DatabaseSettings};
use sqlx::postgres::PgPoolOptions;


pub async fn build(configuration: Settings) -> axum::Server<AddrIncoming, IntoMakeService<Router>> {
    let connection_pool = get_connection_pool(&configuration.database);
    
    let sender_email = configuration.email_client.sender()
        .expect("Invalid sender email address.");
    let base_url = reqwest::Url::parse(&configuration.email_client.base_url)
        .expect("Failed to parse URL");
    let timeout = configuration.email_client.timeout();

    let email_client = EmailClient::new(
        base_url,
        sender_email,
        configuration.email_client.authorization_token,
        timeout
    );

    let address = format!(
        "{}:{}",
        configuration.application.host,
        configuration.application.port
    );
    let listener = TcpListener::bind(address)
        .expect("Failed to bind a port");

   run(
    connection_pool,
        email_client,
        listener,
        configuration.application.base_url,
        configuration.application.hmac_secret
    )
}

pub fn get_connection_pool(
    configuration: &DatabaseSettings,
) -> PgPool {
    PgPoolOptions::new()
        .acquire_timeout(std::time::Duration::from_secs(2))
        .connect_lazy_with(configuration.with_db())
}
pub struct ApplicationBaseUrl(pub String);

fn run(
    db_pool: PgPool,
    email_client: EmailClient,
    listener: TcpListener,
    base_url: String,
    hmac_secret: Secret<String>,
) -> axum::Server<AddrIncoming, IntoMakeService<Router>> {
    let db_pool = Arc::new(db_pool);
    let email_client = Arc::new(email_client);

    let router = Router::new()
            .route("/", get(home))
            .route("/blog", get(blog))
            .route("/reviews", get(reviews))
            .route("/health_check", get(health_check))
            .route("/subscriptions", post(subscribe))
            .route("/subscriptions/confirm", get(confirm))
            .route("/newsletters", post(publish_newsletter))
            .route("/login", get(login_form).post(login))
            .fallback(handler_404)
            .layer(TraceLayer::new_for_http()
                .make_span_with(|request: &Request<Body>| {
                    request_id(request)
                })
            )
            .layer(RequestIdLayer)
            .layer(Extension(Arc::clone(&email_client)))
            .layer(Extension(base_url.clone()))
            .layer(Extension(HmacSecret(hmac_secret.clone())))
            .with_state(Arc::clone(&db_pool));

    axum::Server::from_tcp(listener)
        .expect("Failed to bind a port.")
        .serve(router.into_make_service()) 
}

#[derive(Clone)]
pub struct HmacSecret(pub Secret<String>);