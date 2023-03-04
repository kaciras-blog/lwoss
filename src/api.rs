use std::borrow::BorrowMut;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{body::StreamBody, http, http::StatusCode, Json, response::IntoResponse, Router, routing::{get, post}};
use axum::extract::{BodyStream, Path, State};
use axum::response::Response;
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, ValueHint};
use cookie::Expiration;
use cookie::time::{Duration, OffsetDateTime};
use futures::StreamExt;
use serde::Serialize;
use tempfile::NamedTempFile;
use tokio_util::io::{ReaderStream, StreamReader};
use toml;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use xxhash_rust::xxh3::Xxh3;

#[derive(Clone)]
pub struct DataDirs {
	pub data_dir: PathBuf,
	pub buf_dir: PathBuf,
	pub password: Option<String>,
}

#[derive(Serialize)]
struct UploadVO {
	hash: String,
}

pub async fn login(State(options): State<DataDirs>, jar: CookieJar, body: String) -> Response {
	let password = match options.password {
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

pub async fn upload(state: State<DataDirs>, mut body: BodyStream) -> Response {
	// Create temp file in the same drive as data folder to avoid copy on rename.
	let mut tmpfile = NamedTempFile::new_in(&state.buf_dir).unwrap();
	let mut hasher = Xxh3::new();

	while let Some(chunk) = body.next().await {
		match chunk {
			Ok(data) => {
				tmpfile.write(&data).unwrap();
				hasher.update(&data);
			}
			Err(_) => return StatusCode::BAD_REQUEST.into_response(),
		};
	};

	let hash = hasher.digest128().to_be_bytes();
	let hash = general_purpose::URL_SAFE_NO_PAD.encode(&hash[..15]);
	println!("hash is {}", hash);

	let path = &state.data_dir.join(&hash);
	if !path.exists() {
		fs::rename(tmpfile, path).unwrap();
	}

	return Json(UploadVO { hash }).into_response();
}

pub async fn download(state: State<DataDirs>, Path(hash): Path<String>) -> Response {
	let path = &state.data_dir.join(hash);

	let file = match tokio::fs::File::open(path).await {
		Ok(file) => file,
		Err(_) => return StatusCode::NOT_FOUND.into_response(),
	};

	return StreamBody::new(ReaderStream::new(file)).into_response();
}
