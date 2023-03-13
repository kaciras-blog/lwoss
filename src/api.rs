use std::ops::Deref;
use std::sync::Arc;

use axum::{body::StreamBody, http::StatusCode, Json, response::IntoResponse, Router};
use axum::body::Body;
use axum::extract::{BodyStream, Path, State};
use axum::http::{HeaderMap, Request};
use axum::http::request::Parts;
use axum::response::Response;
use axum::routing::{get, post};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use cookie::time::{Duration, OffsetDateTime};
use log;
use serde::Serialize;
use tokio::io::AsyncSeekExt;
use tokio_util::io::ReaderStream;
use tower_http::limit::ResponseBody;

use crate::bucket::UploadVO;
use crate::context::OSSContext;
use crate::range::send_range;

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

pub async fn upload(ctx: State<OSSContext>, body: BodyStream) -> Response {
	let buf = ctx.receive_file(body).await.unwrap();
	let hash = buf.hash.clone();
	println!("hash is {}", hash);

	buf.save().unwrap();
	return Json(UploadVO { hash }).into_response();
}

pub async fn download(ctx: State<OSSContext>, Path(hash): Path<String>, headers: HeaderMap) -> Response {
	let path = ctx.data_dir.join(hash);
	send_range(headers, path, String::from("image/png")).await
}

pub fn manual_bucket() -> Router<OSSContext> {
	return Router::new().route("/", post(upload)).route("/:hash", get(download));
}
