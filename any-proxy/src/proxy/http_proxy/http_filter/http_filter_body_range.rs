use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_stream_request::{
    HttpBodyBuf, HttpBodyBufFilter, HttpStreamRequest,
};
use any_base::io::async_write_msg::{MsgReadBufFile, MsgWriteBufBytes};
use any_base::typ::ArcRwLockTokio;
use anyhow::Result;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref BODY_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub async fn set_body_filter(plugin: PluginHttpFilter) -> Result<()> {
    if BODY_FILTER_NEXT.is_none().await {
        BODY_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn http_filter_body_range(r: Arc<HttpStreamRequest>) -> Result<()> {
    do_http_filter_body_range(&r).await?;
    let next = BODY_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_body_range(r: &HttpStreamRequest) -> Result<()> {
    let r_ctx = &mut *r.ctx.get_mut();
    let in_body_buf = r_ctx.in_body_buf.take().unwrap();
    r_ctx.r_in.curr_slice_start += in_body_buf.size;

    let seek = in_body_buf.seek;
    let mut range_start = in_body_buf.seek;
    let mut range_end = range_start + in_body_buf.size - 1;

    //start <= end
    //0-1  1  2,    0-0  1 ,  1-1 2
    if range_start > r_ctx.r_in.range.range_end {
        return Ok(());
    }

    if range_end < r_ctx.r_in.range.range_start {
        return Ok(());
    }

    if range_start < r_ctx.r_in.range.range_start {
        range_start = r_ctx.r_in.range.range_start;
    }

    if range_end > r_ctx.r_in.range.range_end {
        range_end = r_ctx.r_in.range.range_end;
    }
    let out_body_buf = match in_body_buf.buf {
        HttpBodyBuf::Bytes(buf) => {
            let MsgWriteBufBytes { data } = buf;
            let index = range_start - seek;
            let seek = range_start;
            let size = range_end - range_start + 1;
            log::trace!(target: "ext3",
                        "r.session_id:{}-{}, bytes body_range seek:{}, size:{}, {}-{}",
                r.session_id, r.local_cache_req_count,
                seek,
                size,
                seek,
                seek + size - 1
            );
            HttpBodyBufFilter {
                buf: HttpBodyBuf::Bytes(MsgWriteBufBytes::from_bytes(
                    data.slice(index as usize..(index + size) as usize),
                )),
                seek,
                size,
            }
        }
        HttpBodyBuf::File(buf) => {
            let MsgReadBufFile {
                file_ext,
                seek: _,
                size: _,
            } = buf;

            let body_start = if r.http_cache_file.is_some() {
                let cache_file_node = r
                    .http_cache_file
                    .ctx_thread
                    .get()
                    .cache_file_node()
                    .unwrap();
                cache_file_node.fix.body_start as u64
            } else {
                0
            };

            let seek = range_start;
            let file_seek = seek + body_start;
            let size = range_end - range_start + 1;

            log::trace!(target: "ext3",
                        "r.session_id:{}-{}, file body_range seek:{}, file_seek:{}, size:{}, {}-{}, {}-{}",
                r.session_id,r.local_cache_req_count,
                seek,
                file_seek,
                size,
                seek,
                seek + size - 1,
                file_seek,
                file_seek + size - 1
            );
            HttpBodyBufFilter {
                buf: HttpBodyBuf::File(MsgReadBufFile::new(file_ext, file_seek, size)),
                seek,
                size,
            }
        }
    };
    r_ctx.r_in.left_content_length -= out_body_buf.size as i64;
    r_ctx.out_body_buf = Some(out_body_buf);
    return Ok(());
}
