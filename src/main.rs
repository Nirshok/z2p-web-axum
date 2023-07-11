use myweb::startup::build;
use myweb::telemetry::{get_subscriber, init_subscriber};
use myweb::utils::shutdown_signal;
use myweb::configuration::get_configuration;

#[tokio::main]
async fn main() {
    let subscriber = get_subscriber("my-web".into(), "info".into(), std::io::stdout);
    init_subscriber(subscriber);
    
    let configuration = get_configuration().expect("Failed to read configuration");

    let server = build(configuration).await;

    server
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}
