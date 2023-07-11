use std::sync::Arc;
use axum::{extract::{Query, State}, Json, response::IntoResponse};
use hyper::StatusCode;
use sqlx::PgPool;
use uuid::Uuid;
use anyhow::Context;
use crate::routes::error_chain_fmt;

#[derive(thiserror::Error)]
pub enum ConfirmError {
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error)
}

impl std::fmt::Debug for ConfirmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl IntoResponse for ConfirmError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match self {
            Self::UnexpectedError(e) => {
                tracing::error!("\nServer error: {:?}", e);
                (StatusCode::INTERNAL_SERVER_ERROR, "Unexpected internal server error.".to_owned())
            } 
        };
            

        let body = Json(serde_json::json!({
            "error": error_message
        }));

        (status, body).into_response()
    }
}

#[derive(serde::Deserialize)]
pub struct Parameters {
    subscription_token: String,
}

#[tracing::instrument(
    name = "Confirm a pending subscriber",
    skip(parameters, pool)
)]
pub async fn confirm(
    Query(parameters): Query<Parameters>,
    State(pool): State<Arc<PgPool>>,
) -> Result<impl IntoResponse, ConfirmError> {
    let subscription_token = parse_subscription_token(&parameters.subscription_token);
    let id = get_subscriber_id_from_token(&pool, &subscription_token)
        .await
        .context("Failed to get subscriber id from database.")?;

    match id {
        // Non-existing token!
        None => Ok(StatusCode::UNAUTHORIZED),
        Some(subscriber_id) => {
            let pending = subscriber_is_pending(&pool, subscriber_id)
                .await
                .context("Failed to check subscriber's status.")?;

            if pending {
                confirm_subscriber(&pool, subscriber_id)
                    .await
                    .context("Failed to change subscriber's status.")?;
                Ok(StatusCode::OK)
            } else {
                Ok(StatusCode::BAD_REQUEST)
            }
        }
    }
}

#[tracing::instrument(
    name = "Check if subscriber status is 'pending'",
    skip(subscriber_id, pool)
)]
pub async fn subscriber_is_pending(
    pool: &PgPool,
    subscriber_id: Uuid,
) -> Result<bool, sqlx::Error> {
    let result = sqlx::query!(
        r#"
            SELECT status FROM subscriptions
                WHERE id = $1
        "#,
        subscriber_id
    )
    .fetch_one(pool)
    .await?;

    if result.status == "pending_confirmation" {
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tracing::instrument(
    name = "Mark subscriber as confirmed",
    skip(subscriber_id, pool)
)]
pub async fn confirm_subscriber(
    pool: &PgPool,
    subscriber_id: Uuid
) -> Result<(), sqlx::Error> {    
    sqlx::query!(
        r#"
        UPDATE subscriptions SET status = 'confirmed'
            WHERE id = $1
        "#,
        subscriber_id,
    )
    .execute(pool)
    .await?;

    Ok(())
}

#[tracing::instrument(
    name = "Get subscriber_id from token",
    skip(pool, subscription_token)
)]
pub async fn get_subscriber_id_from_token(
    pool: &PgPool,
    subscription_token: &str,
) -> Result<Option<Uuid>, sqlx::Error> {
    let result = sqlx::query!(
        r#"SELECT subscriber_id FROM subscription_tokens
            WHERE subscription_token = $1"#,
        subscription_token,
    )
    .fetch_optional(pool)
    .await?;

    Ok(result.map(|r| r.subscriber_id))
}

fn parse_subscription_token(subscription_token: &str) -> String {
    subscription_token.chars().filter(|ch| ch.is_alphanumeric()).collect()
}

#[cfg(test)]
mod test {
    use crate::routes::subscriptions_confirm::parse_subscription_token;

    #[test]
    fn non_alphanumeric_tokens_are_parsed() {
        let token = "DROP TABLE subscribers;)";
        let parsed = parse_subscription_token(token);

        assert_eq!("DROPTABLEsubscribers".to_owned(), parsed);
    }
}