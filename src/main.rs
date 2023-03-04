use std::borrow::BorrowMut;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;

use axum::{body::StreamBody, http, http::StatusCode, Json, response::IntoResponse, Router, routing::{get, post}};
use axum::extract::{BodyStream, Path};
use axum::response::Response;
use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, ValueHint};
use futures::StreamExt;
use tempfile::NamedTempFile;
use tokio_util::io::{ReaderStream, StreamReader};
use toml;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use xxhash_rust::xxh3::Xxh3;

use serde::Deserialize;

#[derive(Parser, Debug)]
struct Args {
	/// Specific config file path.
	#[arg(long, value_hint = ValueHint::FilePath)]
	config: Option<PathBuf>,
}

#[derive(Deserialize)]
struct AppConfig {
	host: Option<String>,
	port: Option<u16>,
	data_dir: Option<PathBuf>,
}

fn load_config(args: Args) -> AppConfig {
	let config = match args.config {
		Some(file) => fs::read_to_string(file),
		None => {
			let mut file = env::current_dir().unwrap();
			file.push("lwoss.toml");
			if !file.is_file() {
				Ok(String::with_capacity(0))
			} else {
				fs::read_to_string(file)
			}
		}
	};
	let config = config.unwrap();
	return toml::from_str(config.as_str()).unwrap();
}

#[tokio::main]
async fn main() {
	let config = load_config(Args::parse());

	// build our application with a route
	let app = Router::new()
		.route("/", post(upload))
		.route("/s/:hash", get(create_user))
		.layer(CorsLayer::new()
			.allow_origin(AllowOrigin::mirror_request())
			.allow_headers(Any)
			.allow_methods(Any));

	// `axum::Server` is a re-export of `hyper::Server`
	let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();

	println!("LWEOS listening on {}", addr);
}

async fn create_user(Path(hash): Path<String>) -> Response {
	let file = match tokio::fs::File::open("Cargo.toml").await {
		Ok(file) => file,
		Err(err) => return (StatusCode::NOT_FOUND, "File not found").into_response(),
	};

	let file = ReaderStream::new(file);
	let body = StreamBody::new(file);
	return (StatusCode::OK, body).into_response();
}

async fn upload(mut body: BodyStream) -> Response {
	// Create temp file in the same drive as data folder to avoid copy on rename.
	let cwd = env::current_dir().unwrap();
	let mut tmpfile = NamedTempFile::new_in(cwd).unwrap();

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
	let hash = general_purpose::URL_SAFE_NO_PAD.encode(&hash[..20]);
	println!("hash is {}", hash);

	std::fs::rename(tmpfile, hash).unwrap();
	return (StatusCode::OK).into_response();
}
