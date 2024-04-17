pub mod websocket_echo_server;
pub mod websocket_server;
pub mod websocket_static_server;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::ServerArg;
use crate::proxy::{util as proxy_util, StreamConfigContext};
use crate::util::util::host_and_port;
use crate::Protocol77;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::WebSocketStream;

use crate::proxy::http_proxy::http_header_parse::{copy_request_parts, get_http_host, HttpParts};
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::util::{
    find_local, run_plugin_handle_access, run_plugin_handle_websocket_serverless,
};
use hyper::Body;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref WEBSOCKET_HELLO_KEY: String = "websocket_hello".to_string();
}

pub async fn stream_parse(
    arg: ServerArg,
    stream: BufStream<StreamFlow>,
) -> Result<
    Option<(
        Arc<HttpStreamRequest>,
        Arc<StreamConfigContext>,
        WebSocketStream<BufStream<StreamFlow>>,
    )>,
> {
    let stream_info = arg.stream_info.clone();
    if arg.server_stream_info.is_tls {
        stream_info.get_mut().client_protocol77 = Some(Protocol77::WebSockets);
    } else {
        stream_info.get_mut().client_protocol77 = Some(Protocol77::WebSocket);
    }

    let mut part: Option<HttpParts> = None;

    let copy_headers_callback =
        |request: &Request, response: Response| -> std::result::Result<Response, ErrorResponse> {
            let parts = copy_request_parts(stream_info.clone(), request);
            if parts.is_ok() {
                part = Some(parts.unwrap());
            }
            Ok(response)
        };

    let client_stream = tokio_tungstenite::accept_hdr_async(stream, copy_headers_callback)
        .await
        .map_err(|e| anyhow::anyhow!("err:accept_hdr_async => e:{}", e))?;
    if part.is_none() {
        return Err(anyhow::anyhow!("err:part.is_none"));
    }
    let part = part.unwrap();
    let mut request = http::request::Request::new(Body::empty());
    *request.method_mut() = part.method.clone();
    *request.version_mut() = part.version.clone();
    *request.uri_mut() = part.uri.clone();
    *request.headers_mut() = part.headers.clone();
    log::trace!(target: "main", "request.headers:{:?}", request.headers());

    let http_host = get_http_host(&mut request)?;
    let (domain, _) = host_and_port(&http_host);
    let domain = ArcString::new(domain.to_string());
    let hello_str = request.headers().get(&*WEBSOCKET_HELLO_KEY).cloned();

    let scc = proxy_util::parse_proxy_domain(
        &arg,
        move || async move {
            let hello = {
                if hello_str.is_none() {
                    None
                } else {
                    let hello =
                        general_purpose::STANDARD.decode(hello_str.as_ref().unwrap().as_bytes())?;
                    let hello: AnyproxyHello = toml::from_slice(&hello)
                        .map_err(|e| anyhow!("err:toml::from_slice=> e:{}", e))?;
                    Some((hello, 0))
                }
            };
            Ok(hello)
        },
        || async { Ok(domain) },
    )
    .await?;

    if run_plugin_handle_access(scc.clone(), stream_info.clone()).await? {
        return Err(anyhow::anyhow!("run_plugin_handle_access"));
    }

    let session_id = stream_info.get().session_id;
    let r = Arc::new(
        HttpStreamRequest::new(
            arg.clone(),
            arg.clone(),
            session_id,
            None.into(),
            part,
            request,
            false,
            false,
            None.into(),
        )
        .await?,
    );

    r.http_arg.stream_info.get_mut().http_r = Some(r.clone()).into();

    find_local(stream_info.clone())?;
    let scc = stream_info.get().scc.clone().unwrap();

    let client_stream =
        run_plugin_handle_websocket_serverless(scc.clone(), stream_info.clone(), client_stream)
            .await?;
    if client_stream.is_none() {
        return Ok(None);
    }
    let client_stream = client_stream.unwrap();

    Ok(Some((r, scc, client_stream)))
}
