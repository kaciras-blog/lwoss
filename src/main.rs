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

#[tokio::main]
async fn main() {


	// build our application with a route
	let app = Router::new()
		.route("/s/:hash", get(create_user));

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
