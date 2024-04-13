use crate::config::config_toml::HttpServerProxyConfig;
use crate::proxy::http_proxy::http_hyper_connector::HttpHyperConnector;
use crate::proxy::http_proxy::http_stream::HttpStream;
use crate::proxy::http_proxy::HyperExecutorLocal;
use crate::proxy::stream_info::StreamInfo;
use crate::stream::connect::{Connect, ConnectInfo};
use crate::Protocol7;
use any_base::typ::Share;
use anyhow::Result;
use http::Version;
use std::sync::Arc;
use tokio::time::Duration;

impl HttpStream {
    pub async fn get_client(
        &self,
        session_id: u64,
        version: Version,
        connect_func: Arc<Box<dyn Connect>>,
        config: &HttpServerProxyConfig,
        stream_info: Share<StreamInfo>,
        protocol7: &str,
    ) -> Result<Arc<hyper::Client<HttpHyperConnector>>> {
        let scc = stream_info.get().scc.clone();
        let addr = connect_func.addr().await?;
        let is_http2 = match &version {
            &hyper::http::Version::HTTP_11 => false,
            &hyper::http::Version::HTTP_2 => true,
            _ => {
                return Err(anyhow::anyhow!(
                    "err:http version not found => version:{:?}",
                    version
                ))?
            }
        };
        let upstream_connect_info = ConnectInfo {
            protocol7: Protocol7::from_string(&protocol7)?,
            domain: connect_func.domain().await,
            elapsed: 0.0,
            local_addr: addr.clone(),
            remote_addr: addr.clone(),
            peer_stream_size: None,
            max_stream_size: None,
            min_stream_cache_size: None,
            channel_size: None,
        };
        stream_info
            .get_mut()
            .upstream_connect_info
            .set(upstream_connect_info);

        let http_context = {
            use crate::config::net_core;
            let net_core_conf = net_core::curr_conf(scc.net_curr_conf());

            if net_core_conf.is_disable_share_http_context {
                self.http_arg.http_context.clone()
            } else {
                net_core_conf.http_context.clone()
            }
        };

        let key = format!("{}-{}-{}", protocol7, addr, is_http2);
        log::debug!("session_id:{}, get_client key:{}", session_id, key);
        let client = http_context.client_map.get().get(&key).cloned();
        if client.is_some() {
            return Ok(client.unwrap());
        }

        let request_id = stream_info.get().request_id.clone();
        use crate::config::common_core;
        let common_core_conf = common_core::main_conf_mut(&self.http_arg.ms).await;
        let http = HttpHyperConnector::new(
            request_id,
            connect_func,
            common_core_conf.session_id.clone(),
            self.http_arg.executors.context.run_time.clone(),
        );

        let client = hyper::Client::builder()
            .executor(HyperExecutorLocal(self.http_arg.executors.clone()))
            .pool_max_idle_per_host(config.proxy_pass.pool_max_idle_per_host)
            .pool_idle_timeout(Duration::from_secs(config.proxy_pass.pool_idle_timeout))
            .http2_only(is_http2)
            //.set_host(false)
            .build(http);
        let client = Arc::new(client);

        http_context
            .client_map
            .get_mut()
            .insert(key, client.clone());

        Ok(client)
    }
}
