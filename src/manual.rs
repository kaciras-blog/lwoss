use axum::{Json, response::IntoResponse, Router};
use axum::extract::{BodyStream, Path, State};
use axum::http::{HeaderMap, StatusCode};
use axum::http::header::CACHE_CONTROL;
use axum::http::HeaderValue;
use axum::response::Response;
use axum::routing::{get, post};
use log;

use crate::context::{OSSContext, UploadVO};
use crate::range::{FileRangeReadr, send_range};

/*
 * 【文件的多层封装】
 * 一个文件可能有多层封装，如果只看浏览器支持的，最多有三层，以视频为例：
 * 1）最内层是视频流的编码，比如 H.264、HEVC、AV1。
 * 2）容器格式，比如 mp4、webm、mkv。
 * 3）打包压缩（虽然视频很少用），比如 gzip、br。
 *
 * HTTP 协议仅支持后两者，即 Accept 和 Accept-Encoding，而最内层的编码却没有对应的头部。
 * 如果要原生使用，通常是在 HTML 靠 <source> 标签选择，但这与本项目的后端选择策略相悖。
 *
 * 目前的想法是在前端检测支持的程度，然后加入请求头。
 * 这部分比较复杂，因为编码还可以细分各种 Profile，实现可以参考：
 * https://cconcolato.github.io/media-mime-support
 * https://evilmartians.com/chronicles/better-web-video-with-av1-codec
 */

/**
 * 多版本存储策略，支持上传同一个资源的多个版本，下载时自动选择最优的。
 *
 * 比如视频转码很费时，放在服务端占用大量资源，可以选择让用户自己转码，
 * 然后上传同一视频的多个版本。
 *
 * <h2>安全性</h2>
 * 目前的实现未检查文件的路径和内容，存在恶意上传的风险，请用于可信来源！
 * file-type 之类的库会更好些，但即便检查了文件头，仍不能保证内容有效，除非完整地解码。
 *
 * <h2>原始版本</h2>
 * 以后要想做自动转码会用到，把旧版手动上传的转成新编码，这需要判断出那个
 * 是原始文件，目前视频较少手动记一下也行。
 *
 * <h2>内容一致性</h2>
 * 需要注意视频转码是有损的，这意味着难以检测上传的多个版本是否包含相同的内容，
 * 如果上传了不同的视频作为变体，则不同的浏览器可能访问到不同的内容。
 */
pub fn manual_bucket() -> Router<OSSContext> {
	return Router::new().route("/", post(upload)).route("/:hash", get(download));
}

struct ManualBucket {
	ctx: OSSContext,

}

async fn upload(ctx: State<OSSContext>, body: BodyStream) -> Response {
	let buf = ctx.receive_file(body).await.unwrap();
	let hash = buf.hash.clone();
	log::trace!("New file saved, hash={}", hash);

	buf.save().unwrap();
	return Json(UploadVO { hash }).into_response();
}

const IMMUTABLE: &str = "public,max-age=31536000,immutable";

async fn download(ctx: State<OSSContext>, Path(hash): Path<String>, headers: HeaderMap) -> Response {
	let path = ctx.data_dir.join(&hash);
	let file = FileRangeReadr::new_hashed(path, hash, "image/png");
	if let Ok(file) = file.await {
		let mut response = send_range(headers, file).await;
		response.headers_mut().append(CACHE_CONTROL, HeaderValue::from_static(IMMUTABLE));
		return response;
	}
	return StatusCode::INTERNAL_SERVER_ERROR.into_response();
}
