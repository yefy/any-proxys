use crate::io::buf_reader::BufReader;
use crate::stream::stream_flow::StreamFlow;
use tokio::io::AsyncReadExt;

pub async fn read_domain(
    buf_reader: &mut BufReader<&StreamFlow>,
) -> anyhow::Result<Option<String>> {
    buf_reader.start();
    let domain = read_do_domain(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow::anyhow!("err:rollback"));
    }
    if domain.is_err() {
        Ok(None)
    } else {
        Ok(Some(domain?))
    }
}

async fn read_do_domain(buf_reader: &mut BufReader<&StreamFlow>) -> anyhow::Result<String> {
    let mut slice = [0u8; 4096];
    let content_type = buf_reader.read_u8().await?;
    log::trace!("content_type:{}", content_type);
    if content_type != 22 {
        return Err(anyhow::anyhow!(
            "err:content_type => content_type:{}",
            content_type
        ));
    }

    let _content_version = buf_reader.read_exact(&mut slice[..2]).await?;
    let _content_length = buf_reader.read_exact(&mut slice[..2]).await?;
    let handshake_type = buf_reader.read_u8().await?;
    log::trace!("handshake_type:{}", handshake_type);
    if handshake_type != 1 {
        return Err(anyhow::anyhow!(
            "err:handshake_type => handshake_type:{}",
            handshake_type
        ));
    }
    let _handshake_version = buf_reader.read_exact(&mut slice[..3]).await?;
    let _handshake_length = buf_reader.read_exact(&mut slice[..2]).await?;
    let _random = buf_reader.read_exact(&mut slice[..32]).await?;
    let session_id_len = buf_reader.read_u8().await?;
    if session_id_len > 0 {
        let _session_id = buf_reader
            .read_exact(&mut slice[..session_id_len as usize])
            .await?;
    }
    let cipher_suites_length = buf_reader.read_u16().await?;
    if cipher_suites_length > 0 {
        let _cipher_suites = buf_reader
            .read_exact(&mut slice[..cipher_suites_length as usize])
            .await?;
    }
    let compression_methods_length = buf_reader.read_u8().await?;
    if compression_methods_length > 0 {
        let _compression_methods = buf_reader
            .read_exact(&mut slice[..compression_methods_length as usize])
            .await?;
    }
    let _extensions_length = buf_reader.read_u16().await?;
    loop {
        let extension_type = buf_reader.read_u16().await?;
        let extension_len = buf_reader.read_u16().await?;
        if extension_type == 0 {
            let _server_name_list_len = buf_reader.read_u16().await?;
            loop {
                let server_name_type = buf_reader.read_u8().await?;
                let server_name_len = buf_reader.read_u16().await?;
                if server_name_type == 0 {
                    log::trace!("server_name_type == 0");
                    if server_name_len <= 0 {
                        return Err(anyhow::anyhow!("err:server_name_len <= 0"));
                    }
                    let server_name_data = &mut slice[..server_name_len as usize];
                    buf_reader.read_exact(server_name_data).await?;
                    let domain = String::from_utf8_lossy(server_name_data).to_string();
                    log::trace!("domain:{}", domain);
                    return Ok(domain);
                } else {
                    if server_name_len > 0 {
                        let _ = buf_reader
                            .read_exact(&mut slice[..server_name_len as usize])
                            .await?;
                    }
                }
            }
        } else {
            if extension_len > 0 {
                let _ = buf_reader
                    .read_exact(&mut slice[..extension_len as usize])
                    .await?;
            }
        }
    }
}
