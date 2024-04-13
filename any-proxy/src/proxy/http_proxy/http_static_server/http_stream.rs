use crate::proxy::http_proxy::http_cache_file_node::ProxyCacheFileNode;
use crate::proxy::http_proxy::http_header_parse::copy_request_parts;
use crate::proxy::http_proxy::http_stream_request::{
    HttpCacheStatus, HttpResponseBody, HttpStreamRequest,
};
use crate::proxy::http_proxy::{stream_send_err_head, HttpHeaderResponse};
use crate::proxy::proxy;
use crate::proxy::stream_info::{ErrStatus, StreamInfo};
use crate::proxy::stream_start;
use crate::proxy::util::{find_local, http_serverless, run_plugin_handle_http_serverless};
use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use any_base::io::async_write_msg::MsgReadBufFile;
use any_base::typ::Share;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use chrono::DateTime;
use chrono::Utc;
use http::HeaderValue;
use http::Version;
use hyper::http::{Request, Response};
use hyper::Body;
use std::sync::Arc;
//use std::time::UNIX_EPOCH;

#[allow(dead_code)]
pub struct HttpStream {
    arg: ServerArg,
    http_arg: ServerArg,
    scc: Arc<StreamConfigContext>,
    request: Option<Request<Body>>,
    header_response: Arc<HttpHeaderResponse>,
}

impl HttpStream {
    pub fn new(
        arg: ServerArg,
        http_arg: ServerArg,
        scc: Arc<StreamConfigContext>,
        request: Request<Body>,
        response_tx: async_channel::Sender<Response<Body>>,
    ) -> HttpStream {
        let header_response = Arc::new(HttpHeaderResponse::new(response_tx));
        HttpStream {
            arg,
            http_arg,
            scc,
            request: Some(request),
            header_response,
        }
    }

    pub async fn start(self) -> Result<()> {
        let stream_info = self.http_arg.stream_info.clone();
        let shutdown_thread_rx = self
            .http_arg
            .executors
            .context
            .shutdown_thread_tx
            .subscribe();
        let header_response = self.header_response.clone();
        let ret = stream_start::do_start(self, stream_info, shutdown_thread_rx).await;
        if ret.is_err() {
            let _ = stream_send_err_head(header_response).await;
        }
        ret
    }
}

#[async_trait]
impl proxy::Stream for HttpStream {
    async fn do_start(&mut self, stream_info: Share<StreamInfo>) -> Result<()> {
        stream_info.get_mut().add_work_time1("http_static_server");
        self.http_arg.stream_info.get_mut().err_status = ErrStatus::Ok;
        let scc = self.http_arg.stream_info.get().scc.clone().unwrap();

        let session_id = self.http_arg.stream_info.get().session_id;
        let request = self.request.take().unwrap();
        log::debug!(target: "ext3", "r.session_id:{}, client headers = {:#?}", session_id, request);
        let part = copy_request_parts(self.http_arg.stream_info.clone(), &request)?;

        let r = Arc::new(
            HttpStreamRequest::new(
                self.arg.clone(),
                self.http_arg.clone(),
                session_id,
                self.header_response.clone(),
                part,
                request,
                false,
                false,
                None.into(),
            )
            .await?,
        );
        r.ctx.get_mut().r_in.http_cache_status = HttpCacheStatus::Hit;
        r.http_arg.stream_info.get_mut().http_r = Some(r.clone()).into();
        let ms = scc.ms().clone();

        find_local(stream_info.clone())?;
        let scc = stream_info.get().scc.clone().unwrap();

        use crate::config::net_core_plugin;
        let http_core_plugin_main_conf = net_core_plugin::main_conf(&ms).await;

        let plugin_http_header_in = http_core_plugin_main_conf.plugin_http_header_in.get().await;
        (plugin_http_header_in)(r.clone()).await?;

        if run_plugin_handle_http_serverless(scc.clone(), stream_info.clone()).await? {
            return http_serverless(r, scc.clone()).await;
        }

        let ret = self.do_stream(r.clone()).await;
        if ret.is_err() {
            let header_response = self.header_response.clone();
            let _ = stream_send_err_head(header_response).await;
        }
        return ret;
    }
}

