use any_base::io_rb::buf_reader::BufReader;
use anyhow::anyhow;
use anyhow::Result;
use tokio::io::AsyncRead;
use tokio::io::AsyncReadExt;

pub async fn read_domain<R: AsyncRead + Unpin>(
    buf_reader: &mut BufReader<R>,
) -> Result<Option<String>> {
    buf_reader.start();
    let domain = read_do_domain(buf_reader).await;
    if !buf_reader.rollback() {
        return Err(anyhow!("err:rollback"));
    }
    if domain.is_err() {
        Ok(None)
    } else {
        Ok(Some(domain?))
    }
}

async fn read_do_domain<R: AsyncRead + Unpin>(buf_reader: &mut R) -> Result<String> {
    let mut slice = [0u8; 4096];
    let content_type = buf_reader
        .read_u8()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u8 => e:{}", e))?;
    log::trace!("content_type:{}", content_type);
    if content_type != 22 {
        return Err(anyhow!("err:content_type => content_type:{}", content_type));
    }

    let _content_version = buf_reader
        .read_exact(&mut slice[..2])
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let _content_length = buf_reader
        .read_exact(&mut slice[..2])
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let handshake_type = buf_reader
        .read_u8()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u8 => e:{}", e))?;
    log::trace!("handshake_type:{}", handshake_type);
    if handshake_type != 1 {
        return Err(anyhow!(
            "err:handshake_type => handshake_type:{}",
            handshake_type
        ));
    }
    let _handshake_version = buf_reader
        .read_exact(&mut slice[..3])
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let _handshake_length = buf_reader
        .read_exact(&mut slice[..2])
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let _random = buf_reader
        .read_exact(&mut slice[..32])
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    let session_id_len = buf_reader
        .read_u8()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u8 => e:{}", e))?;
    if session_id_len > 0 {
        let _session_id = buf_reader
            .read_exact(&mut slice[..session_id_len as usize])
            .await
            .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    }
    let cipher_suites_length = buf_reader
        .read_u16()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
    if cipher_suites_length > 0 {
        let _cipher_suites = buf_reader
            .read_exact(&mut slice[..cipher_suites_length as usize])
            .await
            .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    }
    let compression_methods_length = buf_reader
        .read_u8()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u8 => e:{}", e))?;
    if compression_methods_length > 0 {
        let _compression_methods = buf_reader
            .read_exact(&mut slice[..compression_methods_length as usize])
            .await
            .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
    }
    let _extensions_length = buf_reader
        .read_u16()
        .await
        .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
    loop {
        let extension_type = buf_reader
            .read_u16()
            .await
            .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
        let extension_len = buf_reader
            .read_u16()
            .await
            .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
        if extension_type == 0 {
            let _server_name_list_len = buf_reader
                .read_u16()
                .await
                .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
            loop {
                let server_name_type = buf_reader
                    .read_u8()
                    .await
                    .map_err(|e| anyhow!("err:buf_reader.read_u8 => e:{}", e))?;
                let server_name_len = buf_reader
                    .read_u16()
                    .await
                    .map_err(|e| anyhow!("err:buf_reader.read_u16 => e:{}", e))?;
                if server_name_type == 0 {
                    log::trace!("server_name_type == 0");
                    if server_name_len <= 0 {
                        return Err(anyhow!("err:server_name_len <= 0"));
                    }
                    let server_name_data = &mut slice[..server_name_len as usize];
                    buf_reader
                        .read_exact(server_name_data)
                        .await
                        .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
                    let domain = String::from_utf8_lossy(server_name_data).to_string();
                    log::trace!("domain:{}", domain);
                    return Ok(domain);
                } else {
                    if server_name_len > 0 {
                        let _ = buf_reader
                            .read_exact(&mut slice[..server_name_len as usize])
                            .await
                            .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
                    }
                }
            }
        } else {
            if extension_len > 0 {
                let _ = buf_reader
                    .read_exact(&mut slice[..extension_len as usize])
                    .await
                    .map_err(|e| anyhow!("err:buf_reader.read_exact => e:{}", e))?;
            }
        }
    }
}
