use std::fs::Metadata;
use std::future::Future;
use std::io;
use std::io::{ErrorKind, SeekFrom};
use std::ops::RangeInclusive;
use std::path::Path;
use std::time::SystemTime;

use axum::body::{Empty, HttpBody, StreamBody};
use axum::http::{HeaderMap, StatusCode};
use axum::http::header::{
	ACCEPT_RANGES, CONTENT_LENGTH, CONTENT_RANGE,
	CONTENT_TYPE, ETAG, IF_NONE_MATCH,
	IF_UNMODIFIED_SINCE, LAST_MODIFIED, RANGE,
};
use axum::http::response::Builder;
use axum::response::{IntoResponse, Response};
use futures::{AsyncRead, FutureExt, StreamExt, TryFutureExt};
use http_range_header::parse_range_header;
use httpdate::{fmt_http_date, parse_http_date};
use tokio::fs::File;
use tokio::io::{AsyncReadExt, AsyncSeekExt, Take};
use tokio_util::io::ReaderStream;

pub enum FileCache {
	None,
	Hashed(String),
	Modified,
}

pub enum CacheIdentifier {
	None,
	Etag(String),
	Modified(SystemTime),
}

pub struct FileRangeReadr {
	file: File,
	metadata: Metadata,

	pub mime: String,
	pub cache: CacheIdentifier,
}

impl FileRangeReadr {
	pub async fn open(path: impl AsRef<Path>, mime: String, cache: FileCache) -> io::Result<Self> {
		let file = File::open(path).await?;
		let metadata = file.metadata().await?;

		let cache = match cache {
			FileCache::None => CacheIdentifier::None,
			FileCache::Hashed(hash) => CacheIdentifier::Etag(hash),
			FileCache::Modified => match metadata.modified() {
				Err(_) => CacheIdentifier::None,
				Ok(time) => CacheIdentifier::Modified(time),
			},
		};

		return Ok(FileRangeReadr { file, metadata, cache, mime });
	}

	pub fn size(&self) -> u64 {
		return self.metadata.len();
	}

	pub fn get_whole(self) -> ReaderStream<File> {
		return ReaderStream::new(self.file);
	}

	pub async fn get_range(mut self, range: RangeInclusive<u64>) -> ReaderStream<Take<File>> {
		let size = range.end() - range.start() + 1;

		// There is only one seek in progress, so it don't return Err.
		self.file.seek(SeekFrom::Start(*range.start())).await.unwrap();
		return ReaderStream::new(self.file.take(size));
	}
}

pub async fn send_file(
	path: impl AsRef<Path>,
	headers: &HeaderMap,
	mime: String,
	cache: FileCache
) -> Response {
	match FileRangeReadr::open(path, mime, cache).await {
		Ok(file) => {
			send_range(headers, file).await
		}
		Err(e) => match e.kind() {
			ErrorKind::NotFound => StatusCode::NOT_FOUND.into_response(),
			_ => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
		}
	}
}

/// 发送一个文件，支持 206 Partial Content。
/// 暂时无法发送多段，因为实现起来复杂些，而且没见过这种请求，如果遇到了再考虑。
///
/// https://tools.ietf.org/html/rfc7233#section-4.1
///
/// 代码参考了：
/// https://github.com/tower-rs/tower-http/blob/master/tower-http/src/services/fs/serve_dir/future.rs
///
pub async fn send_range(headers: &HeaderMap, mut reader: FileRangeReadr) -> Response {
	let mut builder = Response::builder().header(ACCEPT_RANGES, "bytes");

	// Cache-Control is added by middleware,
	builder = match &reader.cache {

		// https://developer.mozilla.org/docs/Web/HTTP/Headers/ETag
		CacheIdentifier::Etag(value) => {
			let x = headers.get(IF_NONE_MATCH)
				.and_then(|v| v.to_str().ok());

			if x == Some(&value) {
				return StatusCode::NOT_MODIFIED.into_response();
			}
			builder.header(ETAG, value)
		}

		// https://developer.mozilla.org/docs/Web/HTTP/Headers/Last-Modified
		CacheIdentifier::Modified(time) => {
			let x = headers.get(IF_UNMODIFIED_SINCE)
				.and_then(|v| v.to_str().ok())
				.and_then(|v| parse_http_date(v).ok());

			if x == Some(*time) {
				return StatusCode::NOT_MODIFIED.into_response();
			}
			builder.header(LAST_MODIFIED, fmt_http_date(*time))
		}

		CacheIdentifier::None => builder, // Cache disabled or not available.
	};

	if let Some(value) = headers.get(RANGE) {
		// Use option chain to handle various type of errors.
		let ranges = value.to_str().ok()
			.map(|s| s.to_owned())
			.and_then(|s| parse_range_header(&s).ok())
			.and_then(|r| r.validate(reader.size()).ok());

		if let Some(ranges) = ranges {
			if ranges.len() == 1 {
				return single(builder, reader, ranges[0].to_owned()).await;
			}
			log::warn!("Can not handle request with multiple ranges.")
		}

		// Ranges parsing failed，or has unsatisfied value.
		let empty = Empty::new().map_err(|e| match e {});
		builder
			.status(StatusCode::RANGE_NOT_SATISFIABLE)
			.header(CONTENT_RANGE, format!("bytes */{}", reader.size()))
			.body(empty.boxed_unsync()).unwrap()
	} else {
		// No Range header in the request，send whole file.
		builder
			.header(CONTENT_LENGTH, reader.size())
			.header(CONTENT_TYPE, &reader.mime)
			.body(StreamBody::new(reader.get_whole()).boxed_unsync()).unwrap()
	}
}

