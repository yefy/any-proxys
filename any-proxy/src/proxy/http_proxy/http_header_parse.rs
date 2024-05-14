use crate::proxy::http_proxy::http_stream_request::HttpRange;
use crate::proxy::stream_info::StreamInfo;
use crate::util::util::host_and_port;
use any_base::typ::Share;
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use http::header::{HeaderName, HeaderValue, HOST};
use http::{HeaderMap, Method, Response, StatusCode, Uri, Version};
use hyper::http::Request;
use hyper::{AnyProxyHyperBuf, AnyProxyRawHeaders, Body};
use log::debug;
use log::error;
use std::mem::MaybeUninit;
use std::time::{SystemTime, UNIX_EPOCH};

const MAX_HEADERS: usize = 100;

pub struct HttpParts {
    /// The request's method
    pub method: Method,

    /// The request's URI
    pub uri: Uri,

    /// The request's version
    pub version: Version,

    /// The request's headers
    pub headers: HeaderMap<HeaderValue>,
}

macro_rules! header_name {
    ($bytes:expr) => {{
        {
            match HeaderName::from_bytes($bytes) {
                Ok(name) => name,
                Err(e) => maybe_panic!(e),
            }
        }
    }};
}

macro_rules! header_value {
    ($bytes:expr) => {{
        {
            unsafe { HeaderValue::from_maybe_shared_unchecked($bytes) }
        }
    }};
}

macro_rules! maybe_panic {
    ($($arg:tt)*) => ({
        let _err = ($($arg)*);
        if cfg!(debug_assertions) {
            panic!("{:?}", _err);
        } else {
            error!("Internal Hyper error, please report {:?}", _err);
            return Err(ErrParse::Internal)
        }
    })
}

/// An Incoming Message head. Includes request/status line, and headers.
#[derive(Debug, Default)]
pub(crate) struct MessageHead<S> {
    /// HTTP version of the message.
    pub(crate) version: http::Version,
    /// Subject (request line or status line) of Incoming message.
    pub(crate) subject: S,
    /// Headers of the Incoming message.
    pub(crate) headers: http::HeaderMap,
    /// Extensions.
    extensions: http::Extensions,
}

impl MessageHead<http::StatusCode> {
    fn into_response<B>(self, body: B) -> http::Response<B> {
        let mut response = http::Response::new(body);
        *response.status_mut() = self.subject;
        *response.headers_mut() = self.headers;
        *response.version_mut() = self.version;
        *response.extensions_mut() = self.extensions;
        response
    }
}

