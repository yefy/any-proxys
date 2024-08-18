use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_filter::http_filter_header::send_multipart_range_body;
use crate::proxy::http_proxy::http_header_parse::{get_http_multipart_range, if_range};
use crate::proxy::http_proxy::http_stream_request::{
    HttpMultipartRange, HttpMultipartRangeHeader, HttpRange, HttpStreamRequest,
};
use any_base::typ::ArcRwLockTokio;
use anyhow::Result;
use hyper::http::header::HeaderValue;
use hyper::http::StatusCode;
use lazy_static::lazy_static;
use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

lazy_static! {
    pub static ref MULTIPART_RANGE_ID: Arc<AtomicU64> = Arc::new(AtomicU64::new(1));
}

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub fn get_multipart_range_id() -> u64 {
    MULTIPART_RANGE_ID.fetch_add(1, Ordering::Relaxed)
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

pub async fn do_http_filter_header_range(r: &Arc<HttpStreamRequest>) -> Result<()> {
    if !r.ctx.get().is_request_cache {
        return Ok(());
    }

    let response_info = r.ctx.get().r_out.response_info.clone().unwrap();
    let ret = get_http_filter_header_range(r, response_info.range.raw_content_length).await;
    let rctx = &mut *r.ctx.get_mut();
    if ret.is_err() {
        rctx.r_out.headers.insert(
            http::header::CONTENT_RANGE,
            HeaderValue::from_bytes(
                format!("bytes */{}", response_info.range.raw_content_length).as_bytes(),
            )?,
        );
        rctx.r_out.status = StatusCode::RANGE_NOT_SATISFIABLE;
    } else {
        if !rctx.is_out_status_ok() {
            rctx.r_out.headers.insert(
                http::header::CONTENT_LENGTH,
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

            rctx.r_out.headers.remove(http::header::ACCEPT_RANGES);
            rctx.r_out.status = StatusCode::PARTIAL_CONTENT;

            if rctx.r_in.multipart_range_header.is_some() {
                let multipart_range_header = &rctx.r_in.multipart_range_header;
                rctx.r_out.headers.insert(
                    http::header::CONTENT_TYPE,
                    HeaderValue::from_bytes(multipart_range_header.header_value.as_bytes())?,
                );
                rctx.r_out.headers.insert(
                    http::header::CONTENT_LENGTH,
                    HeaderValue::from(multipart_range_header.body_len),
                );
                rctx.r_out.headers.remove(http::header::CONTENT_RANGE);
            } else {
                rctx.r_out.headers.insert(
                    http::header::CONTENT_RANGE,
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

                rctx.r_out.headers.insert(
                    http::header::CONTENT_LENGTH,
                    HeaderValue::from_bytes(rctx.r_in.range.content_length.to_string().as_bytes())?,
                );
            }
        } else {
            rctx.r_out.headers.remove(http::header::CONTENT_RANGE);
            if rctx.r_out.transfer_encoding.is_none() {
                rctx.r_out.headers.insert(
                    http::header::CONTENT_LENGTH,
                    HeaderValue::from_bytes(rctx.r_in.range.content_length.to_string().as_bytes())?,
                );
            }
        }
    }
    return Ok(());
}

pub async fn get_http_filter_header_range(
    r: &Arc<HttpStreamRequest>,
    mut raw_content_length: u64,
) -> Result<()> {
    let is_parse_range = {
        let rctx = &mut *r.ctx.get_mut();
        if rctx.r_in.is_load_range {
            return Ok(());
        }
        rctx.r_in.is_load_range = true;
        rctx.r_in.is_parse_range
    };

    let mut range = if !is_parse_range {
        let rctx = &mut *r.ctx.get_mut();
        rctx.r_in.is_parse_range = true;
        let (range, multipart_ranges) =
            get_http_multipart_range(&rctx.r_in.headers, raw_content_length)?;
        if multipart_ranges.is_some() {
            let multipart_range_id = get_multipart_range_id();
            let header_value =
                format!("multipart/byteranges; boundary={:0>20}", multipart_range_id);
            let mut multipart_ranges = multipart_ranges.unwrap();
            let mut body_len = 0;
            let mut body_header_len = 0;
            let mut multipart_bodys = VecDeque::with_capacity(multipart_ranges.len() + 1);
            for multipart_range in multipart_ranges.iter() {
                let multipart_body = format!("\r\n--{:0>20}\r\nContent-Type: application/octet-stream\r\nContent-Range: bytes {}-{}/{}\r\n\r\n",
                                             multipart_range_id, multipart_range.range_start, multipart_range.range_end, multipart_range.raw_content_length);
                let multipart_body_len =
                    multipart_body.len() as u64 + multipart_range.content_length;
                body_len += multipart_body_len;
                body_header_len += multipart_body.len() as u64;
                multipart_bodys.push_back(multipart_body);
            }
            let multipart_body_end = format!("\r\n--{:0>20}--\r\n", multipart_range_id);
            body_len += multipart_body_end.len() as u64;
            body_header_len += multipart_body_end.len() as u64;
            multipart_bodys.push_back(multipart_body_end);
            multipart_ranges.push_back(HttpRange {
                is_range: false,
                raw_content_length,
                content_length: 0,
                range_start: 0,
                range_end: 0,
            });
            let range = multipart_ranges.pop_front().unwrap();
            let multipart_range = HttpMultipartRange {
                ranges: multipart_ranges,
                body: multipart_bodys,
                body_header_len,
            };
            let multipart_range_header = HttpMultipartRangeHeader {
                body_len,
                header_value,
            };

            rctx.r_in.multipart_range = Some(multipart_range).into();
            rctx.r_in.multipart_range_header = Some(multipart_range_header).into();
            range
        } else {
            range
        }
    } else {
        let range = {
            let rctx = &mut *r.ctx.get_mut();
            if rctx.r_in.multipart_range.is_none() {
                return Err(anyhow::anyhow!("rctx.r_in.multipart_range.is_none"));
            } else {
                if rctx.r_in.multipart_range.ranges.len() <= 0 {
                    return Err(anyhow::anyhow!(
                        "rctx.r_in.multipart_range.ranges.len() <= 0"
                    ));
                } else {
                    let range = rctx.r_in.multipart_range.ranges.pop_front().unwrap();
                    if rctx.r_in.multipart_range.ranges.len() <= 0 {
                        raw_content_length = 0;
                    }
                    range
                }
            }
        };
        send_multipart_range_body(r).await?;
        range
    };

    log::trace!(target: "ext3", "r.session_id:{}-{}, range:{:?}",
                r.session_id, r.local_cache_req_count, range);

    let rctx = &mut *r.ctx.get_mut();
    if range.is_range {
        let (_, if_range_time) = if_range(&rctx.r_in.headers)?;
        let response_info = rctx.r_out.response_info.as_ref().unwrap();
        if if_range_time > 0 && if_range_time != response_info.last_modified_time {
            range.is_range = false;
            unsafe { rctx.r_in.multipart_range.take_op() };
            unsafe { rctx.r_in.multipart_range_header.take_op() };
        }
    }

    if !range.is_range {
        range.range_start = 0;
        range.range_end = 0;
        range.content_length = raw_content_length;
        if raw_content_length > 0 {
            range.range_end = raw_content_length - 1;
        }
    }

    if (rctx.r_in.is_head && rctx.r_out.status.is_success())
        || rctx.r_out.status == StatusCode::NOT_MODIFIED
        || rctx.r_out.status == StatusCode::PRECONDITION_FAILED
    {
        rctx.r_in.left_content_length = 0;
        rctx.r_in.out_content_length = 0;
        unsafe { rctx.r_in.multipart_range.take_op() };
    } else {
        rctx.r_in.left_content_length = range.content_length as i64;
        rctx.r_in.out_content_length = rctx.r_in.left_content_length;
    }

    if !rctx.r_out.status.is_success() {
        unsafe { rctx.r_in.multipart_range.take_op() };
        unsafe { rctx.r_in.multipart_range_header.take_op() };
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
