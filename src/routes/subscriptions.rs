use anyhow::Context;
use axum::Extension;
use axum_macros::debug_handler;
use serde::Deserialize;
use axum::{
    http::StatusCode,
    Form,
    extract::State, response::IntoResponse,
    Json
};
use chrono::Utc;
use uuid::Uuid;
use sqlx::{PgPool, Postgres, Transaction};
use std::sync::Arc;
use crate::domain::{NewSubscriber, SubscriberName, SubscriberEmail};
use crate::email_client::EmailClient;
use rand::distributions::Alphanumeric;
use rand::{thread_rng, Rng};
// Return on better PC
// use tera::{Tera, Context as TeraContext};
// use lazy_static::lazy_static;

#[derive(Deserialize)]
pub struct FormData {
    email: String,
    name: String,
}

impl TryFrom<FormData> for NewSubscriber {
    type Error = String;
    
    fn try_from(value: FormData) -> Result<Self, Self::Error> {
        
        let name = SubscriberName::parse(value.name)?;
        let email = SubscriberEmail::parse(value.email)?;
        
        Ok(Self {email, name})
    }
}

#[derive(thiserror::Error)]
pub enum SubscribeError {
    #[error("{0}")]
    ValidationError(String),
    #[error(transparent)]
    UnexpectedError(#[from] anyhow::Error)
}

impl std::fmt::Debug for SubscribeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl IntoResponse for SubscribeError {
    fn into_response(self) -> axum::response::Response {
        let (status, error_message) = match self {
            // TODO: make tracing in the middleware (how?)
            Self::ValidationError(e) => {
                tracing::error!("\nParsing error: {}", e);
                (StatusCode::BAD_REQUEST, e)
            },
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


#[tracing::instrument(
    name = "Adding a new subscriber",
    skip(form, pool, email_client, base_url),
    fields(
        subscriber_email = %form.email,
        subscriber_name = %form.name
    )
)]
#[debug_handler]
pub async fn subscribe(
    State(pool): State<Arc<PgPool>>,
    Extension(email_client): Extension<Arc<EmailClient>>,
    Extension(base_url): Extension<String>,
    Form(form): Form<FormData>,
) -> Result<impl IntoResponse, SubscribeError> {
    let new_subscriber = form.try_into().map_err(SubscribeError::ValidationError)?;

    let mut transaction = pool
        .begin()
        .await
        .context("Failed to acquire a Postgres connection from the pool.")?;

    let check_subscriber = subscriber_exists(&mut transaction, &new_subscriber)
        .await 
        .context("Failed to check if subscriber is already in the database.")?;

    let subscriber_id = match check_subscriber {
        Some(id) => id,
        None => insert_subscriber(&mut transaction, &new_subscriber)
            .await
            .context("Failed to insert new subscriber in the database.")?,
        };

    let subscription_token = match check_subscriber {
        Some(id) => retrieve_token_from_database(id, &mut transaction)
            .await
            .context("Failed to retrieve subscriber token from the database.")?,
        None => {
                let subscription_token = generate_subscription_token();
                store_token(&mut transaction, subscriber_id, &subscription_token)
                .await
                    .context("Failed to store the confirmation token for a new subscriber.")?;
                subscription_token
        }
    };

    transaction.commit()
        .await
        .context("Failed to commit SQL transaction to store a new subscriber.")?;

    send_confirmation_email(
        &email_client,
        new_subscriber,
        &base_url,
        &subscription_token,
    )
    .await
    .context("Failed to send a confirmation email.")?;


    Ok(StatusCode::OK)
}

fn generate_subscription_token() -> String {
    let mut rng = thread_rng();
    std::iter::repeat_with(||  rng.sample(Alphanumeric))
        .map(char::from)
        .take(25)
        .collect()
}

#[tracing::instrument(
    name = "Retrieve existing subscriber's token",
    skip(transaction, id)
)]
pub async fn retrieve_token_from_database(
    id: Uuid,
    transaction: &mut Transaction<'_, Postgres>,
) -> Result<String, sqlx::Error> {
    let token = sqlx::query!(
        r#"
            SELECT subscription_token FROM subscription_tokens
                WHERE subscriber_id = $1
        "#,
        id
    )
    .fetch_one(&mut **transaction)
    .await?;

    Ok(token.subscription_token)
}

