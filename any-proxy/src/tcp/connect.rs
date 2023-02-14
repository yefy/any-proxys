use super::client as tcp_lient;
use crate::config::config_toml::TcpConfig;
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
    tcp_config: TcpConfig,
}

pub struct Connect {
    context: Arc<ConnectContext>,
}

impl Connect {
    pub fn new(
        host: String,
        address: SocketAddr, //ip:port, domain:port
        tcp_config: TcpConfig,
    ) -> Connect {
        Connect {
            context: Arc::new(ConnectContext {
                host,
                address,
                tcp_config,
            }),
        }
    }
}

#[async_trait]
impl connect::Connect for Connect {
    async fn connect(
        &self,
        _request_id: Option<String>,
        stream_info: &mut Option<&mut stream_flow::StreamFlowInfo>,
    ) -> Result<(stream_flow::StreamFlow, ConnectInfo)> {
        let timeout =
            tokio::time::Duration::from_secs(self.context.tcp_config.tcp_connect_timeout as u64);
        let start_time = Instant::now();
        let addr = self.context.address.clone();
        let client =
            tcp_lient::Client::new(addr, timeout, Arc::new(self.context.tcp_config.clone()))
                .map_err(|e| anyhow!("err:tcp_lient::Client::new => e:{}", e))?;
        let connection = {
            client
                .connect(stream_info)
                .await
                .map_err(|e| anyhow!("err:client.connect => e:{}", e))?
        };
        let (protocol_name, mut stream, local_addr, remote_addr) = {
            connection
                .stream(stream_info)
                .await
                .map_err(|e| anyhow!("err:connection.stream => e:{}", e))?
        };
        let elapsed = start_time.elapsed().as_secs_f32();

        let read_timeout =
            tokio::time::Duration::from_secs(self.context.tcp_config.tcp_recv_timeout as u64);
        let write_timeout =
            tokio::time::Duration::from_secs(self.context.tcp_config.tcp_send_timeout as u64);
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
