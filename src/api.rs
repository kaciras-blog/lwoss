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
use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, ValueHint};
use futures::StreamExt;
use tempfile::NamedTempFile;
use tokio_util::io::{ReaderStream, StreamReader};
use toml;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use xxhash_rust::xxh3::Xxh3;

#[derive(Clone)]
pub struct DataDirs {
	pub data_dir: PathBuf,
	pub buf_dir: PathBuf,
}

pub async fn upload(state: State<DataDirs>, mut body: BodyStream) -> Response {
	// Create temp file in the same drive as data folder to avoid copy on rename.
	let mut tmpfile = NamedTempFile::new_in(&state.buf_dir).unwrap();
	let mut hasher = Xxh3::new();

	while let Some(chunk) = body.next().await {
		match chunk {
			Ok(data) => {
				hasher.update(data.as_ref());
				tmpfile.write(data.as_ref()).unwrap();
			}
			Err(_) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
		};
	};

	let hash = hasher.digest128().to_be_bytes();
	let hash = general_purpose::URL_SAFE_NO_PAD.encode(&hash[..15]);
	println!("hash is {}", hash);

	let path = &state.data_dir.join(&hash);
	if !path.exists() {
		fs::rename(tmpfile, path).unwrap();
	}

	return (StatusCode::OK, hash).into_response();
}

pub async fn download(state: State<DataDirs>, Path(hash): Path<String>) -> Response {
	let path = &state.data_dir.join(hash);

	let file = match tokio::fs::File::open(path).await {
		Ok(file) => file,
		Err(err) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
	};

	let file = ReaderStream::new(file);
	let body = StreamBody::new(file);
	return (StatusCode::OK, body).into_response();
}
