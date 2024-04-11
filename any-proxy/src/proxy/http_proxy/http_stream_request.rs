use crate::proxy::ServerArg;
use crate::proxy::StreamConfigContext;
use any_base::typ::{ArcMutex, ArcRwLock, OptionExt, ShareRw};
//use hyper::body::HttpBody;
use crate::config::net_core_proxy::CACHE_FILE_SLISE;
use crate::proxy::http_proxy::http_cache_file::HttpCacheFile;
use crate::proxy::http_proxy::http_header_parse::{http_headers_size, HttpParts};
use crate::proxy::http_proxy::http_hyper_connector::HttpHyperConnector;
use any_base::executor_local_spawn::ExecutorLocalSpawn;
use any_base::io::async_write_msg::{MsgReadBufFile, MsgWriteBufBytes};
use any_base::util::HttpHeaderExt;
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use http::{Extensions, Version};
use hyper::http::request::Parts;
use hyper::http::Request;
use hyper::{Body, Response};
use std::sync::atomic::{AtomicI64, Ordering};
use std::sync::Arc;

pub const LOCAL_CACHE_REQ_KEY: &'static str = "local_cache_req_key";

#[derive(Clone)]
pub enum HttpBodyBuf {
    Bytes(MsgWriteBufBytes),
    File(MsgReadBufFile),
}

impl HttpBodyBuf {
    pub fn from_bytes(buf: MsgWriteBufBytes) -> HttpBodyBuf {
        HttpBodyBuf::Bytes(buf)
    }

    pub fn from_file(buf: MsgReadBufFile) -> HttpBodyBuf {
        HttpBodyBuf::File(buf)
    }
}

#[derive(Clone)]
pub struct HttpBodyBufFilter {
    pub buf: HttpBodyBuf,
    pub seek: u64,
    pub size: u64,
}

impl HttpBodyBufFilter {
    pub fn new(buf: HttpBodyBuf, seek: u64, size: u64) -> HttpBodyBufFilter {
        HttpBodyBufFilter { buf, seek, size }
    }

    pub fn from_bytes(buf: MsgWriteBufBytes, seek: u64, size: u64) -> HttpBodyBufFilter {
        HttpBodyBufFilter {
            buf: HttpBodyBuf::Bytes(buf),
            seek,
            size,
        }
    }

    pub fn from_file(buf: MsgReadBufFile, seek: u64, size: u64) -> HttpBodyBufFilter {
        HttpBodyBufFilter {
            buf: HttpBodyBuf::File(buf),
            seek,
            size,
        }
    }

    pub fn to_bytes(self) -> Option<MsgWriteBufBytes> {
        if let HttpBodyBuf::Bytes(buf) = self.buf {
            return Some(buf);
        }
        return None;
    }

    pub fn to_file(self) -> Option<MsgReadBufFile> {
        if let HttpBodyBuf::File(buf) = self.buf {
            return Some(buf);
        }
        return None;
    }
}

#[derive(Debug, Clone)]
pub enum CacheFileStatus {
    Expire,
    Exist,
}

#[derive(Debug, Clone)]
pub enum HttpCacheStatus {
    Create,
    Miss,
    Hit,
    Bypass,
    Expired,
}

pub struct HttpInMain {
    pub http_cache_status: Option<HttpCacheStatus>,
    pub cache_file_status: Option<CacheFileStatus>,
    pub is_slice: bool,
}

impl HttpInMain {
    pub fn new() -> Self {
        HttpInMain {
            http_cache_status: None,
            cache_file_status: None,
            is_slice: false,
        }
    }
}

pub struct HttpIn {
    pub method: hyper::Method,
    pub uri: hyper::Uri,
    pub version: hyper::Version,
    pub headers: hyper::HeaderMap<http::HeaderValue>,

    pub method_upstream: hyper::Method,
    pub uri_upstream: hyper::Uri,
    pub version_upstream: hyper::Version,
    pub headers_upstream: hyper::HeaderMap<http::HeaderValue>,
    pub extensions_upstream: Option<Extensions>,
    pub body: Option<Body>,

    pub head_size: usize,
    pub head_upstream_size: usize,

    pub is_version1_upstream: bool,

    pub is_load_range: bool,
    pub range: HttpRange,
    pub cache_control_time: i64,
    pub is_head: bool,
    pub is_get: bool,
    pub left_content_length: i64,
    pub curr_slice_start: u64,
    pub bitmap_curr_slice_start: u64,
    pub bitmap_last_slice_start: u64,
    pub skip_bitset_index: i64,
    pub slice_start: u64,
    pub slice_end: u64,
    pub is_slice: bool,
    pub curr_request_count: usize,
    pub http_cache_status: HttpCacheStatus,
    pub main: HttpInMain,
    pub is_body_nil: bool,
    pub cur_slice_index: usize,
}

