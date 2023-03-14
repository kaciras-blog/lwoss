use std::fs::Metadata;
use std::io::{ErrorKind, Seek, SeekFrom};
use std::ops::RangeInclusive;
use std::path::PathBuf;

use axum::body::{Bytes, Empty, HttpBody, StreamBody};
use axum::Error;
use axum::http::{HeaderMap, StatusCode};
use axum::http::response::Builder;
use axum::response::{IntoResponse, Response};
use futures::{FutureExt, StreamExt};
use http_range_header::parse_range_header;
use httpdate::fmt_http_date;
use tokio::fs::File;
use tokio::io::AsyncSeekExt;
use tokio_util::io::ReaderStream;
use tower_http::limit::ResponseBody;

pub async fn send_file_range(headers: HeaderMap, file: PathBuf, mime: String) -> Response {
	match File::open(file).await {
		Ok(file) => {
			match file.metadata().await {
				Ok(attrs) => send_range(headers, file, attrs, mime).await,
				Err(_) => StatusCode::INTERNAL_SERVER_ERROR.into_response()
			}
		}
		Err(e) => {
			if e.kind() == ErrorKind::NotFound {
				return StatusCode::NOT_FOUND.into_response();
			}
			return StatusCode::INTERNAL_SERVER_ERROR.into_response();
		}
	}
}

/// https://github.com/tower-rs/tower-http/blob/master/tower-http/src/services/fs/serve_dir/future.rss
async fn send_range(headers: HeaderMap, file: File, attrs: Metadata, mime: String) -> Response {
	let builder = Response::builder()
		.header("Accept-Ranges", "bytes")
		.header("Last-Modified", fmt_http_date(attrs.modified().unwrap()));

	if let Some(value) = headers.get("Range") {
		// Use option chain to handle various type of errors.
		let ranges = value.to_str().ok()
			.map(|s| s.to_owned())
			.and_then(|s| parse_range_header(&s).ok())
			.and_then(|r| r.validate(attrs.len()).ok());

		if let Some(ranges) = ranges {
			if ranges.len() == 1 {
				return single(builder, file, ranges[0].clone(), mime).await;
			}
			log::warn!("Can not handle request with multiple ranges.")
		}

		// Ranges parsing failed，or has unsatisfied value.
		let empty = Empty::new().map_err(|e| match e {});
		builder
			.status(StatusCode::RANGE_NOT_SATISFIABLE)
			.header("Content-Range", format!("bytes */{}", attrs.len()))
			.body(empty.boxed_unsync()).unwrap()
	} else {
		// No Range header in the request，send whole file.
		builder
			.header("Content-Length", attrs.len())
			.header("Content-Type", mime)
			.body(StreamBody::new(ReaderStream::new(file)).boxed_unsync()).unwrap()
	}
}

async fn single(builder: Builder, mut file: File, x: RangeInclusive<u64>, mime: String) -> Response {
	let size = x.end() - x.start() + 1;

	// There is only one seek in progress, so it don't return Err.
	file.seek(SeekFrom::Start(*x.start())).await.unwrap();
	let reader = ReaderStream::new(file);

	builder.status(StatusCode::PARTIAL_CONTENT)
		.header("Content-Length", size)
		.header("Content-Type", mime)
		.header("Content-Range", format!("bytes {}-{}/{}", x.start(), x.end(), size))
		.body(StreamBody::new(reader.take(size as usize)).boxed_unsync()).unwrap()
}