pub fn http_parse(buf: &Bytes) -> std::result::Result<(Response<Body>, usize), ErrParse> {
    debug_assert!(!buf.is_empty(), "parse called with empty buf");

    let h1_parser_config = httparse::ParserConfig::default();
    let h09_responses = false;

    // Loop to skip information status code headers (100 Continue, etc).
    loop {
        // Unsafe: see comment in Server Http1Transaction, above.
        let mut headers_indices: [MaybeUninit<HeaderIndices>; MAX_HEADERS] = unsafe {
            // SAFETY: We can go safely from MaybeUninit array to array of MaybeUninit
            MaybeUninit::uninit().assume_init()
        };
        let (len, status, _reason, version, headers_len) = {
            // SAFETY: We can go safely from MaybeUninit array to array of MaybeUninit
            let mut headers: [MaybeUninit<httparse::Header<'_>>; MAX_HEADERS] =
                unsafe { MaybeUninit::uninit().assume_init() };

            let mut response = httparse::Response::new(&mut []);
            let bytes = buf.as_ref();
            match h1_parser_config.parse_response_with_uninit_headers(
                &mut response,
                bytes,
                &mut headers,
            ) {
                Ok(httparse::Status::Complete(len)) => {
                    log::trace!(target: "main", "Response.parse Complete({})", len);
                    let status = StatusCode::from_u16(response.code.unwrap())
                        .map_err(|_| ErrParse::Status)?;

                    let reason = {
                        let reason = response.reason.unwrap();
                        // Only save the reason phrase if it isn't the canonical reason
                        if Some(reason) != status.canonical_reason() {
                            Some(Bytes::copy_from_slice(reason.as_bytes()))
                        } else {
                            None
                        }
                    };

                    let version = if response.version.unwrap() == 1 {
                        Version::HTTP_11
                    } else {
                        Version::HTTP_10
                    };
                    record_header_indices(bytes, &response.headers, &mut headers_indices)?;
                    let headers_len = response.headers.len();
                    (len, status, reason, version, headers_len)
                }
                Ok(httparse::Status::Partial) => return Err(ErrParse::Internal),
                Err(httparse::Error::Version) if h09_responses => {
                    log::trace!(target: "main", "Response.parse accepted HTTP/0.9 response",);

                    (0, StatusCode::OK, None, Version::HTTP_09, 0)
                }
                Err(_) => return Err(ErrParse::Internal),
            }
        };
        let slice = buf.slice(0..len);
        // let mut slice = buf.split_to(len);
        //
        // if h1_parser_config
        //     .obsolete_multiline_headers_in_responses_are_allowed()
        // {
        //     for header in &headers_indices[..headers_len] {
        //         // SAFETY: array is valid up to `headers_len`
        //         let header = unsafe { &*header.as_ptr() };
        //         for b in &mut slice[header.value.0..header.value.1] {
        //             if *b == b'\r' || *b == b'\n' {
        //                 *b = b' ';
        //             }
        //         }
        //     }
        // }
        //
        // let slice = slice.freeze();

        let mut headers = http::HeaderMap::new();

        headers.reserve(headers_len);
        for header in &headers_indices[..headers_len] {
            // SAFETY: array is valid up to `headers_len`
            let header = unsafe { &*header.as_ptr() };
            let name = header_name!(&slice[header.name.0..header.name.1]);
            let value = header_value!(slice.slice(header.value.0..header.value.1));

            headers.append(name, value);
        }

        let mut extensions = http::Extensions::default();

        extensions.insert(AnyProxyRawHeaders(AnyProxyHyperBuf(slice.clone())));

        let head = MessageHead {
            version,
            subject: status,
            headers,
            extensions,
        };

        return Ok((head.into_response(hyper::Body::empty()), len));
    }
}

#[derive(Debug)]
pub enum Header {
    Token,
    #[cfg(feature = "http1")]
    ContentLengthInvalid,
    #[cfg(all(feature = "http1", feature = "server"))]
    TransferEncodingInvalid,
    #[cfg(feature = "http1")]
    TransferEncodingUnexpected,
}

#[derive(Debug)]
pub enum ErrParse {
    Method,
    Version,
    #[cfg(feature = "http1")]
    VersionH2,
    Uri,
    #[cfg_attr(not(all(feature = "http1", feature = "server")), allow(unused))]
    UriTooLong,
    Header(Header),
    TooLarge,
    Status,
    #[cfg_attr(debug_assertions, allow(unused))]
    Internal,
}

#[derive(Clone, Copy)]
struct HeaderIndices {
    name: (usize, usize),
    value: (usize, usize),
}

fn record_header_indices(
    bytes: &[u8],
    headers: &[httparse::Header<'_>],
    indices: &mut [MaybeUninit<HeaderIndices>],
) -> std::result::Result<(), ErrParse> {
    let bytes_ptr = bytes.as_ptr() as usize;

    for (header, indices) in headers.iter().zip(indices.iter_mut()) {
        if header.name.len() >= (1 << 16) {
            debug!("header name larger than 64kb: {:?}", header.name);
            return Err(ErrParse::TooLarge);
        }
        let name_start = header.name.as_ptr() as usize - bytes_ptr;
        let name_end = name_start + header.name.len();
        let value_start = header.value.as_ptr() as usize - bytes_ptr;
        let value_end = value_start + header.value.len();

        // FIXME(maybe_uninit_extra)
        // FIXME(addr_of)
        // Currently we don't have `ptr::addr_of_mut` in stable rust or
        // MaybeUninit::write, so this is some way of assigning into a MaybeUninit
        // safely
        let new_header_indices = HeaderIndices {
            name: (name_start, name_end),
            value: (value_start, value_end),
        };
        *indices = MaybeUninit::new(new_header_indices);
    }

    Ok(())
}

fn extend(dst: &mut Vec<u8>, data: &[u8]) {
    dst.extend_from_slice(data);
}

fn write_headers(headers: &HeaderMap, dst: &mut Vec<u8>) {
    for (name, value) in headers {
        extend(dst, name.as_str().as_bytes());
        extend(dst, b": ");
        extend(dst, value.as_bytes());
        extend(dst, b"\r\n");
    }
}

pub fn http_respons_to_vec(response: &Response<Body>) -> Vec<u8> {
    let mut data = Vec::with_capacity(102);
    {
        let dst = &mut data;
        match response.version() {
            Version::HTTP_10 => extend(dst, b"HTTP/1.0"),
            Version::HTTP_11 => extend(dst, b"HTTP/1.1"),
            Version::HTTP_2 => extend(dst, b"HTTP/1.1"),
            other => panic!("unexpected request version: {:?}", other),
        }

        log::debug!(target: "main",
            "respons_to_vec => {},{}",
            response.status().as_u16(),
            response.status().canonical_reason().unwrap()
        );

        extend(dst, b" ");
        extend(dst, response.status().as_u16().to_string().as_bytes());
        extend(dst, b" ");
        extend(dst, response.status().canonical_reason().unwrap().as_ref());
        extend(dst, b"\r\n");

        write_headers(response.headers(), dst);
        extend(dst, b"\r\n");

        log::debug!(target: "main",
            "respons_to_vec:[{}]",
            String::from_utf8(data.clone()).unwrap()
        );
    }
    return data;
}

pub fn get_http_host<T>(request: &mut Request<T>) -> Result<String> {
    let version = request.version();
    let host_value = request.headers().get(hyper::http::header::HOST);
    let http_host = match host_value {
        None => "",
        Some(t) => match t.to_str() {
            Err(e) => return Err(e)?,
            Ok(t) => t,
        },
    };

    let http_host = match http_host {
        "" => match (version, request.uri().host(), request.uri().port_u16()) {
            (Version::HTTP_2, Some(host), Some(port)) => format!("{}:{}", host, port),
            (Version::HTTP_2, Some(host), None) => format!("{}", host),
            _ => {
                return Err(anyhow::anyhow!(
                    "err:http_host nil => http_host:{:?}",
                    http_host
                ))?
            }
        },
        _ => http_host.to_string(),
    };

    if version == Version::HTTP_2 && host_value == None && http_host.len() > 0 {
        request
            .headers_mut()
            .insert("host", HeaderValue::from_bytes(http_host.as_bytes())?);
    }
    Ok(http_host)
}

pub fn e_tag(headers: &hyper::HeaderMap<http::HeaderValue>) -> Result<HeaderValue> {
    use http::header::ETAG;
    let etag = headers.get(ETAG).cloned();
    if etag.is_none() {
        return Ok(HeaderValue::from_static(""));
    }
    Ok(etag.unwrap())
}

pub fn header_value_to_str(v: &HeaderValue) -> Result<&str> {
    Ok(v.to_str()?.trim())
}

pub fn content_range(headers: &hyper::HeaderMap<http::HeaderValue>) -> Result<HttpRange> {
    let mut data = HttpRange {
        is_range: false,
        raw_content_length: 0,
        content_length: 0,
        range_start: 0,
        range_end: 0,
    };

    use http::header::CONTENT_RANGE;
    let range = headers.get(CONTENT_RANGE);
    if range.is_none() {
        return Ok(data);
    }
    //bytes
    let range = range.unwrap().to_str()?;
    if range.len() < 6 || &range[0..6] != "bytes " {
        return Ok(data);
    }
    let range = &range[6..];
    if range.len() <= 0 {
        return Ok(data);
    }

    data.is_range = true;

    let range = range.split_once("/");
    if range.is_none() {
        return Err(anyhow::anyhow!("content_range"));
    }
    let (range, raw_content_length) = range.unwrap();
    let range = range.split_once("-");
    if range.is_none() {
        return Err(anyhow::anyhow!("content_range"));
    }
    let (range_start, range_end) = range.unwrap();
    data.range_start = range_start.trim().parse::<u64>()?;
    data.range_end = range_end.trim().parse::<u64>()?;
    if data.range_start > data.range_end {
        return Err(anyhow!(""));
    }

    data.raw_content_length = raw_content_length.trim().parse::<u64>()?;
    data.content_length = data.range_end - data.range_start + 1;

    return Ok(data);
}

pub fn content_length(headers: &hyper::HeaderMap<http::HeaderValue>) -> Result<u64> {
    use http::header::CONTENT_LENGTH;
    let content_length = headers.get(CONTENT_LENGTH);
    if content_length.is_none() {
        return Ok(0);
    }
    let content_length = content_length.unwrap().to_str()?.trim();
    let content_length = content_length.parse::<u64>()?;
    return Ok(content_length);
}

pub fn last_modified(headers: &hyper::HeaderMap<http::HeaderValue>) -> Result<(HeaderValue, u64)> {
    use http::header::LAST_MODIFIED;
    use std::str::FromStr;
    let last_modified = headers.get(LAST_MODIFIED).cloned();
    if last_modified.is_none() {
        return Ok((HeaderValue::from_static(""), 0));
    }
    let last_modified = last_modified.unwrap();
    let last_modified_time = httpdate::HttpDate::from_str(last_modified.to_str()?)?;
    let last_modified_time = std::time::SystemTime::from(last_modified_time);
    let last_modified_time = last_modified_time
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs();
    return Ok((last_modified, last_modified_time));
}

pub fn cache_control_time(
    headers: &hyper::HeaderMap<http::HeaderValue>,
) -> Result<(i64, u64, Option<SystemTime>)> {
    use http::header::CACHE_CONTROL;
    use http::header::PRAGMA;
    let cache_control_time = headers.get(CACHE_CONTROL);
    let cache_control_time = if cache_control_time.is_some() {
        cache_control_time
    } else {
        let cache_control_time = headers.get(PRAGMA);
        if cache_control_time.is_some() {
            cache_control_time
        } else {
            None
        }
    };
    if cache_control_time.is_none() {
        return Ok((-1, 0, None));
    }

    let cache_control_time = cache_control_time.unwrap().to_str()?;
    let index = cache_control_time.rfind("=");
    if index.is_none() {
        return Ok((0, 0, None));
    }
    let index = index.unwrap();
    let cache_control_time = cache_control_time[index + 1..].trim().parse::<u64>()?;
    let expires = std::time::SystemTime::now();
    use std::ops::Add;
    let expires = expires.add(std::time::Duration::from_secs(cache_control_time));
    let expires_n = expires.duration_since(UNIX_EPOCH).unwrap().as_secs();

    return Ok((cache_control_time as i64, expires_n, Some(expires)));
}

pub fn get_http_range(
    headers: &hyper::HeaderMap<http::HeaderValue>,
    raw_content_length: u64,
) -> Result<HttpRange> {
    let mut data = HttpRange {
        is_range: false,
        raw_content_length,
        content_length: raw_content_length,
        range_start: 0,
        range_end: 0,
    };

    use http::header::RANGE;
    let range = headers.get(RANGE);
    if range.is_none() {
        return Ok(data);
    }

    if raw_content_length <= 0 {
        return Ok(data);
    }

    let range = range.unwrap().to_str();
    if range.is_err() {
        return Ok(data);
    }
    let range = range.unwrap();

    if range.len() < 6 || &range[0..6] != "bytes=" {
        return Ok(data);
    }

    let range = &range[6..];
    if range.len() <= 0 {
        return Ok(data);
    }

    data.is_range = true;
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
        if vv1 >= raw_content_length {
            // HTTP/1.1 416 Requested Range Not Satisfiable
            return Err(anyhow!(""));
        }
        data.range_start = vv1;
    } else {
        let vv2 = vv2?;
        if vv2 > raw_content_length {
            // HTTP/1.1 416 Requested Range Not Satisfiable
            return Err(anyhow!(""));
        }
        data.range_start = raw_content_length - vv2;
        data.range_end = raw_content_length - 1;
        data.content_length = vv2;
        return Ok(data);
    }

    if vv2.is_ok() {
        let vv2 = vv2?;
        if vv2 >= raw_content_length {
            data.range_end = raw_content_length - 1;
        } else {
            data.range_end = vv2;
        }
    } else {
        if v2.len() > 0 {
            // HTTP/1.1 416 Requested Range Not Satisfiable
            return Err(anyhow!(""));
        }
        data.range_end = raw_content_length - 1;
    }

    if data.range_start > data.range_end {
        // HTTP/1.1 416 Requested Range Not Satisfiable
        return Err(anyhow!(""));
    }
    data.content_length = data.range_end - data.range_start + 1;

    return Ok(data);
}

pub fn get_http_range_ext(in_headers: &hyper::HeaderMap<http::HeaderValue>) -> Result<(i64, i64)> {
    let mut range_start = -1;
    let mut range_end = -1;
    use http::header::RANGE;
    let range = in_headers.get(RANGE);
    if range.is_none() {
        range_start = 0;
        return Ok((range_start, range_end));
    }

    let range = range.unwrap().to_str();
    if range.is_err() {
        range_start = 0;
        return Ok((range_start, range_end));
    }
    let range = range.unwrap();

    if range.len() < 6 || &range[0..6] != "bytes=" {
        range_start = 0;
        return Ok((range_start, range_end));
    }

    let range = &range[6..];
    if range.len() <= 0 {
        range_start = 0;
        return Ok((range_start, range_end));
    }

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
        range_start = vv1 as i64;
    } else {
        return Ok((range_start, range_end));
    }

    if vv2.is_ok() {
        let vv2 = vv2?;
        range_end = vv2 as i64;
    } else {
        if v2.len() > 0 {
            // HTTP/1.1 416 Requested Range Not Satisfiable
            return Err(anyhow!(""));
        }
    }

    if range_start > 0 && range_end > 0 && range_start > range_end {
        // HTTP/1.1 416 Requested Range Not Satisfiable
        return Err(anyhow!(""));
    }
    return Ok((range_start, range_end));
}