impl HttpStream {
    async fn do_stream(&mut self, r: Arc<HttpStreamRequest>) -> Result<()> {
        let scc = self.http_arg.stream_info.get().scc.clone();
        log::trace!("r.request.version:{:?}", r.ctx.get().r_in.version);
        let is_client_sendfile_version = match &r.ctx.get().r_in.version {
            &Version::HTTP_2 => false,
            &Version::HTTP_3 => false,
            _ => true,
        };

        let socket_fd = self.http_arg.stream_info.get().server_stream_info.raw_fd;
        let (
            is_open_sendfile,
            directio,
            file_name,
            plugin_http_header_filter,
            plugin_http_body_filter,
        ) = {
            use crate::config::net_core_plugin;
            let http_core_plugin_main_conf = net_core_plugin::main_conf(scc.ms()).await;
            let plugin_http_header_filter =
                http_core_plugin_main_conf.plugin_http_header_filter.clone();
            let plugin_http_body_filter =
                http_core_plugin_main_conf.plugin_http_body_filter.clone();

            let net_core_conf = scc.net_core_conf();

            let is_open_sendfile = net_core_conf.is_open_sendfile;
            let directio = net_core_conf.directio;

            use crate::config::net_server_static_http;
            let http_server_static_conf = net_server_static_http::curr_conf(scc.net_curr_conf());
            let mut seq = "";
            let r_ctx = &*r.ctx.get();
            let mut name = r_ctx.r_in.uri.path();

            log::trace!("name:{}", name);
            if name.len() <= 0 || name == "/" {
                seq = "/";
                name = &http_server_static_conf.conf.index;
            }

            let file_name = format!("{}{}{}", http_server_static_conf.conf.path, seq, name);
            (
                is_open_sendfile,
                directio,
                file_name,
                plugin_http_header_filter,
                plugin_http_body_filter,
            )
        };

        let file_name = ArcString::new(file_name);
        let file_ext = ProxyCacheFileNode::open_file(file_name.clone(), directio).await?;
        let metadata = file_ext.file.get().metadata().map_err(|e| {
            anyhow!(
                "err:file.metadata => file_name:{}, e:{}",
                file_name.as_str(),
                e
            )
        })?;
        let file_len = metadata.len();
        let modified = metadata.modified().map_err(|e| {
            anyhow!(
                "err:file.metadata => file_name:{}, e:{}",
                file_name.as_str(),
                e
            )
        })?;

        let last_modified = httpdate::HttpDate::from(modified.clone());
        let last_modified = last_modified.to_string();
        let datetime: DateTime<Utc> = modified.into();
        let e_tag = format!("{:x}-{:x}", datetime.timestamp(), file_len);
        {
            let r_ctx = &mut *r.ctx.get_mut();
            r_ctx
                .r_out
                .headers
                .insert(http::header::CONTENT_LENGTH, HeaderValue::from(file_len));
            r_ctx.r_out.headers.insert(
                "Last-Modified",
                HeaderValue::from_bytes(last_modified.as_bytes())?,
            );
            r_ctx
                .r_out
                .headers
                .insert("ETag", HeaderValue::from_bytes(e_tag.as_bytes())?);

            use crate::util::default_config;
            r_ctx.r_out.headers.insert(
                "Server",
                HeaderValue::from_bytes(default_config::HTTP_VERSION.as_bytes())?,
            );
            r_ctx.r_out.headers.insert(
                "Content-Type",
                HeaderValue::from_bytes("text/plain".as_bytes())?,
            );
            r_ctx.r_out.headers.insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );

            if r_ctx.r_in.version == Version::HTTP_2 {
                r_ctx.r_out.headers.remove("connection");
            } else if r_ctx.r_in.version == Version::HTTP_10 {
                r_ctx.r_out.headers.insert(
                    "Connection",
                    HeaderValue::from_bytes("keep-alive".as_bytes())?,
                );
            }

            r_ctx.r_out.version = r_ctx.r_in.version;
        }

        let is_sendfile = if is_client_sendfile_version
            && socket_fd > 0
            && is_open_sendfile
            && file_ext.is_sendfile()
        {
            true
        } else {
            false
        };
        r.ctx.get_mut().is_client_sendfile = is_sendfile;

        let plugin_http_header_filter = plugin_http_header_filter.get().await;
        (plugin_http_header_filter)(r.clone()).await?;

        if r.ctx.get().r_in.is_head {
            return Ok(());
        }

        let file = Arc::new(file_ext);
        let buf_file = MsgReadBufFile::new(file.clone(), 0, file_len);
        let upstream_body = HttpResponseBody::File(Some(buf_file));

        let client_write_tx = r.ctx.get_mut().client_write_tx.take();
        if client_write_tx.is_none() {
            log::trace!(target: "ext", "r.session_id:{}-{}, Response disable body", r.session_id, r.local_cache_req_count);
            return Ok(());
        }
        let mut client_write_tx = client_write_tx.unwrap();
        use crate::proxy::http_proxy::http_stream::HttpStream as _HttpStream;
        _HttpStream::stream_to_client(
            r.clone(),
            upstream_body,
            plugin_http_body_filter,
            &mut client_write_tx,
        )
        .await?;
        r.ctx.get_mut().client_write_tx = Some(client_write_tx);
        _HttpStream::stream_end_free(&r).await?;
        return Ok(());
    }
}
