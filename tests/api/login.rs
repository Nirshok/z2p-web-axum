use crate::helpers::{spawn_app, assert_is_redirected_to};

#[tokio::test]
async fn an_error_flash_message_is_set_on_failure() {
    // Arrange 
    let app = spawn_app().await;

    // Act
    let login_body = serde_json::json!({
        "username": "random-username",
        "password": "random-password",
    });

    let response = app.post_login(&login_body).await;

    // Assert
    assert_is_redirected_to(&response, "/login");

    let flash_cookie = response
        .cookies()
        .find(|c| c.name() == "_flash")
        .unwrap();
    assert_eq!(flash_cookie.value(), "Authentication%20failed");

    // Act 2: Follow the redirect
    let html_page = app.get_login_html().await;
    assert!(html_page.contains(r#"<p><i>Authentication failed</i></p>"#));

    // Act 3: Reload the login page
    let html_page = app.get_login_html().await;
    assert!(!html_page.contains(r#"<p><i>Authentication failed</i></p>"#));
}