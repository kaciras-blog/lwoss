use std::borrow::BorrowMut;
use std::env;
use std::error::Error;
use axum::{body::StreamBody, routing::{get, post}, http::StatusCode, response::IntoResponse, Json, Router, http};
use std::net::SocketAddr;
use axum::extract::{BodyStream, Path};
use std::fs::{File};
use std::io::Write;
use std::path::{PathBuf};
use axum::response::Response;
use tokio_util::io::{ReaderStream, StreamReader};
use xxhash_rust::xxh3::Xxh3;
use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt;
use tempfile::NamedTempFile;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

#[tokio::main]
async fn main() {


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
