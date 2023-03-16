use std::io::ErrorKind;
use std::path::{Component, Path, PathBuf};

use axum::body::Body;
use axum::extract::State;
use axum::http::{Method, Request, StatusCode};
use axum::response::{IntoResponse, Response};
use axum::Router;

use crate::range::{FileCache, FileRangeReadr, send_range};

fn normalize_path(base: &Path, path: &str) -> Option<PathBuf> {
	let path = path.trim_start_matches('/');
	let path = Path::new(path);

	let mut joined = base.to_path_buf();

	for component in path.components() {
		match component {
			Component::Normal(comp) => {
				// protect against paths like `/foo/c:/bar/baz` (#204)

				if Path::new(&comp)
					.components()
					.all(|c| matches!(c, Component::Normal(_)))
				{
					joined.push(comp)
				} else {
					return None;
				}
			}
			Component::CurDir => {}
			_ => return None,
		}
	}
	Some(joined)
}

#[derive(Clone)]
struct ServeDirectory {
	pub base: PathBuf,
	pub fallback: Option<PathBuf>,
}

async fn serve_dir(state: State<ServeDirectory>, request: Request<Body>) -> Response {
	let method = request.method();
	if method != Method::GET && method != Method::HEAD {
		return StatusCode::METHOD_NOT_ALLOWED.into_response();
	}

	let path = request.uri().path();
	if let Some(path) = normalize_path(&state.base, path) {
		let response = serve_file(&path, &request).await;

		if response.status() != StatusCode::NOT_FOUND {
			return response;
		}
		if let Some(default) = &state.fallback {
			return serve_file(default, &request).await;
		}
		response
	} else {
		return StatusCode::NOT_FOUND.into_response();
	}
}

async fn serve_file(path: &Path, request: &Request<Body>) -> Response {
	let mime = mime_guess::from_path(&path)
		.first_raw()
		.unwrap_or("application/octet-stream")
		.to_string();

	// TODO: compressed variants

	match FileRangeReadr::open(path, mime, FileCache::Modified).await {
		Ok(file) => {
			send_range(request.headers(), file).await
		},
		Err(e) => match e.kind() {
			ErrorKind::NotFound => {
				StatusCode::NOT_FOUND.into_response()
			},
			_ if path.is_dir() => {
				StatusCode::NOT_FOUND.into_response()
			},
			_ => StatusCode::INTERNAL_SERVER_ERROR.into_response()
		}
	}
}

pub fn serve_static<OS>(base: PathBuf, fallback: Option<PathBuf>) -> Router<OS> {
	Router::new().fallback(serve_dir).with_state(ServeDirectory { base, fallback })
}