#[tracing::instrument(
    name = "Check if subscriber exists in database",
    skip(transaction, new_subscriber)
)]
pub async fn subscriber_exists(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Option<Uuid>, sqlx::Error> {
    let existing = sqlx::query!(
        r#"
            SELECT id FROM subscriptions
                WHERE email = $1
        "#,
        new_subscriber.email.as_ref()
    )
    .fetch_optional(&mut **transaction)
    .await?;

    Ok(existing.map(|r| r.id))
}

#[tracing::instrument(
    name = "Send a confirmation email to a new subscriber",
    skip(email_client, new_subscriber)
)]
pub async fn send_confirmation_email(
    email_client: &EmailClient,
    new_subscriber: NewSubscriber,
    base_url: &str,
    subscription_token: &str,
) -> Result<(), reqwest::Error> {
    let confirmation_link = format!(
        "{}/subscriptions/confirm?subscription_token={}",
        base_url,
        subscription_token
    );
    let plain_body = format!(
        "Welcome to our newsletter!\nVisit {} to confirm your subscription.",
        confirmation_link
    );
    let html_body = format!(
        "Welcome to our newsletter!<br />\
        Click <a href=\"{}\">here</a> to confirm your subscription.",
        confirmation_link
    );
    // Return it after getting a better PC
    // let html_body = make_template(&confirmation_link)
    //     .expect("Failed to render template");

    email_client
        .send_email(
            &new_subscriber.email,
            "Welcome!",
            &html_body, 
            &plain_body, 
        )
        .await
}

#[tracing::instrument(
    name = "Saving new subscriber details in the database.",
    skip(new_subscriber, transaction)
)]
pub async fn insert_subscriber(
    transaction: &mut Transaction<'_, Postgres>,
    new_subscriber: &NewSubscriber,
) -> Result<Uuid, sqlx::Error> {
    let subscriber_id = Uuid::new_v4();
    
    sqlx::query!(
        r#"
            INSERT INTO subscriptions (id, email, name, subscribed_at, status)
                VALUES ($1, $2, $3, $4, 'pending_confirmation')
        "#,
        subscriber_id,
        new_subscriber.email.as_ref(),
        new_subscriber.name.as_ref(),
        Utc::now()
    )
    .execute(&mut **transaction)
    .await?;
    
    Ok(subscriber_id)
}

#[tracing::instrument(
    name = "Store subscription token in the database",
    skip(subscription_token, transaction)
)]
pub async fn store_token(
    transaction: &mut Transaction<'_, Postgres>,
    subscriber_id: Uuid,
    subscription_token: &str
) -> Result<(), StoreTokenError> {
    sqlx::query!(
        r#"INSERT INTO subscription_tokens (subscription_token, subscriber_id)
            VALUES ($1, $2)"#,
        subscription_token,
        subscriber_id
    )
    .execute(&mut **transaction)
    .await
    .map_err(StoreTokenError)?;

    Ok(())
}


pub struct StoreTokenError(sqlx::Error);

impl std::fmt::Debug for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        error_chain_fmt(self, f)
    }
}

impl std::error::Error for StoreTokenError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        Some(&self.0)
    }
}

impl std::fmt::Display for StoreTokenError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "A database error was encountered while \
            trying to store a subscription token"
        )
    }
}

impl IntoResponse for StoreTokenError {
    fn into_response(self) -> axum::response::Response {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            format!(
                "A database error was encountered while \
                trying to store a subscription token"
            )
        ).into_response()
    }
}

// Return it after getting a better PC
// lazy_static!{
//     pub static ref TEMPLATES: Tera = {
//         let tera = Tera::new("templates/**/*")
//             .expect("Failed to parse templates");
//         tera
//     };
// }

// fn make_template(confirmation_link: &str) -> Result<String, tera::Error> {
//     let tera = &TEMPLATES;
//     let mut context = TeraContext::new();
//     context.insert("title", "Welcome to our newsletter!");
//     context.insert("link", confirmation_link);

//     let rendered = tera.render("confirmation_email.html", &context)?;
//     Ok(rendered)
// }

pub fn error_chain_fmt(
    e: &impl std::error::Error,
    f: &mut std::fmt::Formatter<'_>,
) -> std::fmt::Result {
    writeln!(f, "{}\n", e)?;
    let mut current = e.source();
    while let Some(cause) = current {
        writeln!(f, "Caused by:\n\t{}", cause)?;
        current = cause.source();
    }
    Ok(())
}


