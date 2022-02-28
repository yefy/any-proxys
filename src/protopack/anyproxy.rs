use crate::io::buf_reader::BufReader;
use crate::stream::stream_flow::StreamFlow;
use any_tunnel::protopack;
use any_tunnel2::protopack as protopack2;
use serde::ser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::io::BufWriter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const ANYPROXY_MAX_HEADER_SIZE: usize = 4096;
pub const ANYPROXY_FLAG: &'static str = "$${anyproxy}";
pub const ANYPROXY_VERSION: &'static str = "anyproxy.0.1.0";

//ANYPROXY_FLAG AnyproxyHeaderSize_u16 AnyproxyHeader AnyproxyHello
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHeader {
    pub header_type: u8,
    pub body_size: u16,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHello {
    pub version: String,
    pub request_id: String,
    pub client_addr: SocketAddr,
    pub domain: String,
}

pub enum AnyproxyHeaderType {
    Hello = 1,
    TunnelHello = 2,
    TunnelPack = 3,
    TunnelPackAck = 4,
    TunnelClose = 5,
    Max = 6,
}

pub enum AnyproxyPack {
    Hello(AnyproxyHello),
}

#[derive(Clone)]
pub enum AnyproxyArcPack {
    Hello(Arc<AnyproxyHello>),
}

pub async fn read_tunnel_hello(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> anyhow::Result<Option<protopack::TunnelHello>> {
    buf_reader.start();
    let tunnel_hello = protopack::read_tunnel_hello(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow::anyhow!("err:rollback"));
    }
    if tunnel_hello.is_err() {
        Ok(None)
    } else {
        Ok(tunnel_hello?)
    }
}

pub async fn read_tunnel2_hello(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> anyhow::Result<Option<protopack2::TunnelHello>> {
    buf_reader.start();
    let tunnel_hello = protopack2::read_tunnel_hello(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow::anyhow!("err:rollback"));
    }
    if tunnel_hello.is_err() {
        Ok(None)
    } else {
        Ok(tunnel_hello?)
    }
}

pub async fn read_hello(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> anyhow::Result<Option<AnyproxyHello>> {
    let anyproxy = read_pack_rollback(buf_reader).await?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let anyproxy = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::Hello(hello) => {
            return Ok(Some(hello));
        }
    }
}

pub async fn write_pack<T: ?Sized>(
    buf_writer: &mut BufWriter<&StreamFlow>,
    typ: AnyproxyHeaderType,
    value: &T,
) -> anyhow::Result<()>
where
    T: ser::Serialize,
{
    let slice = toml::to_vec(value)?;
    let header_slice = toml::to_vec(&AnyproxyHeader {
        header_type: typ as u8,
        body_size: slice.len() as u16,
    })?;
    buf_writer.write(ANYPROXY_FLAG.as_bytes()).await?;
    buf_writer.write_u16(header_slice.len() as u16).await?;
    buf_writer.write(&header_slice).await?;
    buf_writer.write(&slice).await?;
    buf_writer.flush().await?;
    Ok(())
}

pub async fn read_pack_rollback(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> anyhow::Result<Option<AnyproxyPack>> {
    buf_reader.start();
    let anyproxy = read_pack(buf_reader).await?;
    if anyproxy.is_none() {
        if !buf_reader.rollback() {
            return Err(anyhow::anyhow!("err:rollback"));
        }
        return Ok(None);
    }
    if !buf_reader.commit() {
        return Err(anyhow::anyhow!("err:commit"));
    }
    Ok(anyproxy)
}

pub async fn read_pack(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> anyhow::Result<Option<AnyproxyPack>> {
    let mut slice = [0u8; ANYPROXY_MAX_HEADER_SIZE];
    let flag_slice = &mut slice[..ANYPROXY_FLAG.len() as usize];
    buf_reader.read_exact(flag_slice).await?;
    let flag_str = String::from_utf8_lossy(flag_slice);
    if ANYPROXY_FLAG != &flag_str {
        return Ok(None);
    }

    let header_size = buf_reader.read_u16().await?;
    if header_size as usize > ANYPROXY_MAX_HEADER_SIZE || header_size <= 0 {
        return Err(anyhow::anyhow!(
            "err:header_size > ANYPROXY_MAX_HEADER_SIZE => header_size:{}",
            header_size
        ));
    }

    let header_slice = &mut slice[..header_size as usize];
    buf_reader.read_exact(header_slice).await?;
    let header: AnyproxyHeader = toml::from_slice(header_slice)
        .map_err(|e| anyhow::anyhow!("err:AnyproxyPack header=> e:{}", e))?;

    if header.header_type >= AnyproxyHeaderType::Max as u8 {
        return Err(anyhow::anyhow!("err:AnyproxyPack header_type"));
    }

    if header.body_size as usize > ANYPROXY_MAX_HEADER_SIZE || header.body_size <= 0 {
        return Err(anyhow::anyhow!("err:AnyproxyPack body_size"));
    }

    let body_slice = &mut slice[..header.body_size as usize];
    buf_reader.read_exact(body_slice).await?;

    if header.header_type == AnyproxyHeaderType::Hello as u8 {
        let value: AnyproxyHello = toml::from_slice(body_slice)
            .map_err(|e| anyhow::anyhow!("err:AnyproxyPack=> e:{}", e))?;
        Ok(Some(AnyproxyPack::Hello(value)))
    } else {
        Err(anyhow::anyhow!("err:AnyproxyPack type"))
    }
}
