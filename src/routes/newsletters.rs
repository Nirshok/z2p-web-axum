use std::sync::Arc;
use base64::Engine;
use secrecy::Secret;
use anyhow::Context;
use axum::{
    Extension,
    Json,
    headers::{HeaderMap},
    response::{IntoResponse},
    extract::State,
    http::{StatusCode, HeaderValue, header}
};
use sqlx::PgPool;
use crate::{email_client::EmailClient, domain::SubscriberEmail};
use super::error_chain_fmt;
use crate::authentication::{AuthError, validate_credentials, Credentials};


#[derive(thiserror::Error)]
pub enum PublishError {
    #[error("Authentication failed.")]
    AuthError(#[source] anyhow::Error),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error)
}

impl std::fmt::Debug for PublishError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl IntoResponse for PublishError {
    fn into_response(self) -> axum::response::Response {
        match self {
            Self::UnexpectedError(e) => {
                tracing::error!("\nServer error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Unexpected internal server error.").into_response()
            },
            Self::AuthError(e) => {
                tracing::error!("\nAuthorization error: {:?}", e);
                
                let header_value = HeaderValue::from_str(r#"Basic realm="publish""#).unwrap();
                (
                    StatusCode::UNAUTHORIZED,
                    [(header::WWW_AUTHENTICATE, header_value)],
                    "Authorization failed."
                )
                .into_response()
            }
        }
    }    
}

#[derive(serde::Deserialize)]
pub struct BodyData {
    title: String,
    content: Content
}
#[derive(serde::Deserialize)]
pub struct Content {
    html: String,
    text: String
}

#[tracing::instrument(
    name = "Publish a newsletter issue",
    skip(body, pool, email_client, headers),
    fields(username=tracing::field::Empty, user_id=tracing::field::Empty)
)]
pub async fn publish_newsletter(
    State(pool): State<Arc<PgPool>>,
    Extension(email_client): Extension<Arc<EmailClient>>,
    headers: HeaderMap,
    Json(body): Json<BodyData>,
) -> Result<impl IntoResponse, PublishError> {
    let credentials = basic_authentication(headers)
        .map_err(PublishError::AuthError)?;
    tracing::Span::current().record(
        "username",
        &tracing::field::display(&credentials.username)
    );
    let user_id = validate_credentials(credentials, &pool)
        .await
        // We match on `AuthError`'s variants, but we pass the **whole** error
        // into the constructors for `PublishError` variants. This ensures that
        // the context of the top-level wrapper is preserved when the error is
        // logged by our middleware  
        .map_err(|e| match e {
            AuthError::InvalidCredentials(_) => PublishError::AuthError(e.into()),
            AuthError::UnexpectedError(_) => PublishError::UnexpectedError(e.into())
        })?;
    tracing::Span::current().record(
        "user_id",
        &tracing::field::display(&user_id)
    );

    let subscribers = get_confirmed_subscribers(&pool)
        .await
        .context("Failed to get confirmed subscribers from the database")?;
    
    for subscriber in subscribers {
        match subscriber {
            Ok(subscriber) => {
                email_client
                    .send_email(
                        &subscriber.email,
                        &body.title,
                        &body.content.html,
                        &body.content.text
                    )
                    .await
                    .with_context(|| {
                        format!(
                            "Failed to send newsletter issue to {}",
                            subscriber.email
                        )
                    })?;
            },
            Err(error) => {
                tracing::warn!(
                    // We record the error chain as a structual field
                    // on the log record
                    error.cause_chain = ?error,
                    // Using '\' to split a long string literal over
                    // two lines, without creating a '\n' character.
                    "Skipping a confirmed subscriber. \
                    Their stored contact details are invalid.",
                );
            } 
        }
    }
    Ok(StatusCode::OK)
}


fn basic_authentication(
    headers: HeaderMap,
) -> Result<Credentials, anyhow::Error> {
    let header_value = headers
        .get("Authorization")
        .context("The 'Authorization' header was missing.")?
        .to_str()
        .context("The 'Authorization' header was not a valid UTF-8 string.")?;
    let base64encoded_segment = header_value
        .strip_prefix("Basic ")
        .context("The 'Authorization' scheme was not 'Basic'.")?;
    let decoded_bytes = base64::engine::general_purpose::STANDARD
        .decode(base64encoded_segment)
        .context("Failed to base64-decode 'Basic' credentials.")?;
    let decoded_credentials = String::from_utf8(decoded_bytes)
        .context("The decoded credentials string is not valid UTF-8.")?;

    // Split into two segments using ":" as delimiter
    let mut credentials = decoded_credentials.splitn(2, ':');
    let username = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A username must be provided in 'Basic' auth."))?
        .to_string();
    let password = credentials
        .next()
        .ok_or_else(|| anyhow::anyhow!("A password must be provided in 'Basic' auth."))?
        .to_string();

    Ok(Credentials {
        username,
        password: Secret::new(password) })
}
struct ConfirmedSubscriber {
    email: SubscriberEmail,
}

#[tracing::instrument(
    name = "Get confirmed subscribers",
    skip(pool)
)]
async fn get_confirmed_subscribers(
    pool: &PgPool,
    // We are returning a `Vec` of `Result`s in the happy case.
    // This allows the caller to bubble up errors due to network issues or other
    // transient failures using the `?` operator, while the compiler 
    // forces them to handle the subtler mapping error.
    // See http://sled.rs/errors.html for a deep-dive about this technique.
) -> Result<Vec<Result<ConfirmedSubscriber, anyhow::Error>>, anyhow::Error> {  
    let confirmed_subscribers = sqlx::query!(
        r#"
            SELECT email FROM subscriptions
                WHERE status = 'confirmed'
        "#,
    )
    .fetch_all(pool)
    .await?
    .into_iter()
    .map(|r| match SubscriberEmail::parse(r.email) {
        Ok(email) => Ok(ConfirmedSubscriber { email }),
        Err(error) => Err(anyhow::anyhow!(error)),
    })
    .collect();

    Ok(confirmed_subscribers)
}
