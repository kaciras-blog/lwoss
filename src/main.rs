mod api;

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
use crate::api::{DataDirs,upload,download};

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
	let wd = config.data_dir.unwrap_or("data".into());

	let api_ctx = DataDirs {
		data_dir: wd.join("files"),
		buf_dir: wd.join("buffer"),
	};

	fs::create_dir_all(&api_ctx.data_dir).unwrap();
	fs::create_dir_all(&api_ctx.buf_dir).unwrap();

	// build our application with a route
	let app = Router::new()
		.route("/", post(upload))
		.route("/s/:hash", get(download))
		.layer(CorsLayer::new()
			.allow_origin(AllowOrigin::mirror_request())
			.allow_headers(Any)
			.allow_methods(Any));

	let app = app.with_state(api_ctx);
	let addr = SocketAddr::from(([127, 0, 0, 1], 3000));
	println!("LWEOS listening on {}", addr);

	// `axum::Server` is a re-export of `hyper::Server`

	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await
		.unwrap();
}
