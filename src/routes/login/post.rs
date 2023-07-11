use std::sync::Arc;
use axum::extract::State;
use axum::response::{IntoResponse, Response};
use axum::{Form, Extension};
use hyper::{StatusCode, header::{LOCATION, SET_COOKIE}};
use secrecy::{Secret, ExposeSecret};
use crate::authentication::{validate_credentials, Credentials, AuthError};
use crate::routes::error_chain_fmt;
use sqlx::PgPool;
use axum_extra::extract::cookie::{Cookie, CookieJar};

#[derive(serde::Deserialize)]
pub struct FormData {
    username: String,
    password: Secret<String>,
}
#[derive(thiserror::Error)]
pub enum LoginError {
    #[error("Authentication failed")]
    AuthError(#[source] anyhow::Error),
    #[error("Something went wrong")]
    UnexpectedError(#[from] anyhow::Error),
}

impl std::fmt::Debug for LoginError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

// impl IntoResponse for LoginError {
//     fn into_response(self) -> Response {
//          match self {
//             LoginError::AuthError(e) => {
//                 tracing::error!("\nAuthentication failed: {:?}", e);    
//                 (StatusCode::UNAUTHORIZED, "Authentication failed")
//                     .into_response()
//             },
//             LoginError::UnexpectedError(e) => {
//                 tracing::error!("\nUnexpected error: {:?}", e);
//                 (StatusCode::INTERNAL_SERVER_ERROR, "Something went wrong.")
//                     .into_response()
//             }
//          }
//     }
// }
#[tracing::instrument(
    skip(form, pool),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn login(
    State(pool): State<Arc<PgPool>>,
    Form(form): Form<FormData>,
) -> impl IntoResponse {
    let credentials = Credentials {
        username: form.username,
        password: form.password,
    };
    tracing::Span::current()
        .record("username", &tracing::field::display(&credentials.username));
    match validate_credentials(credentials, &pool).await {
        Ok(user_id) => {
            tracing::Span::current()
                .record("user_id", &tracing::field::display(&user_id));
            (
                StatusCode::SEE_OTHER,
                [(LOCATION, "/")]
            ).into_response()
        },
        Err(e) => {
            let e = match e {
                AuthError::InvalidCredentials(_) => LoginError::AuthError(e.into()),
                AuthError::UnexpectedError(_) => LoginError::UnexpectedError(e.into()),
            };

            tracing::error!("\nServer error: {e:?}");

            (
                StatusCode::SEE_OTHER,
                [(LOCATION, "/login"),],
                CookieJar::new()
                    .add(Cookie::new("_flash", e.to_string())),
                
            ).into_response()
        }
    }
}

