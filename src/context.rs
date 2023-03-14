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

	/// 接收上传的对象到临时文件，并计算 Hash，稍后可以决定是否保存。
	/// 这样能避免过大的文件消耗内存，适用于不需要在程序内处理的情况。
	pub async fn receive_file(&self, body: BodyStream) -> Result<FileBuf, axum::Error> {
		return FileBuf::receive(self, body).await;
	}
}

pub struct FileBuf {
	target: PathBuf,

	pub file: NamedTempFile,

	/// 20 个字符的 URL-Safe base64 字符串，是文件的 Hash。
	///
	/// 之所以选择 20 个字符，是因为它有 120 bit，接近原始输出 128，
	/// 且能被 6(base64) 和 8(byte) 整除。
	///
	/// 【大小写与文件系统】
	/// base64 是大小写敏感的，但有些文件系统不敏感（HFS+, exFAT, ...），
	/// 在这些系统上，base64 每个字符的种类由 64 降为 38，通过计算：
	///
	///   log(2, pow(38, N)) / log(2, pow(64, N))
	/// = log(38) / log(64)
	/// ≈ 0.875
	///
	/// 可以得出，此时 base64 有效位数降低为原来的 0.875 倍，
	/// 也就是从 120 bit 降低为 104.95 bit，碰撞几率仍然很低。
	///
	/// 由生日问题可以得出，104 bit 需要一千四百亿输入才能达到一亿分之一的碰撞率。
	/// https://en.wikipedia.org/wiki/Birthday_attack
	///
	/// 【为什么不用 Hex】
	/// 我有强迫症，能省几个字符坚决不用更长的，而且文件名太长也不好看。
	pub hash: String,
}

// 一个请求只能上传一个文件，不支持用 Form 一次传多个，理由如下：
// 1) 多传让请求体的大小限制混乱。
// 2) 多传的实现更复杂，而且能被多次单传替代，而且没看到明显收益。
impl FileBuf {

	// Create temp file in the same drive as data folder to avoid copy on rename.
	async fn receive(ctx: &OSSContext, mut body: BodyStream) -> Result<FileBuf, axum::Error> {
		let mut file = NamedTempFile::new_in(&ctx.buf_dir).unwrap();

		// 非加密 Hash 速度快，但有恶意碰撞的风险，在允许公开上传时需要注意。
		let mut hasher = Xxh3::new();

		while let Some(chunk) = body.next().await {
			let data = chunk?;
			hasher.update(&data);
			file.write(&data).unwrap();
		}

		let hash = hasher.digest128().to_be_bytes();
		let hash = general_purpose::URL_SAFE_NO_PAD.encode(&hash[..15]);

		return Ok(FileBuf { hash, file, target: ctx.data_dir.clone() });
	}

	pub fn save(self) -> Result<File, PersistError> {
		log::debug!("New file saved, hash={}", self.hash);
		return self.file.persist(self.target.join(self.hash));
	}
}
