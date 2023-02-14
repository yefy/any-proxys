use super::client as quic_client;
use crate::config::config_toml::QuicConfig;
use crate::quic::endpoints;
use crate::stream::client::Client;
use crate::stream::connect;
use crate::stream::connect::ConnectInfo;
use crate::stream::stream_flow;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

pub struct ConnectContext {
    host: String,
    address: SocketAddr, //ip:port, domain:port
    ssl_domain: String,
    endpoints: Arc<endpoints::Endpoints>,
    quic_config: QuicConfig,
}

pub struct Connect {
    context: Arc<ConnectContext>,
}

impl Connect {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        endpoints: Arc<endpoints::Endpoints>,
        quic_config: QuicConfig,
    ) -> Connect {
        Connect {
            context: Arc::new(ConnectContext {
                host,
                address,
                ssl_domain,
                endpoints,
                quic_config,
            }),
        }
    }
}

#[async_trait]
impl connect::Connect for Connect {
    async fn connect(
        &self,
        _request_id: Option<String>,
        info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(stream_flow::StreamFlow, ConnectInfo)> {
        let timeout =
            tokio::time::Duration::from_secs(self.context.quic_config.quic_connect_timeout as u64);
        let start_time = Instant::now();

        let addr = self.context.address.clone();
        let endpoint = self.context.endpoints.endpoint()?;

        let client =
            quic_client::Client::new(addr, timeout, endpoint.clone(), &self.context.ssl_domain)
                .map_err(|e| anyhow!("err:quic_client::Client::new => e:{}", e))?;
        let connection = {
            client
                .connect(info)
                .await
                .map_err(|e| anyhow!("err:client.connect => e:{}", e))?
        };

        let (protocol_name, mut stream, local_addr, remote_addr) = connection.stream(info).await?;
        let elapsed = start_time.elapsed().as_secs_f32();

        let read_timeout =
            tokio::time::Duration::from_secs(self.context.quic_config.quic_recv_timeout as u64);
        let write_timeout =
            tokio::time::Duration::from_secs(self.context.quic_config.quic_send_timeout as u64);
        stream.set_config(read_timeout, write_timeout, None);

        Ok((
            stream,
            ConnectInfo {
                protocol7: protocol_name,
                domain: self.context.host.clone(),
                elapsed,
                local_addr,
                remote_addr,
                peer_stream_size: None,
                max_stream_size: None,
                min_stream_cache_size: None,
                channel_size: None,
            },
        ))
    }
}
