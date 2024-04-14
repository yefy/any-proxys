use crate::proxy::http_proxy::http_hyper_stream::HttpHyperStream;
use crate::stream::connect::Connect;
use any_base::executor_local_spawn::Runtime;
use any_base::util::ArcString;
use anyhow::anyhow;
use hyper::client::connect::ReqArg2;
use hyper::service::Service;
use std::future::Future;
use std::pin::Pin;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::task::{Context, Poll};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A Connector for the `https` scheme.
#[derive(Clone)]
pub struct HttpHyperConnector {
    request_id: ArcString,
    connect_func: Arc<Box<dyn Connect>>,
    session_id: Arc<AtomicU64>,
    run_time: Arc<Box<dyn Runtime>>,
}

impl HttpHyperConnector {
    pub fn new(
        request_id: ArcString,
        connect_func: Arc<Box<dyn Connect>>,
        session_id: Arc<AtomicU64>,
        run_time: Arc<Box<dyn Runtime>>,
    ) -> HttpHyperConnector {
        HttpHyperConnector {
            request_id,
            connect_func,
            session_id,
            run_time,
        }
    }
}

impl Service<ReqArg2> for HttpHyperConnector {
    type Response = HttpHyperStream;
    type Error = BoxError;

    #[allow(clippy::type_complexity)]
    type Future = Pin<Box<dyn Future<Output = Result<HttpHyperStream, BoxError>> + Send>>;

    fn poll_ready(&mut self, _cx: &mut Context<'_>) -> Poll<Result<(), Self::Error>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, arg2: ReqArg2) -> Self::Future {
        log::trace!(target: "main", "http_connector dst:{}", arg2.dst);
        let request_id = format!(
            "{}-{}",
            self.request_id,
            self.session_id.fetch_add(1, Ordering::Relaxed)
        );
        let upstream_connect_flow_info = if arg2.arg.is_some() {
            Some(
                arg2.arg
                    .as_ref()
                    .unwrap()
                    .upstream_connect_flow_info
                    .clone(),
            )
        } else {
            None
        };
        let request_id = ArcString::new(request_id);
        let connect_func = self.connect_func.clone();
        let run_time = self.run_time.clone();
        Box::pin(async move {
            let connect_info = connect_func
                .connect(
                    Some(request_id.clone()),
                    upstream_connect_flow_info,
                    Some(run_time),
                )
                .await
                .map_err(|e| anyhow!("err:connect => request_id:{}, e:{}", request_id, e))?;

            let (upstream_stream, _) = connect_info;
            Ok(HttpHyperStream::new(upstream_stream))
        })
    }
}
