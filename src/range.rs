use std::fs::Metadata;
use std::io::{ErrorKind, SeekFrom};
use std::ops::RangeInclusive;
use std::path::Path;

use axum::body::{Empty, HttpBody, StreamBody};
use axum::http::{HeaderMap, StatusCode};
use axum::http::response::Builder;
use axum::response::{IntoResponse, Response};
use futures::{FutureExt, StreamExt};
use http_range_header::parse_range_header;
use httpdate::fmt_http_date;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt};
use tokio_util::io::ReaderStream;

pub async fn send_file(headers: HeaderMap, path: impl AsRef<Path>, mime: &str) -> Response {
	match File::open(path).await {
		Ok(file) => {
			match file.metadata().await {
				Ok(attrs) => send(headers, file, attrs, mime).await,
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

/// 发送一个文件，支持 206 Partial Content。
/// 暂时无法发送多段，因为实现起来复杂些，而且没见过这种请求，如果遇到了再考虑。
///
/// https://tools.ietf.org/html/rfc7233#section-4.1
///
/// 代码参考了：
/// https://github.com/tower-rs/tower-http/blob/master/tower-http/src/services/fs/serve_dir/future.rss
///
async fn send(headers: HeaderMap, file: File, attrs: Metadata, mime: &str) -> Response {
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
				let range = ranges[0].to_owned();
				return single(builder, file, attrs, range, mime).await;
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

async fn single(builder: Builder, mut file: File, attrs: Metadata, x: RangeInclusive<u64>, mime: &str) -> Response {
	let size = x.end() - x.start() + 1;

	// There is only one seek in progress, so it don't return Err.
	file.seek(SeekFrom::Start(*x.start())).await.unwrap();
	let reader = ReaderStream::new(file.take(size));

	builder.status(StatusCode::PARTIAL_CONTENT)
		.header("Content-Length", size)
		.header("Content-Type", mime)
		.header("Content-Range", format!("bytes {}-{}/{}", x.start(), x.end(), attrs.len()))
		.body(StreamBody::new(reader).boxed_unsync()).unwrap()
}

#[cfg(test)]
mod tests {
	use axum::body::{BoxBody, HttpBody};
	use axum::http::HeaderMap;
	use hyper::body::to_bytes;

	use crate::range::send_file;

	const FILE: &str = "test-files/sendrange.txt";

	async fn assert_body(actual: BoxBody, expected: &[u8]) {
		assert_eq!(to_bytes(actual).await.unwrap().as_ref(), expected);
	}

	#[tokio::test]
	async fn not_found() {
		let (p, b) = send_file(HeaderMap::new(), "404", "text/plain").await.into_parts();
		insta::assert_debug_snapshot!(p);
		assert_eq!(b.is_end_stream(), true);
	}

	#[tokio::test]
	async fn invalid_range() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "foobar".try_into().unwrap());

		let (p, b) = send_file(headers, FILE, "text/plain").await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_eq!(b.is_end_stream(), true);
	}

	#[tokio::test]
	async fn non_range() {
		let (p, b) = send_file(HeaderMap::new(), FILE, "text/plain").await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_body(b, std::fs::read(FILE).unwrap().as_slice()).await;
	}

	#[tokio::test]
	async fn single() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=1-3".try_into().unwrap());

		let (p, b) = send_file(headers, FILE, "text/html").await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_body(b, b"f m").await;
	}

	#[tokio::test]
	async fn begin_only() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=470-".try_into().unwrap());

		let (p, b) = send_file(headers, FILE, "text/html").await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_body(b, b"ead).").await;
	}

	#[tokio::test]
	async fn end_only() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=-2".try_into().unwrap());

		let (p, b) = send_file(headers, FILE, "text/html").await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_body(b, b").").await;
	}

	#[tokio::test]
	async fn multiple() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=80-83,429-472,294-304".try_into().unwrap());

		let (p, b) = send_file(headers, FILE, "text/html").await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_eq!(b.is_end_stream(), true);
	}
}
