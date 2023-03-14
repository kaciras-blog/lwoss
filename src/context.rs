use std::fs::File;
use std::io::Write;
use std::path::PathBuf;

use axum::extract::BodyStream;
use base64::{Engine as _, engine::general_purpose};
use futures::StreamExt;
use serde::Serialize;
use tempfile::{NamedTempFile, PersistError};
use xxhash_rust::xxh3::Xxh3;

#[derive(Serialize)]
pub struct UploadVO {
	pub hash: String,
}

/// 业务逻辑的状态全，刚玩 Rust 所以弄得简单点，都保存在这一个对象里。
#[derive(Clone)]
pub struct OSSContext {
	pub data_dir: PathBuf,
	pub buf_dir: PathBuf,
	pub password: Option<String>,
}

impl OSSContext {
	pub async fn receive_file(&self, body: BodyStream) -> Result<FileBuf, axum::Error> {
		return FileBuf::receive(self, body).await;
	}
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
	async fn receive(state: &OSSContext, mut body: BodyStream) -> Result<FileBuf, axum::Error> {
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
