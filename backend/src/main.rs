use axum::extract::Path;
use axum::http::header::HeaderMap;
use axum::http::{Method, StatusCode};
use axum::routing::{get, post};
use axum::{Extension, Json, Router};
use axum_extra::extract::cookie::{Cookie, Expiration, Key};
use axum_extra::extract::PrivateCookieJar;
use time::{Duration, OffsetDateTime};

#[tokio::main]
async fn main() {
    println!(
        "Running for client id {}",
        std::env::var("CLIENT_ID").unwrap()
    );
    let key = Key::from(std::env::var("CLIENT_SECRET").unwrap().as_ref());

    let app = Router::new()
        .route("/homeworker/login", post(login))
        .route("/homeworker/logout", post(logout))
        .route(
            "/homeworker/api/v2/*path",
            get(proxy).post(proxy).delete(proxy),
        )
        .layer(Extension(key));

    let addr = std::net::SocketAddr::from(([127, 0, 0, 1], 3000));
    axum::Server::bind(&addr)
        .serve(app.into_make_service())
        .await
        .unwrap();
}

async fn login(
    mut jar: PrivateCookieJar,
    body: String,
) -> Result<PrivateCookieJar, (StatusCode, Json<homeworker::types::ErrorResponse>)> {
    println!("Login with code {body}");

    match homeworker::auth::exchange_token(
        std::env::var("CLIENT_ID").unwrap(),
        std::env::var("CLIENT_SECRET").unwrap(),
        body,
    )
    .await
    {
        Ok(response) => {
            jar = jar.add(
                Cookie::build("access-token", response.access_token)
                    .http_only(true)
                    .secure(true)
                    .path("/homeworker")
                    .max_age(Duration::seconds(response.expires_in as i64))
                    .finish(),
            );
            jar = jar.add(
                Cookie::build("refresh-token", response.refresh_token)
                    .http_only(true)
                    .secure(true)
                    .path("/homeworker")
                    .expires(Expiration::from(
                        OffsetDateTime::now_utc() + Duration::days(729),
                    ))
                    .finish(),
            );
            Ok(jar)
        }
        Err(error) => match error {
            homeworker::Error::RequestError(_) => Err((
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(
                    homeworker::types::Error {
                        name: "Proxy".to_string(),
                        message: "Error while forwarding the request".to_string(),
                        code: StatusCode::INTERNAL_SERVER_ERROR.into(),
                    }
                    .into(),
                ),
            )),
            homeworker::Error::ApiError(err) => Err((StatusCode::BAD_GATEWAY, Json(err.into()))),
        },
    }
}

async fn logout(jar: PrivateCookieJar) -> PrivateCookieJar {
    jar.remove(Cookie::named("access-token"))
        .remove(Cookie::named("refresh-token"))
}

async fn proxy(
    mut cookie_jar: PrivateCookieJar,
    Path(path): Path<String>,
    method: axum::http::Method,
    headers: HeaderMap,
    body: String,
) -> (
    StatusCode,
    PrivateCookieJar,
    Result<String, Json<homeworker::types::ErrorResponse>>,
) {
    println!("Proxy to {method} {path}");

    let access_token = match cookie_jar.get("access-token") {
        Some(cookie) => cookie.value().to_owned(),
        None => {
            // Try to refresh the token
            let refresh_token = match cookie_jar.get("refresh-token") {
                Some(cookie) => cookie.value().to_owned(),
                None => {
                    println!("Token refresh: No refresh token");
                    return (
                        StatusCode::UNAUTHORIZED,
                        cookie_jar,
                        Err(Json(
                            new_error(
                                StatusCode::UNAUTHORIZED,
                                "No access or refresh token found.".to_string(),
                            )
                            .into(),
                        )),
                    );
                }
            };

            match homeworker::auth::refresh_token(
                std::env::var("CLIENT_ID").unwrap(),
                std::env::var("CLIENT_SECRET").unwrap(),
                refresh_token,
            )
            .await
            {
                Ok(response) => {
                    // Set new access cookie
                    println!("Token refresh: Success");
                    cookie_jar = cookie_jar.add(
                        Cookie::build("access-token", response.access_token.clone())
                            .http_only(true)
                            .secure(true)
                            .path("/homeworker")
                            .max_age(Duration::seconds(response.expires_in as i64))
                            .finish(),
                    );
                    response.access_token
                }
                Err(e) => {
                    println!("Token refresh: {:?}", e);
                    return (
                        StatusCode::UNAUTHORIZED,
                        cookie_jar,
                        Err(Json(
                            new_error(
                                StatusCode::UNAUTHORIZED,
                                "Unable to refresh the access token.".to_string(),
                            )
                            .into(),
                        )),
                    );
                }
            }
        }
    };

    let user_agent = match headers.get("User-Agent") {
        Some(header) => header.to_str().unwrap(),
        None => {
            return (StatusCode::UNAUTHORIZED, cookie_jar, Err(
                Json(new_error(
                        StatusCode::UNAUTHORIZED,
                    "No User-Agent header found. This may be required for accessing the homeworker API.".to_string(),
                ).into()),
            ));
        }
    };

    let request_builder = match method {
        Method::GET => {
            reqwest::Client::new().get("https://homeworker.li/api/v2".to_string() + &path)
        }
        Method::POST => reqwest::Client::new()
            .post("https://homeworker.li/api/v2".to_string() + &path)
            .body(body),
        Method::DELETE => reqwest::Client::new()
            .delete("https://homeworker.li/api/v2".to_string() + &path)
            .body(body),
        _ => todo!(),
    };

    let response = match request_builder
        .header("Authorization", "Bearer ".to_string() + &access_token)
        .header("User-Agent", user_agent)
        .send()
        .await
    {
        Ok(response) => response,
        Err(_) => {
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                cookie_jar,
                Err(Json(
                    new_error(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        "Proxy could not reach the homeworker API".to_string(),
                    )
                    .into(),
                )),
            );
        }
    };

    (
        response.status(),
        cookie_jar,
        Ok(response.text().await.unwrap()),
    )
}

fn new_error(code: StatusCode, message: String) -> homeworker::types::Error {
    homeworker::types::Error {
        name: "Proxy".to_string(),
        message,
        code: code.into(),
    }
}