pub fn http_headers_size(
    uri: Option<&hyper::Uri>,
    status_code: Option<http::status::StatusCode>,
    headers: &hyper::HeaderMap<http::HeaderValue>,
) -> usize {
    let mut size = 20;
    if uri.is_some() {
        size += uri
            .unwrap()
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/")
            .len();
    }

    if status_code.is_some() {
        size += 3;
        let status_code = status_code.unwrap().canonical_reason();
        if status_code.is_some() {
            size += status_code.unwrap().len();
        }
    }

    for (k, v) in headers {
        size += k.as_str().len();
        size += v.as_bytes().len();
        size += 3;
    }
    size
}

pub fn copy_request_parts<T>(
    stream_info: Share<StreamInfo>,
    request: &Request<T>,
) -> Result<HttpParts> {
    let scheme = if stream_info.get().server_stream_info.is_tls {
        "https"
    } else {
        "http"
    };

    let req_host = request.headers().get(HOST).cloned();
    if req_host.is_none() {
        return Err(anyhow!("host nil"));
    }
    let req_host = req_host.unwrap();
    let req_host = req_host.to_str().unwrap();
    let (domain, port) = host_and_port(req_host);
    let port = if port.len() <= 0 {
        if stream_info.get().server_stream_info.is_tls {
            "443"
        } else {
            "80"
        }
    } else {
        port
    };

    let uri = format!(
        "{}://{}:{}{}",
        scheme,
        domain,
        port,
        request
            .uri()
            .path_and_query()
            .map(|x| x.as_str())
            .unwrap_or("/")
    );
    let uri: http::Uri = uri.parse()?;
    Ok(HttpParts {
        method: request.method().clone(),
        uri,
        version: request.version().clone(),
        headers: request.headers().clone(),
    })
}

pub fn is_request_body_nil(method: &http::Method) -> bool {
    match method {
        &http::Method::GET => true,
        &http::Method::HEAD => true,
        &http::Method::OPTIONS => true,
        &http::Method::DELETE => true,
        &http::Method::TRACE => true,
        _ => false,
    }
}
