use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use axum::extract::{BodyStream, Path, State};
use axum::response::Response;
use axum::{body::StreamBody, http::StatusCode, response::IntoResponse, routing::post, Json};
use axum_extra::extract::cookie::Cookie;
use axum_extra::extract::CookieJar;
use base64::{engine::general_purpose, Engine as _};
use cookie::time::{Duration, OffsetDateTime};
use futures::StreamExt;
use serde::Serialize;
use tempfile::{NamedTempFile, PersistError};
use tokio_util::io::ReaderStream;
use toml;
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
	let buf = FileBuf::receive(&state, body).await.unwrap();
	let hash = buf.hash.clone();
	println!("hash is {}", hash);

	buf.save().unwrap();
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

pub struct FileBuf {
	target: PathBuf,

	pub file: NamedTempFile,
	pub hash: String,
}

// 一个请求只能上传一个文件，不支持用 Form 一次传多个，理由如下：
// 1) 多传让请求体的大小限制混乱。
// 2) 多传的实现更复杂，而且能被多次单传替代，而且没看到明显收益。
impl FileBuf {
	// Create temp file in the same drive as data folder to avoid copy on rename.
	pub async fn receive(state: &DataDirs, mut body: BodyStream) -> Result<FileBuf, axum::Error> {
		let mut file = NamedTempFile::new_in(&state.buf_dir).unwrap();
		let mut hasher = Xxh3::new();

		while let Some(chunk) = body.next().await {
			let data = chunk?;
			hasher.update(&data);
			file.write(&data).unwrap();
		}

		let hash = hasher.digest128().to_be_bytes();
		let hash = general_purpose::URL_SAFE_NO_PAD.encode(&hash[..15]);

		return Ok(FileBuf { hash, file, target: state.data_dir.clone() });
	}

	pub fn save(self) -> Result<File, PersistError> {
		return self.file.persist(self.target.join(self.hash));
	}
}
