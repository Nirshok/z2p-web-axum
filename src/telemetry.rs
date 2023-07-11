use tower_request_id::RequestId;
use tracing::{Subscriber, Span};
use tracing::subscriber::set_global_default;
use tracing_bunyan_formatter::{BunyanFormattingLayer, JsonStorageLayer};
use tracing_log::LogTracer;
use tracing_subscriber::{layer::SubscriberExt, EnvFilter, Registry};
use tracing_subscriber::fmt::MakeWriter;
use hyper::{Body, http::Request};
use tokio::task::JoinHandle;

pub fn get_subscriber<Sink>(
    name: String,
    env_filter: String,
    sink: Sink,
) -> impl Subscriber + Send + Sync
where

Sink: for<'a> MakeWriter<'a> + Send + Sync + 'static,

{
    let env_filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(env_filter));
    let formatting_layer = BunyanFormattingLayer::new(name, sink);

    Registry::default()
        .with(env_filter)
        .with(JsonStorageLayer)
        .with(formatting_layer)
        .with(tracing_subscriber::fmt::layer())
}

pub fn init_subscriber(subscriber: impl Subscriber + Send + Sync) {
    LogTracer::init().expect("Failed to set logger.");
    set_global_default(subscriber).expect("Failed to set subscriber.");
}

pub fn request_id(request: &Request<Body>) -> Span {
    let request_id = request.extensions()
        .get::<RequestId>()
        .map(ToString::to_string)
        .unwrap_or_else(|| "unknown".into());
    tracing::error_span!(
        "request",
        id = %request_id,
        method = %request.method(),
        uri = %request.uri(),
    )
}

pub fn spawn_blocking_with_tracing<F, R>(f: F) -> JoinHandle<R>
where
    F: FnOnce() -> R + Send + 'static,
    R: Send + 'static,
{
    let current_spawn = tracing::Span::current();
    tokio::task::spawn_blocking(move || { current_spawn.in_scope(f) })
}