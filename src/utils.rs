use axum::{
    response::IntoResponse,
    http::StatusCode,
};
use tokio::signal;

pub async fn handler_404() -> impl IntoResponse {
    (StatusCode::NOT_FOUND, "You've ventured beyond the horison.")
}


pub async fn shutdown_signal() {
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

    tokio::select!{
        _ = ctrl_c => {},
        _ = terminate => {},
    }
    
    println!("\nSignal received, starting graceful shutdown");
}