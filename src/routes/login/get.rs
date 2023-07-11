use axum::response::{IntoResponse, Response};
use axum_extra::extract::cookie::{CookieJar, Cookie};
use hyper::StatusCode;


pub async fn login_form(
    jar: CookieJar,
) -> impl IntoResponse {
    let error_html = match jar.get("_flash") {
        None => "".into(),
        Some(cookie) => {
            format!("<p><i>{}</i></p>", cookie.value())
        }
    };

//     Response::builder()
//         .status(200)
//         .header("Content-Type", "text/html")
//         // Make it with tera later
//         .body(format!(
//             r#"<!DOCTYPE html>
// <html lang="en">
// <head>
//     <meta http-equiv="content-type" content="text/html; charset=utf-8">
//     <title>Login</title>
// </head>
// <body>
//     {error_html}
//     <form action="/login" method="post">
//         <label>Username
//             <input
//                 type="text"
//                 placeholder="Enter Username"
//                 name="username"
//             >
//         </label>
//         <label>Password
//             <input
//                 type="password"
//                 placeholder="Enter Password"
//                 name="password"
//             >
//         </label>
//         <button type="submit">Login</button>
//     </form>
// </body>
// </html>"#,
//         ))
        // .unwrap();
    let body = format!(
            r#"<!DOCTYPE html>
<html lang="en">
<head>
    <meta http-equiv="content-type" content="text/html; charset=utf-8">
    <title>Login</title>
</head>
<body>
    {error_html}
    <form action="/login" method="post">
        <label>Username
            <input
                type="text"
                placeholder="Enter Username"
                name="username"
            >
        </label>
        <label>Password
            <input
                type="password"
                placeholder="Enter Password"
                name="password"
            >
        </label>
        <button type="submit">Login</button>
    </form>
</body>
</html>"#
);
        (
            StatusCode::OK,
            [("Content-Type", "text/html")],
            jar.remove(Cookie::named("_flash")),
            body
        )
    
}