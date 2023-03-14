use axum::{Json, response::IntoResponse, Router};
use axum::extract::{BodyStream, Path, State};
use axum::http::HeaderMap;
use axum::response::Response;
use axum::routing::{get, post};
use log;

use crate::context::{OSSContext, UploadVO};
use crate::range::send_file_range;

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

struct ManualBucket {
	ctx: OSSContext,

}

async fn upload(ctx: State<OSSContext>, body: BodyStream) -> Response {
	let buf = ctx.receive_file(body).await.unwrap();
	let hash = buf.hash.clone();
	log::trace!("hash is {}", hash);

	buf.save().unwrap();
	return Json(UploadVO { hash }).into_response();
}

async fn download(ctx: State<OSSContext>, Path(hash): Path<String>, headers: HeaderMap) -> Response {
	let path = ctx.data_dir.join(hash);
	send_file_range(headers, path, String::from("image/png")).await
}

pub fn manual_bucket() -> Router<OSSContext> {
	return Router::new().route("/", post(upload)).route("/:hash", get(download));
}
