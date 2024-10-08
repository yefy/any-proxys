use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::http_proxy::HttpHeaderResponse;
use crate::proxy::ServerArg;
use anyhow::Result;
use hyper::http::{Response, StatusCode};
use hyper::Body;
use std::sync::Arc;

pub async fn server_handle(r: Arc<HttpStreamRequest>) -> Result<crate::Error> {
    let scc = r.http_arg.stream_info.get().scc.clone();
    use crate::config::net_server_http_static_test;
    let net_server_http_static_test_conf =
        net_server_http_static_test::curr_conf_mut(scc.net_curr_conf());
    if net_server_http_static_test_conf.conf.is_none() {
        return Ok(crate::Error::Ok);
    }

    HttpStream::new(&r).do_stream(&r).await?;
    return Ok(crate::Error::Finish);
}

pub struct HttpStream {
    pub arg: ServerArg,
    pub http_arg: ServerArg,
    pub header_response: Arc<HttpHeaderResponse>,
}

impl HttpStream {
    pub fn new(r: &Arc<HttpStreamRequest>) -> HttpStream {
        HttpStream {
            arg: r.arg.clone(),
            http_arg: r.http_arg.clone(),
            header_response: r.ctx.get().header_response.clone().unwrap(),
        }
    }
    async fn do_stream(&mut self, _r: &Arc<HttpStreamRequest>) -> Result<()> {
        let response = self.run().await?;
        if !self.header_response.is_send() {
            let _ = self.header_response.send_header(response).await;
        }

        Ok(())
    }

    pub async fn run(&mut self) -> Result<Response<Body>> {
        return Ok(Response::builder()
            .status(StatusCode::NOT_FOUND)
            .body(Body::default())?);
        /*
        self.http_arg.stream_info.get_mut().err_status = ErrStatus::Ok;
        let version = self.request.version();
        log::trace!(target: "main", "self.request.version:{:?}", version);
        log::trace!(target: "main", "self.request:{:?}", self.request);
        let is_client_sendfile = match version {
            Version::HTTP_2 => false,
            Version::HTTP_3 => false,
            _ => true,
        };

        let socket_fd = {
            let stream_info = self.http_arg.stream_info.get();
            stream_info.server_stream_info.raw_fd
        };

        let scc = self.scc.get();
        let net_core_conf = scc.net_core_conf();
        let is_open_sendfile = net_core_conf.is_open_sendfile;
        let sendfile_max_write_size = net_core_conf.sendfile_max_write_size;
        let _stream_nopush = net_core_conf.stream_nopush;
        let _directio = net_core_conf.directio;

        use crate::config::net_server_http_static_test;
        let http_server_static_test_conf =
            net_server_http_static_test::currs_conf(scc.net_server_confs());
        let mut seq = "";
        let mut name = self.request.uri().path();

        log::trace!(target: "main", "name:{}", name);
        if name.len() <= 0 || name == "/" {
            seq = "/";
            name = &http_server_static_test_conf.conf.index;
        }

        let file_name = format!("{}{}{}", http_server_static_test_conf.conf.path, seq, name);
        let file = std::fs::File::open(&file_name)
            .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;
        #[cfg(not(unix))]
        let file_fd = 0;
        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;
        #[cfg(unix)]
        let file_fd = file.as_raw_fd();

        let metadata = file
            .metadata()
            .map_err(|e| anyhow!("err:file.metadata => file_name:{}, e:{}", file_name, e))?;
        let file_len = metadata.len();
        let modified = metadata
            .modified()
            .map_err(|e| anyhow!("err:file.metadata => file_name:{}, e:{}", file_name, e))?;
        let last_modified = httpdate::HttpDate::from(modified.clone());
        let last_modified = last_modified.to_string();

        let datetime: DateTime<Utc> = modified.into();
        let e_tag = format!("{:x}-{:x}", datetime.timestamp(), file_len);

        #[cfg(unix)]
        if _directio > 0 && file_len >= _directio {
            use any_base::util::directio_on;
            directio_on(file_fd)?;
        }

        let file = ArcMutex::new(file);

        let (mut res_sender, res_body) = Body::channel();
        let mut response = Response::new(res_body);
        response.headers_mut().insert(
            "Content-Length",
            HeaderValue::from_bytes(file_len.to_string().as_bytes())?,
        );
        response.headers_mut().insert(
            "Server",
            HeaderValue::from_bytes("any-proxy/v2.0.0".as_bytes())?,
        );
        response.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_bytes("text/plain".as_bytes())?,
        );
        response.headers_mut().insert(
            "Last-Modified",
            HeaderValue::from_bytes(last_modified.as_bytes())?,
        );
        response.headers_mut().insert(
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
        );
        response.headers_mut()
            .insert("ETag", HeaderValue::from_bytes(e_tag.as_bytes())?);
        response.headers_mut().insert(
            "Accept-Ranges",
            HeaderValue::from_bytes("bytes".as_bytes())?,
        );
        if *self.request.version_mut() == Version::HTTP_2 {
            response.headers_mut().remove("connection");
        }

        if *self.request.version_mut() == Version::HTTP_10 {
            response.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        *response.version_mut() = *self.request.version_mut();

        self.http_arg.executors._start(
            #[cfg(feature = "anyspawn-count")]
            Some(format!("{}:{}", file!(), line!())),
            move |_| async move {
                if is_client_sendfile && socket_fd > 0 && is_open_sendfile && file_fd > 0 {
                    #[cfg(unix)]
                    if _stream_nopush {
                        use any_base::util::set_tcp_nopush_;
                        if socket_fd > 0 {
                            set_tcp_nopush_(socket_fd, true);
                        }
                    }
                    let mut buf_file = MsgReadBufFile::new(file.clone(), file_fd, 0, file_len);
                    loop {
                        if !buf_file.has_remaining() {
                            break;
                        }

                        let (size, msg) = buf_file.to_msg_write_buf2(sendfile_max_write_size);

                        buf_file.advance(size);

                        match res_sender.send_data(msg).await {
                            Ok(_) => {}
                            Err(e) => {
                                if !e.is_closed() {
                                    log::error!("err: sendfile => err:{}", e);
                                }
                                break;
                            }
                        }
                    }
                    return Ok(());
                }

                let buffer_len: usize = 65536;
                loop {
                    let file = file.clone();
                    let data: Result<(usize, Vec<u8>)> = tokio::task::spawn_blocking(move || {
                        let mut buffer = vec![0u8; buffer_len];
                        let size = file
                            .get_mut()
                            .read(&mut buffer.as_mut_slice()[..])
                            .map_err(|e| anyhow!("err:file.read => e:{}", e))?;
                        Ok((size, buffer))
                    })
                    .await?;
                    let (size, mut buffer) = data?;

                    if size > 0 {
                        if size != buffer_len {
                            unsafe { buffer.set_len(size) }
                        }
                        match res_sender.send_data(MsgWriteBuf::from_vec(buffer)).await {
                            Ok(_) => {}
                            Err(e) => {
                                if !e.is_closed() {
                                    log::error!("err: file_buffer => err:{}", e);
                                }
                                break;
                            }
                        }
                    }
                    if size != buffer_len {
                        break;
                    }
                }
                Ok(())
            },
        );

        Ok(response)

         */
    }
}
