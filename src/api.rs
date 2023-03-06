use axum::{body::StreamBody, http::StatusCode, Json, response::IntoResponse, routing::post};
use axum::extract::{BodyStream, Path, State};
use axum::response::Response;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use cookie::time::{Duration, OffsetDateTime};
use serde::Serialize;
use tokio_util::io::ReaderStream;
use crate::context::{OSSContext};

#[derive(Serialize)]
struct UploadVO {
	hash: String,
}

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

pub async fn upload(ctx: State<OSSContext>, mut body: BodyStream) -> Response {
	let buf = ctx.receive_file(body).await.unwrap();
	let hash = buf.hash.clone();
	println!("hash is {}", hash);

	buf.save().unwrap();
	return Json(UploadVO { hash }).into_response();
}

pub async fn download(ctx: State<OSSContext>, Path(hash): Path<String>) -> Response {
	let path = &ctx.data_dir.join(hash);

	let file = match tokio::fs::File::open(path).await {
		Ok(file) => file,
		Err(_) => return StatusCode::NOT_FOUND.into_response(),
	};

	return StreamBody::new(ReaderStream::new(file)).into_response();
}
