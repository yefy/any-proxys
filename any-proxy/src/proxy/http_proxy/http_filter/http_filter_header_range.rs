use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_header_parse::get_http_range;
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use any_base::typ::ArcRwLockTokio;
use anyhow::Result;
use hyper::http::header::HeaderValue;
use hyper::http::StatusCode;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub async fn set_header_filter(plugin: PluginHttpFilter) -> Result<()> {
    if HEADER_FILTER_NEXT.is_none().await {
        HEADER_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn http_filter_header_range(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!("r.session_id:{}, http_filter_header_range", r.session_id);
    do_http_filter_header_range(&r).await?;
    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_header_range(r: &HttpStreamRequest) -> Result<()> {
    let raw_content_length = {
        let r_ctx = &mut *r.ctx.get_mut();
        if !r_ctx.is_out_status_ok() {
            return Ok(());
        }
        r_ctx
            .r_out
            .response_info
            .as_ref()
            .unwrap()
            .range
            .raw_content_length
    };

    let ret = get_http_filter_header_range(r, raw_content_length).await;
    let r_ctx = &mut *r.ctx.get_mut();
    let res_info_out = r_ctx.r_out.response_info.as_ref().unwrap();
    if ret.is_err() {
        r_ctx.r_out.headers.insert(
            "Content-Range",
            HeaderValue::from_bytes(
                format!("bytes */{}", res_info_out.range.raw_content_length).as_bytes(),
            )?,
        );
        r_ctx.r_out.status = StatusCode::RANGE_NOT_SATISFIABLE;
    } else {
        if r_ctx.r_in.range.is_range {
            r_ctx.r_out.headers.insert(
                "Content-Range",
                HeaderValue::from_bytes(
                    format!(
                        "bytes {}-{}/{}",
                        r_ctx.r_in.range.range_start,
                        r_ctx.r_in.range.range_end,
                        r_ctx.r_in.range.raw_content_length
                    )
                    .as_bytes(),
                )?,
            );
            r_ctx.r_out.status = StatusCode::PARTIAL_CONTENT;

            r_ctx.r_out.headers.insert(
                "Content-Length",
                HeaderValue::from_bytes(r_ctx.r_in.range.content_length.to_string().as_bytes())?,
            );
        } else {
            r_ctx.r_out.headers.remove("Content-Range");
            r_ctx.r_out.headers.insert(
                "Accept-Ranges",
                HeaderValue::from_bytes("bytes".as_bytes())?,
            );
            r_ctx.r_out.status = StatusCode::OK;
            if res_info_out.range.content_length > 0 {
                r_ctx.r_out.headers.insert(
                    "Content-Length",
                    HeaderValue::from_bytes(
                        r_ctx.r_in.range.content_length.to_string().as_bytes(),
                    )?,
                );
            } else {
                r_ctx.r_out.headers.insert(
                    http::header::TRANSFER_ENCODING,
                    HeaderValue::from_bytes("chunked".as_bytes())?,
                );
            }
        }
    }
    return Ok(());
}

pub async fn get_http_filter_header_range(
    r: &HttpStreamRequest,
    raw_content_length: u64,
) -> Result<()> {
    let r_ctx = &mut *r.ctx.get_mut();
    if r_ctx.r_in.is_load_range {
        return Ok(());
    }
    r_ctx.r_in.is_load_range = true;

    let mut range = get_http_range(&r_ctx.r_in.headers, raw_content_length)?;
    if !range.is_range && range.content_length > 0 {
        range.range_start = 0;
        range.range_end = range.content_length - 1;
    }

    if r_ctx.r_in.is_head {
        r_ctx.r_in.left_content_length = 0;
    } else {
        r_ctx.r_in.left_content_length = range.content_length as i64;
    }
    // 100   0 99,  100, 199   5=> 0 99, 99 => 0 99,  100 => 100 199, 105  => 100 199 199 => 100 199
    r_ctx.r_in.slice_start = range.range_start / r.http_request_slice * r.http_request_slice;
    r_ctx.r_in.slice_end =
        range.range_end / r.http_request_slice * r.http_request_slice + r.http_request_slice - 1;
    if r_ctx.r_in.is_slice {
        r_ctx.r_in.curr_slice_start = r_ctx.r_in.slice_start;
        r_ctx.r_in.bitmap_curr_slice_start = r_ctx.r_in.slice_start;
        r_ctx.r_in.bitmap_last_slice_start = r_ctx.r_in.slice_start;
    } else {
        r_ctx.r_in.curr_slice_start = range.range_start;
        r_ctx.r_in.bitmap_curr_slice_start = range.range_start;
        r_ctx.r_in.bitmap_last_slice_start = range.range_start;
        r_ctx.r_in.skip_bitset_index =
            if r_ctx.r_in.bitmap_curr_slice_start % r.http_request_slice == 0 {
                -1
            } else {
                // 1  0,  99 0,  101 1,
                (r_ctx.r_in.bitmap_curr_slice_start / r.http_request_slice) as i64
            };
    }

    r_ctx.r_in.range = range;

    return Ok(());
}
