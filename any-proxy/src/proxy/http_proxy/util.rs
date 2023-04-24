use anyhow::Result;
use hyper::http::header::HeaderValue;
use hyper::http::{Request, Version};
use hyper::Body;

pub fn get_http_host(req: &mut Request<Body>) -> Result<String> {
    let version = req.version();
    let host_value = req.headers().get(hyper::http::header::HOST);
    let http_host = match host_value {
        None => "",
        Some(t) => match t.to_str() {
            Err(e) => return Err(e)?,
            Ok(t) => t,
        },
    };

    let http_host = match http_host {
        "" => match (version, req.uri().host(), req.uri().port_u16()) {
            (Version::HTTP_2, Some(host), Some(port)) => format!("{}:{}", host, port),
            (Version::HTTP_2, Some(host), None) => format!("{}", host),
            _ => return Err(anyhow::anyhow!("err:http_host nil => req:{:?}", req))?,
        },
        _ => http_host.to_string(),
    };

    if version == Version::HTTP_2 && host_value == None && http_host.len() > 0 {
        req.headers_mut()
            .insert("host", HeaderValue::from_bytes(http_host.as_bytes())?);
    }
    Ok(http_host)
}
