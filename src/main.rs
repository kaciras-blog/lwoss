use std::env;
use std::error::Error;
use std::fs::{self, File, OpenOptions};
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::path::PathBuf;

use axum::{Router, Server};
use axum::extract::State;
use axum::http::{Request, StatusCode};
use axum::middleware::{self, Next};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum_extra::extract::cookie::CookieJar;
use clap::{Parser, ValueHint};
use log::{self, LevelFilter, SetLoggerError};
use serde::Deserialize;
use simplelog::{ColorChoice, Config, ConfigBuilder, TerminalMode, WriteLogger, TermLogger};
use tokio::signal;
use toml;
use tower_http::cors::{AllowOrigin, Any, CorsLayer};

use crate::api::{download, login, upload};
use crate::context::OSSContext;

mod api;
mod context;

#[derive(Parser, Debug)]
struct Args {
	/// Specific config file path.
	#[arg(long, value_hint = ValueHint::FilePath)]
	config: Option<PathBuf>,
}

#[derive(Deserialize)]
struct AppConfig {
	log_level: Option<String>,
	log_file: Option<String>,

	host: Option<String>,
	port: Option<u16>,
	body_limit: Option<usize>,

	password: Option<String>,
	data_dir: Option<PathBuf>,
}

fn load_config(args: Args) -> AppConfig {
	let config = match args.config {
		Some(file) => fs::read_to_string(file),
		None => {
			let mut file = env::current_dir().unwrap();
			file.push("lwoss.toml");
			if file.is_file() {
				fs::read_to_string(file)
			} else {
				Ok(String::with_capacity(0))
			}
		}
	};
	let config = config.unwrap();
	return toml::from_str(config.as_str()).unwrap();
}

fn setup_logger(options: &AppConfig) -> Result<(), Box<dyn Error>> {
	let cfg = ConfigBuilder::new()
		.add_filter_allow_str("lwoss")
		.build();

	let lv = match &options.log_level {
		None => LevelFilter::Info,
		Some(name) => match name.as_str() {
			"off" => LevelFilter::Off,
			"error" => LevelFilter::Error,
			"warn" => LevelFilter::Warn,
			"info" => LevelFilter::Info,
			"debug" => LevelFilter::Debug,
			"trace" => LevelFilter::Trace,
			_ => return Err(Box::new(std::fmt::Error {})),
		}
	};

	if let Some(file) = &options.log_file {
		let file = OpenOptions::new()
			.create(true)
			.append(true)
			.open(file)?;
		WriteLogger::init(lv, cfg, file)?;
	} else {
		TermLogger::init(lv, cfg, TerminalMode::Mixed, ColorChoice::Auto)?;
	}

	Ok(())
}

#[tokio::main]
async fn main() {
	let config = load_config(Args::parse());

	setup_logger(&config).expect("Unable to create logger");

	let wd = config.data_dir.unwrap_or("data".into());
	let ctx = OSSContext {
		data_dir: wd.join("files"),
		buf_dir: wd.join("buffer"),
		password: config.password.clone(),
	};

	fs::create_dir_all(&ctx.data_dir).unwrap();
	fs::create_dir_all(&ctx.buf_dir).unwrap();

	let public_routes = Router::new()
		.route("/s/:hash", get(download))
		.route("/login", post(login));

	let mut admin_routes = Router::new()
		.route("/", post(upload));

	if let Some(password) = config.password {
		admin_routes = admin_routes.route_layer(middleware::from_fn_with_state(password, auth));
	}

	let app = public_routes.merge(admin_routes)
		.with_state(ctx)
		.layer(CorsLayer::new()
			.allow_origin(AllowOrigin::mirror_request())
			.allow_headers(Any)
			.allow_methods(Any));

	// https://github.com/tokio-rs/axum/issues/1110
	// if let Some(size) = config.body_limit {
	// 	app = app.layer(RequestBodyLimitLayer::new(size));
	// }

	let addr = SocketAddr::from((
		config.host
			.map(|v| v.parse::<IpAddr>().unwrap())
			.unwrap_or(Ipv4Addr::LOCALHOST.into()),
		config.port.unwrap_or(3000),
	));
	log::info!("LW-OSS is listening on {}", addr);

	// `axum::Server` is a re-export of `hyper::Server`
	Server::bind(&addr).serve(app.into_make_service()).await.unwrap();
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
