use super::client as tcp_client;
use crate::config::config_toml::TcpConfig;
use crate::stream::client::Client;
use crate::stream::connect;
use crate::stream::connect::ConnectInfo;
use crate::Protocol7;
use any_base::executor_local_spawn::Runtime;
use any_base::stream_flow::{StreamFlow, StreamFlowInfo};
use any_base::typ::ArcMutex;
use any_base::util::ArcString;
use anyhow::anyhow;
use anyhow::Result;
use async_trait::async_trait;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Instant;

pub struct ConnectContext {
    host: ArcString,
    address: SocketAddr, //ip:port, domain:port
    tcp_config: TcpConfig,
    ssl_domain: String,
}

pub struct Connect {
    context: Arc<ConnectContext>,
}

impl Connect {
    pub fn new(
        host: ArcString,
        address: SocketAddr, //ip:port, domain:port
        ssl_domain: String,
        tcp_config: TcpConfig,
    ) -> Connect {
        Connect {
            context: Arc::new(ConnectContext {
                host,
                address,
                tcp_config,
                ssl_domain,
            }),
        }
    }
}

#[async_trait]
impl connect::Connect for Connect {
    async fn connect(
        &self,
        _request_id: Option<ArcString>,
        stream_info: ArcMutex<StreamFlowInfo>,
        _run_time: Option<Arc<Box<dyn Runtime>>>,
    ) -> Result<(StreamFlow, ConnectInfo)> {
        let tcp_connect_timeout = self.context.tcp_config.tcp_connect_timeout as u64;
        let timeout = tokio::time::Duration::from_secs(tcp_connect_timeout);
        let start_time = Instant::now();
        let addr = self.context.address.clone();
        let client = tcp_client::Client::new(
            addr,
            timeout,
            Arc::new(self.context.tcp_config.clone()),
            self.context.ssl_domain.clone(),
        )
        .map_err(|e| anyhow!("err:tcp_client::Client::new => e:{}", e))?;
        let connection = {
            client
                .connect(stream_info.clone())
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

        let tcp_recv_timeout = self.context.tcp_config.tcp_recv_timeout as u64;
        let read_timeout = tokio::time::Duration::from_secs(tcp_recv_timeout);
        let tcp_send_timeout = self.context.tcp_config.tcp_send_timeout as u64;
        let write_timeout = tokio::time::Duration::from_secs(tcp_send_timeout);
        stream.set_config(read_timeout, write_timeout, ArcMutex::default());

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

    async fn addr(&self) -> Result<SocketAddr> {
        Ok(self.context.address.clone())
    }

    async fn host(&self) -> Result<ArcString> {
        Ok(self.context.host.clone())
    }
    async fn is_tls(&self) -> bool {
        true
    }
    async fn protocol7(&self) -> String {
        Protocol7::Ssl.to_string()
    }
    async fn domain(&self) -> ArcString {
        self.context.host.clone()
    }
}
