pub mod websocket_server_echo;
pub mod websocket_server_proxy;
pub mod websocket_server_static;
use crate::protopack::anyproxy::AnyproxyHello;
use crate::proxy::ServerArg;
use crate::proxy::{util as proxy_util, StreamConfigContext};
use crate::Protocol77;
use any_base::io::buf_stream::BufStream;
use any_base::stream_flow::StreamFlow;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use base64::{engine::general_purpose, Engine as _};
use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};
use tokio_tungstenite::WebSocketStream;

use crate::proxy::http_proxy::http_header_parse::{parse_request_parts, HttpParts};
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use crate::proxy::stream_info::ErrStatus;
use crate::proxy::util::{
    find_local, run_plugin_handle_access, run_plugin_handle_websocket,
    run_plugin_handle_websocket_serverless,
};
use any_base::io::buf_reader::BufReader;
use futures_core::Stream;
use futures_sink::Sink;
use hyper::Body;
use lazy_static::lazy_static;
use std::sync::Arc;
use tokio_tungstenite::tungstenite::error::Error;
use tokio_tungstenite::tungstenite::Message;

pub trait WebSocketStreamTrait:
    Sink<Message, Error = Error> + Stream<Item = Result<Message, Error>> + Unpin + Sync + Send
{
}
impl<
        T: Sink<Message, Error = Error> + Stream<Item = Result<Message, Error>> + Unpin + Sync + Send,
    > WebSocketStreamTrait for T
{
}

lazy_static! {
    pub static ref WEBSOCKET_HELLO_KEY: String = "websocket_hello".to_string();
}

pub async fn server_handle(arg: ServerArg, client_buf_reader: BufReader<StreamFlow>) -> Result<()> {
    arg.stream_info.get_mut().add_work_time1("websocket");
    let client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );

    arg.stream_info.get_mut().err_status = ErrStatus::CLIENT_PROTO_ERR;
    let value = stream_parse(arg.clone(), client_buf_stream).await?;
    if value.is_none() {
        return Ok(());
    }

    let (_, scc, client_stream) = value.unwrap();
    let ret = run_plugin_handle_websocket(&scc, &arg, client_stream).await;
    arg.stream_info.get_mut().http_r.set_nil();
    ret?;
    Ok(())
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

    let domain_config_context = arg
        .domain_config_listen
        .domain_config_context_map
        .iter()
        .last();
    if domain_config_context.is_some() {
        let (_, domain_config_context) = domain_config_context.unwrap();
        stream_info.get_mut().scc =
            Some(domain_config_context.scc.to_arc_scc(arg.ms.clone())).into();
    }

    let mut part: Option<HttpParts> = None;

    let copy_headers_callback =
        |request: &Request, response: Response| -> std::result::Result<Response, ErrorResponse> {
            let ret = parse_request_parts(&stream_info, request);
            if ret.is_ok() {
                let (_, parts) = ret.unwrap();
                part = Some(parts);
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

    let session_id = stream_info.get().session_id;
    let r = Arc::new(
        HttpStreamRequest::new(
            arg.clone(),
            arg.clone(),
            session_id,
            None.into(),
            part,
            request,
            None,
        )
        .await?,
    );

    r.http_arg.stream_info.get_mut().http_r = Some(r.clone()).into();

    let (domain, hello_str) = {
        let r_in = &mut r.ctx.get_mut().r_in;
        let domain = ArcString::new(r_in.uri.host().unwrap().to_string());
        let hello_str = r_in.upstream_headers.remove(&*WEBSOCKET_HELLO_KEY);
        (domain, hello_str)
    };

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
        |_| async { Ok(domain) },
    )
    .await?;

    arg.stream_info.get_mut().err_status = ErrStatus::ACCESS_LIMIT;
    if run_plugin_handle_access(&scc, &stream_info).await? {
        return Err(anyhow::anyhow!("run_plugin_handle_access"));
    }
    arg.stream_info.get_mut().err_status = ErrStatus::SERVER_ERR;

    find_local(&stream_info)?;
    let scc = stream_info.get().scc.clone();

    let client_stream =
        run_plugin_handle_websocket_serverless(&scc, &stream_info, client_stream).await?;
    if client_stream.is_none() {
        return Ok(None);
    }
    let client_stream = client_stream.unwrap();

    Ok(Some((r, scc.unwrap(), client_stream)))
}
