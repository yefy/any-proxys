use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_header_parse::http_headers_size;
use crate::proxy::http_proxy::http_stream_request::{HttpBodyBufFilter, HttpStreamRequest};
use crate::proxy::http_proxy::stream_write;
use crate::proxy::http_proxy::util::write_body_to_client;
use crate::proxy::stream_stream::StreamStream;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::io::async_write_msg::MsgWriteBufBytes;
use any_base::stream_channel_read;
use any_base::stream_flow::{StreamFlow, StreamFlowErr, StreamFlowInfo};
use any_base::stream_nil_read;
use any_base::stream_nil_write;
use any_base::typ::{ArcMutex, ArcRwLockTokio};
use any_base::util::HttpHeaderExt;
use anyhow::anyhow;
use anyhow::Result;
use hyper::http::{HeaderValue, Response};
use hyper::{AnyProxyHyperBuf, AnyProxyRawHeaders, Body, Version};
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

pub async fn http_filter_header(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!(target: "ext2", "r.session_id:{}, http_filter_header", r.session_id);
    do_http_filter_header(&r).await?;

    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_header(r: &Arc<HttpStreamRequest>) -> Result<()> {
    let stream_info = r.http_arg.stream_info.clone();
    let (response, client_write, left_content_length) = {
        let rctx = &mut *r.ctx.get_mut();
        if rctx.r_in.version == Version::HTTP_10 {
            rctx.r_out.headers.insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        let cache_file_status = if rctx.http_cache_file.is_some() {
            rctx.http_cache_file
                .ctx_thread
                .get()
                .cache_file_status
                .clone()
        } else {
            None
        };

        if rctx.r_in.main.http_cache_status.is_none() {
            rctx.r_in.main.http_cache_status = Some(rctx.r_in.http_cache_status.clone());
            rctx.r_in.main.cache_file_status = cache_file_status.clone();
            rctx.r_in.main.is_slice = rctx.r_in.is_slice;
        }

        log::debug!(target: "ext", "r.session_id:{}-{}, http_cache_status:{:?}---{:?}, \
    cache_file_status:{:?}, is_upstream:{}, last_slice_upstream_index:{}, max_upstream_count:{}",
                        r.session_id, r.local_cache_req_count,
                        rctx.r_in.main.http_cache_status, rctx.r_in.http_cache_status,
                        cache_file_status,
                        rctx.is_upstream, rctx.last_slice_upstream_index, rctx.max_upstream_count);

        if rctx.is_request_cache && rctx.r_out_main.is_some() {
            let response_info = rctx.r_out.response_info.as_ref().unwrap();
            let r_out_main = rctx.r_out_main.as_ref().unwrap();
            let res_info_main = r_out_main.response_info.as_ref().unwrap();
            if res_info_main.last_modified != response_info.last_modified
                || res_info_main.last_modified_time != response_info.last_modified_time
                || res_info_main.e_tag != response_info.e_tag
            {
                return Err(anyhow!("rctx.r_out_main.is_some !="));
            }

            if !rctx.is_out_status_ok() {
                return Err(anyhow!("rctx.r_out_main.is_some !rctx.is_out_status_ok"));
            }

            log::trace!(target: "ext", "r.session_id:{}-{}, disable header",
                            r.session_id, r.local_cache_req_count);

            return Ok(());
        }

        rctx.r_out_main = Some(rctx.r_out.clone());

        let left_content_length = if rctx.is_request_cache {
            rctx.r_in.left_content_length
        } else {
            rctx.r_out.left_content_length
        };

        let (client_write, _res_body) = Body::channel();
        let mut response = if left_content_length <= 0 && rctx.r_out.transfer_encoding.is_none() {
            Response::builder().body(Body::empty())?
        } else {
            Response::builder().body(_res_body)?
        };

        *response.version_mut() = rctx.r_out.version;
        *response.status_mut() = rctx.r_out.status;
        *response.headers_mut() = rctx.r_out.headers.clone();
        let head = rctx.r_out.head.clone();
        if head.is_some() {
            let head = head.unwrap();
            response
                .extensions_mut()
                .insert(AnyProxyRawHeaders(AnyProxyHyperBuf(head.clone())));
        }
        response
            .extensions_mut()
            .insert(hyper::AnyProxyRawHttpHeaderExt(
                hyper::AnyProxyHyperHttpHeaderExt(HttpHeaderExt::new(
                    rctx.hyper_http_sent_bytes.clone(),
                )),
            ));

        // if rctx.is_request_cache && rctx.is_out_status_err() {
        //     response.headers_mut().remove(http::header::CONTENT_RANGE);
        //     response.headers_mut().remove(http::header::CONTENT_LENGTH);
        //     response.headers_mut().remove(http::header::CACHE_CONTROL);
        //     response.headers_mut().remove(http::header::ETAG);
        //     response.headers_mut().remove(http::header::EXPIRES);
        //     response.headers_mut().remove(http::header::LAST_MODIFIED);
        // }

        match response.version() {
            Version::HTTP_09 => {
                response.headers_mut().remove(http::header::CONNECTION);
            }
            Version::HTTP_10 => {
                response.headers_mut().insert(
                    http::header::CONNECTION,
                    HeaderValue::from_bytes(b"keep-alive")?,
                );
            }
            Version::HTTP_11 => {
                response.headers_mut().insert(
                    http::header::CONNECTION,
                    HeaderValue::from_bytes(b"keep-alive")?,
                );
            }
            Version::HTTP_2 => {
                response.headers_mut().remove(http::header::CONNECTION);
            }
            Version::HTTP_3 => {
                response.headers_mut().remove(http::header::CONNECTION);
            }
            _ => {}
        }

        rctx.r_out.head_size = http_headers_size(None, Some(response.status()), response.headers());
        (response, client_write, left_content_length)
    };

    log::debug!(target: "ext3", "r.session_id:{}-{}, to client response:{:#?}",
                r.session_id, r.local_cache_req_count, response);

    stream_info.get_mut().err_status = response.status().as_u16() as usize;
    let header_response = r.ctx.get().header_response.clone();
    if let Err(_) = header_response.send_header(response).await {
        return Err(anyhow!("response_tx.send"));
    }
    {
        let rctx = &mut *r.ctx.get_mut();
        if left_content_length <= 0 && rctx.r_out.transfer_encoding.is_none() {
            use chrono::Local;
            let stream_info = stream_info.get();
            stream_info.upstream_stream_flow_info.get_mut().err = StreamFlowErr::ReadClose;
            stream_info
                .upstream_stream_flow_info
                .get_mut()
                .err_time_millis = Local::now().timestamp_millis();

            log::trace!(target: "ext", "r.session_id:{}-{}, disable create body stream",
                        r.session_id, r.local_cache_req_count);
            return Ok(());
        }

        let upstream_stream_flow_info = ArcMutex::new(StreamFlowInfo::new());
        let (client_write_tx, client_stream) = {
            let stream_info = stream_info.get();
            let (stream_tx, stream_rx) = stream_channel_read::Stream::bounded(1);
            let stream_write = stream_write::Stream::new(client_write);

            let mut read_stream =
                StreamFlow::new2(stream_rx, stream_nil_write::Stream::new(), None);
            //这里是统计回源和本地文件的流量
            //read_stream.set_stream_info(Some(stream_info.upstream_stream_flow_info.clone()));
            read_stream.set_stream_info(Some(upstream_stream_flow_info.clone()));
            let mut write_stream =
                StreamFlow::new2(stream_nil_read::Stream::new(), stream_write, None);
            write_stream.set_stream_info(Some(stream_info.client_stream_flow_info.clone()));

            let client_stream = StreamFlow::new2(read_stream, write_stream, None);
            //let mut client_stream = StreamFlow::new2(stream_rx, stream_write, None);
            //client_stream.set_stream_info(Some(stream_info.client_stream_flow_info.clone()));
            (stream_tx, client_stream)
        };

        rctx.client_write_tx = Some(client_write_tx);
        let is_client_sendfile = rctx.is_client_sendfile;
        let scc = r.http_arg.stream_info.get().scc.clone();

        let mut executor = ExecutorLocalSpawn::new(r.http_arg.executors.clone());
        executor._start(
            #[cfg(feature = "anyspawn-count")]
            Some(format!("{}:{}", file!(), line!())),
            move |_| async move {
                let session_id = stream_info.get().session_id;
                let ret = StreamStream::stream_single(
                    &scc,
                    &stream_info,
                    client_stream,
                    is_client_sendfile,
                    true,
                )
                .await
                .map_err(|e| {
                    anyhow!(
                        "err:stream_to_stream => request_id:{}, e:{}",
                        stream_info.get().request_id,
                        e
                    )
                });
                if let Err(e) = ret {
                    let stream_info = stream_info.get();
                    // stream_info.upstream_stream_flow_info.get_mut().err =
                    //     upstream_stream_flow_info.get().err.clone();
                    // stream_info
                    //     .upstream_stream_flow_info
                    //     .get_mut()
                    //     .err_time_millis = upstream_stream_flow_info.get().err_time_millis;

                    let is_close = upstream_stream_flow_info.get().is_close()
                        || stream_info.client_stream_flow_info.get().is_close();
                    if !is_close {
                        log::error!("err:session_id:{} => err:{} ", session_id, e);
                        return Err(e);
                    }
                }
                Ok(())
            },
        );

        rctx.executor_client_write = Some(executor);
    }

    send_multipart_range_body(r).await?;

    return Ok(());
}

pub async fn send_multipart_range_body(r: &Arc<HttpStreamRequest>) -> Result<()> {
    let (body, client_write_tx) = {
        let rctx = &mut *r.ctx.get_mut();
        if rctx.r_in.multipart_range.is_none() {
            return Ok(());
        }
        let multipart_range = &mut rctx.r_in.multipart_range;
        if multipart_range.body.len() <= 0 {
            return Err(anyhow!("multipart_range.body.len() <= 0"));
        }
        let body = multipart_range.body.pop_front().unwrap();
        (body, rctx.client_write_tx.take())
    };

    if client_write_tx.is_none() {
        return Err(anyhow!("client_write_tx.is_none()"));
    }

    let seek = 0;
    let size = body.len() as u64;
    let body_buf = Some(HttpBodyBufFilter::from_bytes(
        MsgWriteBufBytes::from_bytes(body.into()),
        seek,
        size,
    ));

    log::trace!(target: "ext3", "r.session_id:{}-{}, send_multipart_range_body size:{}",
                r.session_id, r.local_cache_req_count, size);
    let mut client_write_tx = client_write_tx.unwrap();
    write_body_to_client(r, body_buf, &mut client_write_tx).await?;
    r.ctx.get_mut().client_write_tx = Some(client_write_tx);
    Ok(())
}
