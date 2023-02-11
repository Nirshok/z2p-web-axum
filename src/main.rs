use axum::{
    async_trait,
    response::{Html, IntoResponse},
    routing::get,
    Router,
    extract::{Form, FromRef, FromRequestParts, State},
    http::{StatusCode, request::Parts},
};
use serde::Deserialize;
use std::net::SocketAddr;
use tokio::signal;
use tracing_subscriber::{
    layer::SubscriberExt,
    util::SubscriberInitExt,
};
use sqlx::postgres::{PgPool, PgPoolOptions};


#[tokio::main]
async fn main() {

    tracing_subscriber::registry()
        .with(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "myweb=debug".into()),
        )
        .with(tracing_subscriber::fmt::layer())
        .init();
    
    let db_connection = std::env::var("DATABASE_URL")
        .unwrap_or_else(|_| "postgres://Niro:P0stDelkp9kep3@localhost".to_string());
    
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&db_connection)
        .await
        .expect("Can't connect to database.");

    let app = Router::new()
            .route("/", get(mainpage).post(accept_form))
            .route("/blog", get(blog).post(accept_form))
            .route("/reviews", get(reviewes).post(accept_form))
            .route("/database", get(using_connection_pool_extractor).post(using_connection_extractor))
            .with_state(pool);
    
    let app = app.fallback(handler_404);

    let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
    tracing::debug!("Listening on {}", addr);

    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn using_connection_pool_extractor(
    State(pool): State<PgPool>,
) -> Result<String, (StatusCode, String)> {
    sqlx::query_scalar("select 'Hello from PG'")
        .fetch_one(&pool)
        .await
        .map_err(internal_error)
}

struct DatabaseConnection(sqlx::pool::PoolConnection<sqlx::Postgres>);

#[async_trait]
impl<S> FromRequestParts<S> for DatabaseConnection
where
    PgPool: FromRef<S>,
    S: Send + Sync,
{
    type Rejection = (StatusCode, String);

    async fn from_request_parts(_parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let pool = PgPool::from_ref(state);

        let conn = pool.acquire().await.map_err(internal_error)?;
        
        Ok(Self(conn))
    }
}

async fn using_connection_extractor(
    DatabaseConnection(conn): DatabaseConnection,
) -> Result<String, (StatusCode, String)> {
    let mut conn = conn;
    sqlx::query_scalar("select 'Hello from PG'")
        .fetch_one(&mut conn)
        .await
        .map_err(internal_error)
}

fn internal_error<E>(err: E) -> (StatusCode, String)
where 
    E: std::error::Error,
{
    (StatusCode::INTERNAL_SERVER_ERROR, err.to_string())
}
async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "You've ventured beyond the horison")
}
#[derive(Debug, PartialEq, Clone)]
struct User {
    name: String,
    id: i32,
    email: String,
}

/*impl User {
    fn new(&self) -> Self {
        User {
            name: String::from("inputname"),
            id: _last_id + 1,
            email: String::from("inputemail"),
        }
    }
}*/
#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct Input {
    name: String,
    email: String,
}

async fn accept_form(Form(input): Form<Input>) {
    dbg!(&input);
}

async fn mainpage() -> Html<&'static str> {
    Html(
        r#"
        <!doctype html>
        <html>
            <h1>Main page</h1>
            <head>
            <form action="/" method="post">
            <label for="name">
                Enter your name:
                <input type="text" name="name">
            </label>

            <label>
                Enter your email:
                <input type="text" name="email">
            </label>

            <input type="submit" value="Subscribe NOW">
        </form>
            </head>
            <body>
            Uncool manners bruh
            </body>
        </html>
        "#,
    )
}

async fn blog() -> Html<&'static str> {
    Html("<h1>This will be a blog</h1>")
}

async fn reviewes() -> Html<&'static str> {
    Html("<h1>Reviewes time</h1>")
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c()
                .await
                .expect("Failed to install Ctrl + C handler");
    };
    
    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("Failed to install signal handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    
    println!("Signal received, starting graceful shutdown");
}