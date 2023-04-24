use crate::proxy::http_proxy::http_hyper_stream::HttpHyperStream;
use crate::stream::connect::Connect;
use any_base::stream_flow::StreamFlowInfo;
use anyhow::anyhow;
use hyper::{service::Service, Uri};
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A Connector for the `https` scheme.
#[derive(Clone)]
pub struct HttpHyperConnector {
    upstream_connect_flow_info: std::sync::Arc<tokio::sync::Mutex<StreamFlowInfo>>,
    request_id: String,
    upstream_stream_flow_info: std::sync::Arc<std::sync::Mutex<StreamFlowInfo>>,
    connect_func: Arc<Box<dyn Connect>>,
    session_id: Arc<AtomicU64>,
}

impl HttpHyperConnector {
    pub fn new(
        upstream_connect_flow_info: std::sync::Arc<tokio::sync::Mutex<StreamFlowInfo>>,
        request_id: String,
        upstream_stream_flow_info: std::sync::Arc<std::sync::Mutex<StreamFlowInfo>>,
        connect_func: Arc<Box<dyn Connect>>,
        session_id: Arc<AtomicU64>,
    ) -> HttpHyperConnector {
        HttpHyperConnector {
            upstream_connect_flow_info,
            request_id,
            upstream_stream_flow_info,
            connect_func,
            session_id,
        }
    }
}

impl Service<Uri> for HttpHyperConnector {
    type Response = HttpHyperStream;
    type Error = BoxError;

    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<HttpHyperStream, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, dst: Uri) -> Self::Future {
        log::trace!("http_connector dst:{}", dst);
        let request_id = format!(
            "{}-{}",
            self.request_id,
            self.session_id.fetch_add(1, Ordering::Relaxed)
        );
        let upstream_connect_flow_info = self.upstream_connect_flow_info.clone();
        let upstream_stream_flow_info = self.upstream_stream_flow_info.clone();
        let connect_func = self.connect_func.clone();
        Box::pin(async move {
            let connect_info = connect_func
                .connect(
                    Some(request_id.clone()),
                    &mut Some(&mut *upstream_connect_flow_info.lock().await),
                )
                .await
                .map_err(|e| anyhow!("err:connect => request_id:{}, e:{}", request_id, e))?;

            let (mut upstream_stream, _) = connect_info;

            upstream_stream.set_stream_info(Some(upstream_stream_flow_info.clone()));

            Ok(HttpHyperStream::new(upstream_stream))
        })
    }
}
