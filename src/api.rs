use axum::{http::StatusCode, response::IntoResponse};
use axum::extract::State;
use axum::response::Response;
use axum::routing::post;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use cookie::time::{Duration, OffsetDateTime};

use crate::context::OSSContext;

pub async fn login(State(ctx): State<OSSContext>, jar: CookieJar, body: String) -> Response {
	let password = match ctx.password {
		Some(value) => value,
		None => return StatusCode::NO_CONTENT.into_response(),
	};

	if body != password {
		return StatusCode::BAD_REQUEST.into_response();
	}

	let mut cookie = Cookie::new("password", password);
	cookie.set_http_only(true);
	cookie.set_expires(OffsetDateTime::now_utc() + Duration::weeks(52));

	return (StatusCode::NO_CONTENT, jar.add(cookie)).into_response();
}
