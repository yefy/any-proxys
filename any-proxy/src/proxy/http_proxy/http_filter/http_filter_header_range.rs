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
    log::trace!(target: "ext2", "r.session_id:{}, http_filter_header_range", r.session_id);
    do_http_filter_header_range(&r).await?;
    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_header_range(r: &HttpStreamRequest) -> Result<()> {
    if !r.ctx.get().is_request_cache {
        return Ok(());
    }

    let response_info = r.ctx.get().r_out.response_info.clone().unwrap();
    let ret = get_http_filter_header_range(r, response_info.range.raw_content_length).await;
    let rctx = &mut *r.ctx.get_mut();
    if ret.is_err() {
        rctx.r_out.headers.insert(
            "Content-Range",
            HeaderValue::from_bytes(
                format!("bytes */{}", response_info.range.raw_content_length).as_bytes(),
            )?,
        );
        rctx.r_out.status = StatusCode::RANGE_NOT_SATISFIABLE;
    } else {
        if !rctx.is_out_status_ok() {
            rctx.r_out.headers.insert(
                "Content-Length",
                HeaderValue::from_bytes(rctx.r_in.out_content_length.to_string().as_bytes())?,
            );
            return Ok(());
        }

        if rctx.r_in.range.is_range {
            if rctx.r_out.transfer_encoding.is_some() {
                log::error!("err:response_info.transfer_encoding.is_some()");
                return Err(anyhow::anyhow!(
                    "err:response_info.transfer_encoding.is_some()"
                ));
            }

            rctx.r_out.headers.remove("Accept-Ranges");
            rctx.r_out.headers.insert(
                "Content-Range",
                HeaderValue::from_bytes(
                    format!(
                        "bytes {}-{}/{}",
                        rctx.r_in.range.range_start,
                        rctx.r_in.range.range_end,
                        rctx.r_in.range.raw_content_length
                    )
                    .as_bytes(),
                )?,
            );
            rctx.r_out.status = StatusCode::PARTIAL_CONTENT;

            rctx.r_out.headers.insert(
                "Content-Length",
                HeaderValue::from_bytes(rctx.r_in.range.content_length.to_string().as_bytes())?,
            );
        } else {
            rctx.r_out.headers.remove("Content-Range");
            if rctx.r_out.transfer_encoding.is_none() {
                rctx.r_out.headers.insert(
                    "Content-Length",
                    HeaderValue::from_bytes(rctx.r_in.range.content_length.to_string().as_bytes())?,
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
    let rctx = &mut *r.ctx.get_mut();
    if rctx.r_in.is_load_range {
        return Ok(());
    }
    rctx.r_in.is_load_range = true;

    let mut range = get_http_range(&rctx.r_in.headers, raw_content_length)?;
    if !range.is_range && range.content_length > 0 {
        range.range_start = 0;
        range.range_end = range.content_length - 1;
    }

    if (rctx.r_in.is_head && rctx.r_out.status.is_success())
        || rctx.r_out.status == StatusCode::NOT_MODIFIED
        || rctx.r_out.status == StatusCode::PRECONDITION_FAILED
    {
        rctx.r_in.left_content_length = 0;
        rctx.r_in.out_content_length = 0;
    } else {
        rctx.r_in.left_content_length = range.content_length as i64;
        rctx.r_in.out_content_length = rctx.r_in.left_content_length;
    }
    // 100   0 99,  100, 199   5=> 0 99, 99 => 0 99,  100 => 100 199, 105  => 100 199 199 => 100 199
    rctx.r_in.slice_start = range.range_start / rctx.http_request_slice * rctx.http_request_slice;
    rctx.r_in.slice_end = range.range_end / rctx.http_request_slice * rctx.http_request_slice
        + rctx.http_request_slice
        - 1;
    if rctx.r_in.is_slice {
        rctx.r_in.curr_slice_start = rctx.r_in.slice_start;
        rctx.r_in.bitmap_curr_slice_start = rctx.r_in.slice_start;
        rctx.r_in.bitmap_last_slice_start = rctx.r_in.slice_start;
    } else {
        rctx.r_in.curr_slice_start = range.range_start;
        rctx.r_in.bitmap_curr_slice_start = range.range_start;
        rctx.r_in.bitmap_last_slice_start = range.range_start;
        rctx.r_in.skip_bitset_index =
            if rctx.r_in.bitmap_curr_slice_start % rctx.http_request_slice == 0 {
                -1
            } else {
                // 1  0,  99 0,  101 1,
                (rctx.r_in.bitmap_curr_slice_start / rctx.http_request_slice) as i64
            };
    }

    rctx.r_in.range = range;

    return Ok(());
}