async fn single(builder: Builder, mut reader: FileRangeReadr, x: RangeInclusive<u64>) -> Response {
	let length = x.end() - x.start() + 1;

	builder.status(StatusCode::PARTIAL_CONTENT)
		.header(CONTENT_RANGE, format!("bytes {}-{}/{}", x.start(), x.end(), reader.size()))
		.header(CONTENT_LENGTH, length)
		.header(CONTENT_TYPE, &reader.mime)
		.body(StreamBody::new(reader.get_range(x).await).boxed_unsync()).unwrap()
}

#[cfg(test)]
mod tests {
	use std::io::ErrorKind;

	use axum::body::{BoxBody, HttpBody};
	use axum::http::HeaderMap;
	use hyper::body::to_bytes;

	use crate::range::{FileCache, FileRangeReadr, send_range};

	const FILE: &str = "test-files/sendrange.txt";

	async fn stub() -> FileRangeReadr {
		FileRangeReadr::open(FILE, "text/plain".into(), FileCache::None).await.unwrap()
	}

	async fn assert_body(actual: BoxBody, expected: &[u8]) {
		assert_eq!(to_bytes(actual).await.unwrap().as_ref(), expected);
	}

	#[tokio::test]
	async fn not_found() {
		let result = FileRangeReadr::open("404", "text/plain".into(), FileCache::None).await;
		assert_eq!(result.err().unwrap().kind(), ErrorKind::NotFound);
	}

	// ============================= Range =============================

	#[tokio::test]
	async fn invalid_range() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "foobar".try_into().unwrap());

		let (p, b) = send_range(&headers, stub().await).await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_eq!(b.is_end_stream(), true);
	}

	#[tokio::test]
	async fn non_range() {
		let headers = HeaderMap::new();
		let (p, b) = send_range(&headers, stub().await).await.into_parts();
		insta::assert_debug_snapshot!(p);
		assert_body(b, std::fs::read(FILE).unwrap().as_slice()).await;
	}

	#[tokio::test]
	async fn single() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=1-3".try_into().unwrap());

		let (p, b) = send_range(&headers, stub().await).await.into_parts();
		insta::assert_debug_snapshot!(p);
		assert_body(b, b"f m").await;
	}

	#[tokio::test]
	async fn begin_only() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=470-".try_into().unwrap());

		let (p, b) = send_range(&headers, stub().await).await.into_parts();
		insta::assert_debug_snapshot!(p);
		assert_body(b, b"ead).").await;
	}

	#[tokio::test]
	async fn end_only() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=-2".try_into().unwrap());

		let (p, b) = send_range(&headers, stub().await).await.into_parts();
		insta::assert_debug_snapshot!(p);
		assert_body(b, b").").await;
	}

	#[tokio::test]
	async fn multiple() {
		let mut headers = HeaderMap::new();
		headers.append("Range", "bytes=80-83,429-472,294-304".try_into().unwrap());

		let (p, b) = send_range(&headers, stub().await).await.into_parts();

		insta::assert_debug_snapshot!(p);
		assert_eq!(b.is_end_stream(), true);
	}

	// ============================= caching =============================

	#[tokio::test]
	async fn etag() {}
}
