use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_header_parse::http_headers_size;
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::http_proxy::stream_write;
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::stream_stream::StreamStream;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::stream_channel_read;
use any_base::stream_flow::{StreamFlow, StreamFlowInfo};
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
    log::trace!("r.session_id:{}, http_filter_header", r.session_id);
    do_http_filter_header(&r).await?;

    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_header(r: &HttpStreamRequest) -> Result<()> {
    let (response, client_write) = {
        let r_ctx = &mut *r.ctx.get_mut();
        if r_ctx.r_in.version == Version::HTTP_10 {
            r_ctx.r_out.headers.insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        let cache_file_status = if r.http_cache_file.is_some() {
            r.http_cache_file.ctx_thread.get().cache_file_status.clone()
        } else {
            None
        };

        if r_ctx.r_in.main.http_cache_status.is_none() {
            r_ctx.r_in.main.http_cache_status = Some(r_ctx.r_in.http_cache_status.clone());
            r_ctx.r_in.main.cache_file_status = cache_file_status.clone();
            r_ctx.r_in.main.is_slice = r_ctx.r_in.is_slice;
        }

        log::debug!(target: "ext", "r.session_id:{}-{}, http_cache_status:{:?}---{:?}, \
    cache_file_status:{:?}, is_upstream:{}, last_slice_upstream_index:{}, max_upstream_count:{}",
                    r.session_id, r.local_cache_req_count,
                    r_ctx.r_in.main.http_cache_status, r_ctx.r_in.http_cache_status,
                    cache_file_status,
                    r_ctx.is_upstream, r_ctx.last_slice_upstream_index, r_ctx.max_upstream_count);

        if r_ctx.r_out_main.is_some() {
            let response_info = r_ctx.r_out.response_info.as_ref().unwrap();
            let r_out_main = r_ctx.r_out_main.as_ref().unwrap();
            let res_info_main = r_out_main.response_info.as_ref().unwrap();
            if res_info_main.last_modified != response_info.last_modified
                || res_info_main.last_modified_time != response_info.last_modified_time
                || res_info_main.e_tag != response_info.e_tag
            {
                return Err(anyhow!("r_ctx.r_out_main.is_some !="));
            }

            if !r_ctx.is_out_status_ok() {
                return Err(anyhow!("r_ctx.r_out_main.is_some !r_ctx.is_out_status_ok"));
            }

            log::trace!(target: "ext", "r.session_id:{}-{}, disable header",
                        r.session_id, r.local_cache_req_count);
            return Ok(());
        }

        r_ctx.r_out_main = Some(r_ctx.r_out.clone());

        let (client_write, _res_body) = Body::channel();
        let mut response = Response::builder().body(_res_body)?;
        *response.version_mut() = r_ctx.r_out.version;
        *response.status_mut() = r_ctx.r_out.status;
        *response.headers_mut() = r_ctx.r_out.headers.clone();
        let head = r_ctx.r_out.head.clone();
        if head.is_some() {
            let head = head.unwrap();
            response
                .extensions_mut()
                .insert(AnyProxyRawHeaders(AnyProxyHyperBuf(head.clone())));
        }
        response
            .extensions_mut()
            .insert(hyper::AnyProxyRawHttpHeaderExt(
                hyper::AnyProxyHyperHttpHeaderExt(HttpHeaderExt::new()),
            ));

        if !r_ctx.is_out_status_ok() {
            response.headers_mut().remove(http::header::CONTENT_RANGE);
            response.headers_mut().remove(http::header::CONTENT_LENGTH);
            return Ok(());
        }

        r_ctx.r_out.head_size =
            http_headers_size(None, Some(response.status()), response.headers());
        (response, client_write)
    };

    log::debug!(target: "ext3", "r.session_id:{}-{}, to client response:{:#?}",
                r.session_id, r.local_cache_req_count, response);

    let header_response = r.ctx.get().header_response.clone();
    if let Err(_) = header_response.send_header(response).await {
        return Err(anyhow!("response_tx.send"));
    }

    let r_ctx = &mut *r.ctx.get_mut();
    if !r_ctx.is_out_status_ok() {
        log::trace!(target: "ext", "r.session_id:{}-{}, header !r_ctx.is_out_status_ok() disable create body stream",
                    r.session_id, r.local_cache_req_count);
        return Ok(());
    }

    let stream_info = r.http_arg.stream_info.clone();
    let upstream_stream_flow_info = ArcMutex::new(StreamFlowInfo::new());
    let (client_write_tx, client_stream) = {
        let stream_info = stream_info.get();
        let (stream_tx, stream_rx) = stream_channel_read::Stream::bounded(1);
        let stream_write = stream_write::Stream::new(client_write);

        let mut read_stream = StreamFlow::new2(stream_rx, stream_nil_write::Stream::new(), None);
        //这里是统计回源和本地文件的流量
        //read_stream.set_stream_info(Some(stream_info.upstream_stream_flow_info.clone()));
        read_stream.set_stream_info(Some(upstream_stream_flow_info.clone()));
        let mut write_stream = StreamFlow::new2(stream_nil_read::Stream::new(), stream_write, None);
        write_stream.set_stream_info(Some(stream_info.client_stream_flow_info.clone()));

        let client_stream = StreamFlow::new2(read_stream, write_stream, None);
        //let mut client_stream = StreamFlow::new2(stream_rx, stream_write, None);
        //client_stream.set_stream_info(Some(stream_info.client_stream_flow_info.clone()));
        (stream_tx, client_stream)
    };

    r_ctx.client_write_tx = Some(client_write_tx);
    stream_info.get_mut().err_status = ErrStatus::Ok;
    let is_client_sendfile = r_ctx.is_client_sendfile;
    let scc = r.scc.clone();

    let mut executor = ExecutorLocalSpawn::new(r.http_arg.executors.clone());
    executor._start(
        #[cfg(feature = "anyspawn-count")]
        None,
        move |_| async move {
            let session_id = stream_info.get().session_id;
            let ret = StreamStream::stream_single(
                scc,
                stream_info.clone(),
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

    r_ctx.executor_client_write = Some(executor);
    return Ok(());
}
