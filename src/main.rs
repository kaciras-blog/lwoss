use std::borrow::BorrowMut;
use std::env;
use std::error::Error;
use std::fs::{self, File};
use std::io::Write;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::{body::StreamBody, http, http::StatusCode, Json, middleware, response::IntoResponse, Router, routing::{get, post}};
use axum::extract::{BodyStream, Path, State};
use axum::http::Request;
use axum::middleware::Next;
use axum::response::Response;
use axum_extra::extract::cookie::{Cookie, CookieJar};
use base64::{Engine as _, engine::general_purpose};
use clap::{Parser, ValueHint};
use futures::StreamExt;
use serde::Deserialize;
use tempfile::NamedTempFile;
use tokio::signal;
use tokio_util::io::{ReaderStream, StreamReader};
use toml;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};
use xxhash_rust::xxh3::Xxh3;
use tower_http::limit::RequestBodyLimitLayer;

use crate::api::{DataDirs, download, login, upload};

mod api;

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
	password: Option<String>,
	body_limit: Option<usize>,
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
		password: config.password.clone(),
	};

	fs::create_dir_all(&api_ctx.data_dir).unwrap();
	fs::create_dir_all(&api_ctx.buf_dir).unwrap();

	let public_routes = Router::new()
		.route("/s/:hash", get(download))
		.route("/login", post(login));

	let mut admin_routes = Router::new()
		.route("/", post(upload));

	if let Some(password) = config.password {
		admin_routes = admin_routes.route_layer(middleware::from_fn_with_state(password, auth));
	}

	let mut app = public_routes.merge(admin_routes)
		.with_state(api_ctx)
		.layer(CorsLayer::new()
			.allow_origin(AllowOrigin::mirror_request())
			.allow_headers(Any)
			.allow_methods(Any));

	if let Some(size) = config.body_limit {
		app = app.layer(RequestBodyLimitLayer::new(size))
	}

	let addr = SocketAddr::from((
		config.host
			.map(|v| v.parse::<IpAddr>().unwrap())
			.unwrap_or(Ipv4Addr::LOCALHOST.into()),
		config.port.unwrap_or(3000)
	));
	println!("LW-OSS is listening on {}", addr);

	// `axum::Server` is a re-export of `hyper::Server`
	axum::Server::bind(&addr)
		.serve(app.into_make_service())
		.await.unwrap();
}

async fn auth<B>(
	State(password): State<String>,
	jar: CookieJar,
	request: Request<B>,
	next: Next<B>,
) -> Response {
	if let Some(cookie) = jar.get("password") {
		if cookie.value() == password {
			return next.run(request).await;
		}
	}
	return StatusCode::FORBIDDEN.into_response();
}

// https://github.com/tokio-rs/axum/blob/main/examples/graceful-shutdown
async fn shutdown_signal() {
	let ctrl_c = async {
		signal::ctrl_c()
			.await
			.expect("failed to install Ctrl+C handler");
	};

	#[cfg(unix)]
		let terminate = async {
		signal::unix::signal(signal::unix::SignalKind::terminate())
			.expect("failed to install signal handler")
			.recv()
			.await;
	};

	#[cfg(not(unix))]
		let terminate = std::future::pending::<()>();

	tokio::select! {
        _ = ctrl_c => {},
        _ = terminate => {},
    }

	println!("Signal received, starting graceful shutdown...");
}