#[derive(Clone)]
pub struct HttpResponseInfo {
    pub last_modified_time: u64,
    pub last_modified: http::HeaderValue,
    pub e_tag: http::HeaderValue,
    pub cache_control_time: i64,
    //文件实际过期时间，创建文件生成的， 不能被改
    pub expires_time: u64,
    pub range: HttpRange,
    pub head: Option<Bytes>,
}

#[derive(Clone)]
pub struct HttpOut {
    pub status: hyper::StatusCode,
    pub version: hyper::Version,
    pub headers: hyper::HeaderMap<http::HeaderValue>,
    pub extensions: ArcMutex<Extensions>,
    pub status_upstream: hyper::StatusCode,
    pub version_upstream: hyper::Version,
    pub headers_upstream: hyper::HeaderMap<http::HeaderValue>,
    pub head: Option<Bytes>,

    pub head_size: usize,
    pub head_upstream_size: usize,

    pub is_cache: bool,
    pub is_cache_err: bool,
    pub response_info: Option<Arc<HttpResponseInfo>>,
    pub header_ext: HttpHeaderExt,
}

#[derive(Clone, Debug)]
pub struct HttpRange {
    pub is_range: bool,
    pub raw_content_length: u64,
    pub content_length: u64,
    pub range_start: u64,
    pub range_end: u64,
}

impl HttpRange {
    pub fn new() -> Self {
        HttpRange {
            is_range: false,
            raw_content_length: 0,
            content_length: 0,
            range_start: 0,
            range_end: 0,
        }
    }
}

pub struct HttpCacheFileRequest {
    pub response: Response<Body>,
    pub buf_file: Option<MsgReadBufFile>,
}

pub enum HttpRequest {
    Request(Request<Body>),
    CacheFileRequest(HttpCacheFileRequest),
}

pub struct HttpResponse {
    pub response: Response<Body>,
    pub body: HttpResponseBody,
}

pub enum HttpResponseBody {
    Body(Body),
    File(Option<MsgReadBufFile>),
}

pub struct UpstreamCountDrop {
    upstream_count: Option<Arc<AtomicI64>>,
}

impl UpstreamCountDrop {
    pub fn new(upstream_count: Option<Arc<AtomicI64>>) -> Self {
        UpstreamCountDrop { upstream_count }
    }
}

impl Drop for UpstreamCountDrop {
    fn drop(&mut self) {
        if self.upstream_count.is_some() {
            self.upstream_count
                .as_mut()
                .unwrap()
                .fetch_sub(1, Ordering::SeqCst);
        }
    }
}

pub struct HttpStreamRequestContext {
    pub r_in: HttpIn,
    pub r_out: HttpOut,
    pub r_out_main: Option<HttpOut>,
    pub in_body_buf: Option<HttpBodyBufFilter>,
    pub out_body_buf: Option<HttpBodyBufFilter>,
    pub response_tx: Option<tokio::sync::oneshot::Sender<Response<Body>>>,

    pub client_write_tx: Option<any_base::stream_channel_write::Stream>,
    pub executor_client_write: Option<ExecutorLocalSpawn>,
    pub is_client_sendfile: bool,
    pub is_upstream_sendfile: bool,
    pub is_request_cache: bool,
    pub is_upstream: bool,
    pub max_upstream_count: i64,
    pub upstream_count_drop: UpstreamCountDrop,
    pub slice_upstream_index: i32,
    pub last_slice_upstream_index: i32,
    pub is_try_cache: bool,
}

impl HttpStreamRequestContext {
    pub fn is_out_status_ok(&self) -> bool {
        self.r_out.status == http::StatusCode::OK
            || self.r_out.status == http::StatusCode::PARTIAL_CONTENT
    }
}

pub struct HttpStreamRequest {
    pub http_arg: ServerArg,
    pub scc: ShareRw<StreamConfigContext>,
    pub ctx: ArcRwLock<HttpStreamRequestContext>,
    pub http_cache_file: OptionExt<HttpCacheFile>,
    pub cache_file_slice: u64,
    pub http_request_slice: u64,
    pub arg: ServerArg,
    pub is_local_cache_req: bool,
    pub local_cache_req_count: usize,
    pub session_id: u64,
    pub header_ext: HttpHeaderExt,
    pub page_size: usize,
    pub client: OptionExt<Arc<hyper::Client<HttpHyperConnector>>>,
}

