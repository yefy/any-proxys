use super::http_stream::HttpStream;
use super::stream_write;
use crate::proxy::http_proxy::http_connection;
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::{ServerArg, StreamConfigContext};
use any_base::io::async_write_msg::{AsyncWriteMsgExt, MsgReadBufFile};
use any_base::io::buf_reader::BufReader;
use any_base::stream_flow::StreamFlow;
use any_base::stream_msg_read;
use any_base::typ::{ArcMutex, ShareRw};
use anyhow::anyhow;
use anyhow::Result;
use chrono::{DateTime, Utc};
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Response, StatusCode, Version};
use hyper::Body;

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
        let status = StatusCode::NOT_FOUND;
        let method = self.req.method();
        let ret = match method {
            &http::Method::GET => self.do_run().await,
            &http::Method::HEAD => self.do_run().await,
            _ => Err(anyhow!("method:{}", method)),
        };

        if let Err(e) = ret {
            log::error!("err:run => e:{}", e);
        } else {
            return Ok(ret.unwrap());
        }
        return Ok(Response::builder().status(status).body(Body::default())?);
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

        let is_head = self.req.method() == &http::Method::HEAD;

        let socket_fd = {
            let stream_info = self.http_arg.stream_info.get();
            stream_info.server_stream_info.raw_fd
        };

        let (is_open_sendfile, _directio, file_name) = {
            let scc = self.scc.get();
            let http_core_conf = scc.http_core_conf();
            let is_open_sendfile = http_core_conf.is_open_sendfile;
            let _directio = http_core_conf.directio;

            use crate::config::http_server_static;
            let http_server_static_conf = http_server_static::currs_conf(scc.http_server_confs());
            let mut seq = "";
            let mut name = self.req.uri().path();

            log::trace!("name:{}", name);
            if name.len() <= 0 || name == "/" {
                seq = "/";
                name = &http_server_static_conf.conf.index;
            }

            let file_name = format!("{}{}{}", http_server_static_conf.conf.path, seq, name);
            (is_open_sendfile, _directio, file_name)
        };

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

        let mut is_range = false;
        let mut content_length = file_len;
        let mut range1 = 0;
        let mut range2 = file_len - 1;

        let range_ret: Result<()> = async {
            let range = self.req.headers().get("Range");
            if range.is_none() {
                return Ok(());
            }

            let range = range.unwrap().to_str();
            if range.is_err() {
                return Ok(());
            }
            let range = range.unwrap();

            if range.len() < 6 || &range[0..6] != "bytes=" {
                return Ok(());
            }

            let range = &range[6..];
            if range.len() <= 0 {
                return Ok(());
            }

            is_range = true;
            let v = range.split_once("-");
            if v.is_none() {
                // HTTP/1.1 416 Requested Range Not Satisfiable
                return Err(anyhow!(""));
            }
            let (v1, v2) = v.unwrap();
            let v1 = v1.trim();
            let v2 = v2.trim();

            let vv1 = v1.trim().parse::<u64>();
            let vv2 = v2.trim().parse::<u64>();
            if vv1.is_err() && vv2.is_err() {
                // HTTP/1.1 416 Requested Range Not Satisfiable
                return Err(anyhow!(""));
            }

            if vv1.is_ok() {
                let vv1 = vv1?;
                if vv1 >= file_len {
                    // HTTP/1.1 416 Requested Range Not Satisfiable
                    return Err(anyhow!(""));
                }
                range1 = vv1;
            } else {
                let vv2 = vv2?;
                if vv2 > file_len {
                    // HTTP/1.1 416 Requested Range Not Satisfiable
                    return Err(anyhow!(""));
                }
                range1 = file_len - vv2;
                range2 = file_len - 1;
                content_length = vv2;
                return Ok(());
            }

            if vv2.is_ok() {
                let vv2 = vv2?;
                if vv2 >= file_len {
                    // HTTP/1.1 416 Requested Range Not Satisfiable
                    return Err(anyhow!(""));
                }
                range2 = vv2;
            } else {
                if v2.len() > 0 {
                    // HTTP/1.1 416 Requested Range Not Satisfiable
                    return Err(anyhow!(""));
                }
            }

            if range1 > range2 {
                // HTTP/1.1 416 Requested Range Not Satisfiable
                return Err(anyhow!(""));
            }
            content_length = range2 - range1 + 1;
            return Ok(());
        }
        .await;

        #[cfg(unix)]
        if _directio > 0 && _directio >= file_len {
            use any_base::util::directio_on;
            directio_on(file_fd)?;
        }

        let file = ArcMutex::new(file);

        let (res_sender, res_body) = Body::channel();
        let mut res = if range_ret.is_err() || is_head {
            Response::new(Body::default())
        } else {
            Response::new(res_body)
        };
        res.headers_mut().insert(
            "Content-Length",
            HeaderValue::from_bytes(content_length.to_string().as_bytes())?,
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
            "Connection",
            HeaderValue::from_bytes("keep-alive".as_bytes())?,
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

        if range_ret.is_err() {
            res.headers_mut().insert(
                "Content-Range",
                HeaderValue::from_bytes(format!("bytes */{}", file_len).as_bytes())?,
            );
            *res.status_mut() = StatusCode::RANGE_NOT_SATISFIABLE;
            return Ok(res);
        }

        res.headers_mut().insert(
            "Last-Modified",
            HeaderValue::from_bytes(last_modified.as_bytes())?,
        );

        res.headers_mut()
            .insert("ETag", HeaderValue::from_bytes(e_tag.as_bytes())?);
        if is_range {
            res.headers_mut().insert(
                "Content-Range",
                HeaderValue::from_bytes(
                    format!("bytes {}-{}/{}", range1, range2, file_len).as_bytes(),
                )?,
            );
            *res.status_mut() = StatusCode::PARTIAL_CONTENT;
        } else {
            res.headers_mut().insert(
                "Accept-Ranges",
                HeaderValue::from_bytes("bytes".as_bytes())?,
            );
            *res.status_mut() = StatusCode::OK;
        }

        if is_head {
            return Ok(res);
        }

        let http_arg = self.http_arg.clone();
        let scc = self.scc.clone();

        self.http_arg.executors._start(
            #[cfg(feature = "anyspawn-count")]
            Some(format!("{}:{}", file!(), line!())),
            move |_| async move {
                let (mut stream_tx, stream_rx) = stream_msg_read::Stream::bounded(100);
                let stream_write = stream_write::Stream::new(res_sender);
                let client_stream = StreamFlow::new2(stream_rx, stream_write, None);
                let is_sendfile =
                    if is_client_sendfile && socket_fd > 0 && is_open_sendfile && file_fd > 0 {
                        true
                    } else {
                        false
                    };

                let mut buf_file =
                    MsgReadBufFile::new(file.clone(), file_fd, range1, content_length);
                loop {
                    if !buf_file.has_remaining() {
                        break;
                    }
                    let (size, msg) = buf_file.to_msg_write_buf();
                    buf_file.advance(size);
                    stream_tx.write_msg(msg).await?;
                }

                drop(stream_tx);
                HttpStream::new(http_arg, scc, is_sendfile)
                    .start(client_stream)
                    .await
            },
        );

        Ok(res)
    }
}
