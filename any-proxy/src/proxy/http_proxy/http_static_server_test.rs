use crate::proxy::http_proxy::http_connection;
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::{ServerArg, StreamConfigContext};
use any_base::io::async_write_msg::{MsgReadBufFile, MsgWriteBuf};
use any_base::io::buf_reader::BufReader;
use any_base::stream_flow::StreamFlow;
use any_base::typ::{ArcMutex, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Response, StatusCode, Version};
use hyper::Body;
use std::io::Read;

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );

    http_connection(arg, client_buf_stream, |arg, http_arg, scc, req| {
        Box::pin(http_server_run_handle(arg, http_arg, scc, req))
    })
    .await
}

pub async fn http_server_run_handle(
    arg: ServerArg,
    http_arg: ServerArg,
    scc: ShareRw<StreamConfigContext>,
    req: Request<Body>,
) -> Result<Response<Body>> {
    HttpStaticServer::new(arg, http_arg, req, scc).run().await
}

pub struct HttpStaticServer {
    _arg: ServerArg,
    http_arg: ServerArg,
    req: Request<Body>,
    scc: ShareRw<StreamConfigContext>,
}

impl HttpStaticServer {
    pub fn new(
        _arg: ServerArg,
        http_arg: ServerArg,
        req: Request<Body>,
        scc: ShareRw<StreamConfigContext>,
    ) -> HttpStaticServer {
        HttpStaticServer {
            _arg,
            http_arg,
            req,
            scc,
        }
    }

    pub async fn run(&mut self) -> Result<Response<Body>> {
        let ret = self.do_run().await;
        if let Err(e) = ret {
            log::error!("err:run => e:{}", e);
            return Ok(Response::builder()
                .status(StatusCode::NOT_FOUND)
                .body(Body::default())?);
        }
        ret
    }

    pub async fn do_run(&mut self) -> Result<Response<Body>> {
        self.http_arg.stream_info.get_mut().err_status = ErrStatus::Ok;
        let version = self.req.version();
        log::trace!("self.req.version:{:?}", version);
        log::trace!("self.req:{:?}", self.req);
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
        let http_core_conf = scc.http_core_conf();
        let is_open_sendfile = http_core_conf.is_open_sendfile;
        let sendfile_max_write_size = http_core_conf.sendfile_max_write_size;
        let _stream_nopush = http_core_conf.stream_nopush;
        let _directio = http_core_conf.directio;

        use crate::config::http_server_static_test;
        let http_server_static_test_conf =
            http_server_static_test::currs_conf(scc.http_server_confs());
        let mut seq = "";
        let mut name = self.req.uri().path();

        log::trace!("name:{}", name);
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
        if _directio > 0 && _directio >= file_len {
            use any_base::util::directio_on;
            directio_on(file_fd)?;
        }

        let file = ArcMutex::new(file);

        let (mut res_sender, res_body) = Body::channel();
        let mut res = Response::new(res_body);
        res.headers_mut().insert(
            "Content-Length",
            HeaderValue::from_bytes(file_len.to_string().as_bytes())?,
        );
        res.headers_mut().insert(
            "Server",
            HeaderValue::from_bytes("any-proxy/v2.0.0".as_bytes())?,
        );
        res.headers_mut().insert(
            "Content-Type",
            HeaderValue::from_bytes("text/plain".as_bytes())?,
        );
        res.headers_mut().insert(
            "Last-Modified",
            HeaderValue::from_bytes(last_modified.as_bytes())?,
        );
        res.headers_mut().insert(
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
        );
        res.headers_mut()
            .insert("ETag", HeaderValue::from_bytes(e_tag.as_bytes())?);
        res.headers_mut().insert(
            "Accept-Ranges",
            HeaderValue::from_bytes("bytes".as_bytes())?,
        );
        if *self.req.version_mut() == Version::HTTP_2 {
            res.headers_mut().remove("connection");
        }

        if *self.req.version_mut() == Version::HTTP_10 {
            res.headers_mut().insert(
                "Connection",
                HeaderValue::from_bytes("keep-alive".as_bytes())?,
            );
        }

        *res.version_mut() = *self.req.version_mut();

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

        Ok(res)
    }
}
