use crate::io::buf_reader::BufReader;
use crate::io::buf_writer::BufWriter;
use crate::stream::stream_flow::StreamFlow;
use any_tunnel::protopack;
use any_tunnel2::protopack as protopack2;
use anyhow::anyhow;
use anyhow::Result;
use serde::ser;
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
//use tokio::io::BufWriter;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub const ANYPROXY_MAX_HEADER_SIZE: usize = 4096;
pub const ANYPROXY_FLAG: &'static str = "$${anyproxy}";
pub const ANYPROXY_VERSION: &'static str = "anyproxy.0.1.0";
pub const ANYPROXY_TUNNEL_FLAG: &'static str = "$${anyproxy_TUNNEL}";
pub const ANYPROXY_TUNNEL2_FLAG: &'static str = "$${anyproxy_TUNNEL2}";

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

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHeartbeat {
    pub version: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct AnyproxyHeartbeatR {
    pub version: String,
}

pub enum AnyproxyHeaderType {
    Hello = 1,
    Heartbeat = 2,
    HeartbeatR = 3,
    Max = 4,
}

pub enum AnyproxyPack {
    Hello(AnyproxyHello),
    Heartbeat(AnyproxyHeartbeat),
    HeartbeatR(AnyproxyHeartbeatR),
}

pub async fn read_tunnel_hello(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> Result<Option<protopack::TunnelHello>> {
    buf_reader.start();

    let tunnel_hello = protopack::read_tunnel_hello(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow!("err:rollback"));
    }
    if tunnel_hello.is_err() {
        Ok(None)
    } else {
        Ok(tunnel_hello?)
    }
}

pub async fn read_tunnel2_hello(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> Result<Option<protopack2::TunnelHello>> {
    buf_reader.start();

    let tunnel_hello = protopack2::read_tunnel_hello(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow!("err:rollback"));
    }
    if tunnel_hello.is_err() {
        Ok(None)
    } else {
        Ok(tunnel_hello?)
    }
}

pub async fn read_heartbeat(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> Result<Option<AnyproxyHeartbeat>> {
    let anyproxy = read_pack_rollback(buf_reader, AnyproxyHeaderType::Heartbeat)
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let anyproxy = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::Heartbeat(heartbeat) => return Ok(Some(heartbeat)),
        _ => Err(anyhow!("read_heartbeat"))?,
    }
}

pub async fn read_heartbeat_r(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> Result<Option<AnyproxyHeartbeatR>> {
    let anyproxy = read_pack_rollback(buf_reader, AnyproxyHeaderType::HeartbeatR)
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let anyproxy = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::HeartbeatR(heartbeat_r) => return Ok(Some(heartbeat_r)),
        _ => Err(anyhow!("read_heartbeatR"))?,
    }
}

pub async fn read_hello(buf_reader: &mut BufReader<&StreamFlow>) -> Result<Option<AnyproxyHello>> {
    let anyproxy = read_pack_rollback(buf_reader, AnyproxyHeaderType::Hello)
        .await
        .map_err(|e| anyhow!("err:read_pack_rollback => e:{}", e))?;
    if anyproxy.is_none() {
        return Ok(None);
    }
    let anyproxy = anyproxy.unwrap();
    match anyproxy {
        AnyproxyPack::Hello(hello) => return Ok(Some(hello)),
        _ => Err(anyhow!("read_hello"))?,
    }
}

pub async fn write_pack<T: ?Sized>(
    buf_writer: &mut BufWriter<&StreamFlow>,
    typ: AnyproxyHeaderType,
    value: &T,
) -> Result<()>
where
    T: ser::Serialize,
{
    let slice = toml::to_vec(value).map_err(|e| anyhow!("err:toml::to_vec => e:{}", e))?;
    let header_slice = toml::to_vec(&AnyproxyHeader {
        header_type: typ as u8,
        body_size: slice.len() as u16,
    })?;
    buf_writer
        .write(ANYPROXY_FLAG.as_bytes())
        .await
        .map_err(|e| anyhow!("err:buf_writer.write => e:{}", e))?;
    buf_writer
        .write_u16(header_slice.len() as u16)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write_u16 => e:{}", e))?;
    buf_writer
        .write(&header_slice)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write => e:{}", e))?;
    buf_writer
        .write(&slice)
        .await
        .map_err(|e| anyhow!("err:buf_writer.write => e:{}", e))?;
    buf_writer
        .flush()
        .await
        .map_err(|e| anyhow!("err:buf_writer.flush => e:{}", e))?;
    Ok(())
}

pub async fn read_pack_rollback(
    buf_reader: &mut BufReader<&StreamFlow>,
    typ: AnyproxyHeaderType,
) -> Result<Option<AnyproxyPack>> {
    buf_reader.start();
    let anyproxy = read_pack(buf_reader, typ)
        .await
        .map_err(|e| anyhow!("err:read_pack => e:{}", e))?;
    if anyproxy.is_none() {
        if !buf_reader.rollback() {
            return Err(anyhow!("err:rollback"));
        }
        return Ok(None);
    }
    if !buf_reader.commit() {
        return Err(anyhow!("err:commit"));
    }
    Ok(anyproxy)
}

pub async fn read_pack(
    buf_reader: &mut BufReader<&StreamFlow>,
    typ: AnyproxyHeaderType,
) -> Result<Option<AnyproxyPack>> {
    let mut slice = [0u8; ANYPROXY_MAX_HEADER_SIZE];
    let flag_slice = &mut slice[..ANYPROXY_FLAG.len() as usize];
    buf_reader
        .read_exact(flag_slice)
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let flag_str = String::from_utf8_lossy(flag_slice);
    if ANYPROXY_FLAG != &flag_str {
        return Ok(None);
    }

    let header_size = buf_reader
        .read_u16()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
    if header_size as usize > ANYPROXY_MAX_HEADER_SIZE || header_size <= 0 {
        return Err(anyhow!(
            "err:header_size > ANYPROXY_MAX_HEADER_SIZE => header_size:{}",
            header_size
        ));
    }

    let header_slice = &mut slice[..header_size as usize];
    buf_reader
        .read_exact(header_slice)
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let header: AnyproxyHeader =
        toml::from_slice(header_slice).map_err(|e| anyhow!("err:AnyproxyPack header=> e:{}", e))?;

    if header.header_type >= AnyproxyHeaderType::Max as u8 {
        return Err(anyhow!("err:AnyproxyPack header_type"));
    }

    if header.body_size as usize > ANYPROXY_MAX_HEADER_SIZE || header.body_size <= 0 {
        return Err(anyhow!("err:AnyproxyPack body_size"));
    }

    let body_slice = &mut slice[..header.body_size as usize];
    buf_reader
        .read_exact(body_slice)
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;

    if header.header_type != typ as u8 {
        return Ok(None);
    }

    if header.header_type == AnyproxyHeaderType::Hello as u8 {
        let value: AnyproxyHello =
            toml::from_slice(body_slice).map_err(|e| anyhow!("err:AnyproxyPack=> e:{}", e))?;
        Ok(Some(AnyproxyPack::Hello(value)))
    } else if header.header_type == AnyproxyHeaderType::Heartbeat as u8 {
        let value: AnyproxyHeartbeat =
            toml::from_slice(body_slice).map_err(|e| anyhow!("err:AnyproxyPack=> e:{}", e))?;
        Ok(Some(AnyproxyPack::Heartbeat(value)))
    } else if header.header_type == AnyproxyHeaderType::HeartbeatR as u8 {
        let value: AnyproxyHeartbeatR =
            toml::from_slice(body_slice).map_err(|e| anyhow!("err:AnyproxyPack=> e:{}", e))?;
        Ok(Some(AnyproxyPack::HeartbeatR(value)))
    } else {
        Err(anyhow!("err:AnyproxyPack type"))
    }
}