impl HttpStreamRequest {
    pub async fn new(
        arg: ServerArg,
        http_arg: ServerArg,
        scc: ShareRw<StreamConfigContext>,
        session_id: u64,
        response_tx: tokio::sync::oneshot::Sender<Response<Body>>,
        parts: HttpParts,
        request_upstream: Request<Body>,
        mut is_client_sendfile: bool,
        is_upstream_sendfile: bool,
        client: OptionExt<Arc<hyper::Client<HttpHyperConnector>>>,
    ) -> Result<HttpStreamRequest> {
        use crate::util::default_config::PAGE_SIZE;
        let page_size = PAGE_SIZE.load(Ordering::Relaxed);

        let is_version1_upstream = match request_upstream.version() {
            Version::HTTP_2 => false,
            _ => true,
        };

        let is_head = request_upstream.method() == &http::Method::HEAD;
        let is_get = request_upstream.method() == &http::Method::GET;
        let is_body_nil = match request_upstream.method() {
            &http::Method::GET => true,
            &http::Method::HEAD => true,
            &http::Method::OPTIONS => true,
            &http::Method::DELETE => true,
            &http::Method::TRACE => true,
            _ => false,
        };

        let local_cache_req_count = request_upstream.headers().get(LOCAL_CACHE_REQ_KEY);
        let local_cache_req_count = if local_cache_req_count.is_some() {
            let local_cache_req_count = local_cache_req_count.unwrap();
            let local_cache_req_count = local_cache_req_count.to_str()?.parse::<usize>()?;
            if local_cache_req_count > 1 {
                return Err(anyhow!("local_cache_req_count > 1"));
            }
            local_cache_req_count
        } else {
            0
        };

        log::debug!(
            "r.session_id:{}, local_cache_req_count:{}",
            session_id,
            local_cache_req_count
        );

        if local_cache_req_count > 0 {
            is_client_sendfile = false;
        }

        let header_ext = request_upstream
            .extensions()
            .get::<hyper::AnyProxyRawHttpHeaderExt>();
        let header_ext = if header_ext.is_some() {
            log::debug!("HttpStreamRequest header_ext:{}", local_cache_req_count);
            let header_ext = header_ext.unwrap();
            header_ext.0 .0.clone()
        } else {
            log::debug!("nil HttpStreamRequest header_ext:{}", local_cache_req_count);
            HttpHeaderExt::new()
        };

        use crate::config::net_core_proxy;
        let net_curr_conf = scc.get().net_curr_conf();
        let net_core_proxy_conf = net_core_proxy::curr_conf(&net_curr_conf);

        let http_request_slice = net_core_proxy_conf.proxy_request_slice;
        let cache_file_slice = CACHE_FILE_SLISE;

        let HttpParts {
            method,
            uri,
            version,
            headers,
        } = parts;

        let (parts, body) = request_upstream.into_parts();
        let Parts {
            method: method_upstream,
            uri: uri_upstream,
            version: version_upstream,
            headers: headers_upstream,
            extensions: extensions_upstream,
            ..
        } = parts;

        let head = extensions_upstream.get::<hyper::AnyProxyRawHeaders>();
        let head_size = if head.is_some() {
            let head = head.unwrap();
            head.0 .0.len()
        } else {
            http_headers_size(Some(&uri), None, &headers)
        };

        Ok(HttpStreamRequest {
            page_size,
            header_ext,
            session_id,
            http_arg,
            scc,
            client,
            ctx: ArcRwLock::new(HttpStreamRequestContext {
                r_in: HttpIn {
                    method,
                    uri,
                    version,
                    headers,

                    method_upstream,
                    uri_upstream,
                    version_upstream: version_upstream.clone(),
                    headers_upstream,
                    extensions_upstream: Some(extensions_upstream),
                    body: Some(body),
                    head_size,
                    head_upstream_size: 0,
                    is_version1_upstream,

                    is_load_range: false,
                    range: HttpRange::new(),
                    cache_control_time: -1,
                    is_head,
                    is_get,
                    left_content_length: -1,
                    curr_slice_start: 0,
                    bitmap_curr_slice_start: 0,
                    bitmap_last_slice_start: 0,
                    skip_bitset_index: -1,
                    slice_start: 0,
                    slice_end: 0,
                    is_slice: true,
                    curr_request_count: 0,
                    http_cache_status: HttpCacheStatus::Bypass,
                    main: HttpInMain::new(),
                    is_body_nil,
                    cur_slice_index: 0,
                },
                r_out: HttpOut {
                    status: http::StatusCode::OK,
                    version: version_upstream.clone(),
                    headers: hyper::HeaderMap::new(),
                    extensions: ArcMutex::default(),
                    status_upstream: http::StatusCode::OK,
                    version_upstream: version_upstream.clone(),
                    headers_upstream: hyper::HeaderMap::new(),
                    head: None,
                    head_size: 0,
                    head_upstream_size: 0,
                    is_cache: true,
                    is_cache_err: false,
                    response_info: None,
                    header_ext: HttpHeaderExt::new(),
                },
                r_out_main: None,
                in_body_buf: None,
                out_body_buf: None,
                response_tx: Some(response_tx),
                client_write_tx: None,
                executor_client_write: None,
                is_client_sendfile,
                is_upstream_sendfile,
                is_request_cache: false,
                is_upstream: false,
                max_upstream_count: 0,
                slice_upstream_index: -1,
                last_slice_upstream_index: -1,
                upstream_count_drop: UpstreamCountDrop::new(None),
                is_try_cache: false,
            }),
            http_cache_file: None.into(),
            cache_file_slice,
            http_request_slice,
            arg,
            is_local_cache_req: false,
            local_cache_req_count,
        })
    }
}
