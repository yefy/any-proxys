use crate::config::http_server_stream_test::HttpServerStreamTestConfig;
use crate::proxy::util as proxy_util;
use crate::proxy::ServerArg;
#[cfg(unix)]
use any_base::io::async_stream::Stream;
use any_base::io::async_write_msg::{AsyncWriteMsg, AsyncWriteMsgExt, MsgReadBufFile};
use any_base::io::buf_reader::BufReader;
use any_base::stream_flow::StreamFlow;
use any_base::typ::ArcMutex;
use anyhow::anyhow;
use anyhow::Result;
use std::io::Read;
use tokio::io::{AsyncReadExt, AsyncWriteExt};

pub async fn http_server_handle(
    arg: ServerArg,
    client_buf_reader: BufReader<StreamFlow>,
) -> Result<()> {
    let scc = proxy_util::parse_proxy_domain(
        &arg,
        move || async { Ok(None) },
        || async { Ok("www.example.cn".into()) },
    )
    .await?;

    arg.stream_info
        .get_mut()
        .add_work_time1("parse_proxy_domain");

    let mut client_buf_stream = any_base::io::buf_stream::BufStream::from(
        any_base::io::buf_writer::BufWriter::new(client_buf_reader),
    );
    #[cfg(not(unix))]
    let socket_fd = 0;
    #[cfg(unix)]
    let socket_fd = client_buf_stream.raw_fd();

    let HttpServerStreamTestConfig {
        tcp_nopush: _tcp_nopush,
        sendfile_max_write_size,
        buffer_len,
        is_open_sendfile,
        sendfile_sleep_mil_time,
        is_directio: _is_directio,
        file_name,
    } = {
        let scc = scc.get();
        use crate::config::http_server_stream_test;
        let http_server_stream_test = http_server_stream_test::currs_conf(scc.http_server_confs());
        log::info!("config:{:?}", http_server_stream_test.config);
        http_server_stream_test.config.clone()
    };

    log::info!("accept socket_fd:{}", socket_fd);
    scopeguard::defer! {
        log::info!("close socket_fd:{}", socket_fd);
    }

    #[cfg(unix)]
    let mut is_set_tcp_nopush = false;

    loop {
        let mut vec = vec![0u8; 8192 * 2];
        let mut size = 0;
        loop {
            let n = client_buf_stream.read(&mut vec.as_mut_slice()).await?;
            size += n;
            if size > 4 {
                if vec[size - 4] == b'\r'
                    && vec[size - 3] == b'\n'
                    && vec[size - 2] == b'\r'
                    && vec[size - 1] == b'\n'
                {
                    //log::info!("req header ok");
                    break;
                }
            }
        }

        let file_name = format!(
            "/root/Desktop/fdisk/nginx/nginx-1.18.0/nginx/html/{}",
            file_name
        );
        let file = std::fs::File::open(&file_name)
            .map_err(|e| anyhow!("err:file.open => file_name:{}, e:{}", file_name, e))?;

        #[cfg(not(unix))]
        let file_fd = 0;
        #[cfg(unix)]
        use std::os::unix::io::AsRawFd;
        #[cfg(unix)]
        let file_fd = file.as_raw_fd();

        let file_len = file
            .metadata()
            .map_err(|e| anyhow!("err:file.metadata => file_name:{}, e:{}", file_name, e))?
            .len();
        let file = ArcMutex::new(file);

        let mut header = Vec::new();
        header.extend_from_slice(b"HTTP/1.1 200 OK\r\n");
        header.extend_from_slice(b"Server: anyproxy/v2.0.0\r\n");
        header.extend_from_slice(b"Date: Fri, 26 Jan 2024 16:50:13 GMT\r\n");
        header.extend_from_slice(b"Content-Type: text/html\r\n");
        header.extend_from_slice(format!("Content-Length: {}\r\n", file_len).as_bytes());
        header.extend_from_slice(b"Last-Modified: Tue, 21 Apr 2020 14:09:01 GMT\r\n");
        header.extend_from_slice(b"Connection: keep-alive\r\n");
        header.extend_from_slice(b"ETag: 5e9efe7d-264\r\n");
        header.extend_from_slice(b"Accept-Ranges: bytes\r\n\r\n");

        client_buf_stream.write_all(&header).await?;
        //log::info!("rsp header:{}", String::from_utf8(header)?);

        if is_open_sendfile && socket_fd > 0 && file_fd > 0 {
            #[cfg(unix)]
            if !is_set_tcp_nopush && _tcp_nopush {
                is_set_tcp_nopush = true;
                use any_base::util::set_tcp_nopush_;
                if socket_fd > 0 {
                    //log::info!("set_tcp_nopush_");
                    set_tcp_nopush_(socket_fd, true);
                }
            }

            if client_buf_stream.write_cache_size() > 0 {
                client_buf_stream.flush().await?;
            }

            let mut buf_file = MsgReadBufFile::new(file.clone(), file_fd, 0, file_len);
            loop {
                if !buf_file.has_remaining() {
                    break;
                }
                let (_, file_fd, seek, size) = buf_file.get2(sendfile_max_write_size);
                let n = client_buf_stream.sendfile(file_fd, seek, size).await?;
                //log::info!("size:{}, n:{}", size, n);
                if n == 0 && sendfile_sleep_mil_time > 0 {
                    tokio::time::sleep(tokio::time::Duration::from_millis(sendfile_sleep_mil_time))
                        .await;
                }
                buf_file.advance(n);
            }
            client_buf_stream.flush().await?;
            continue;
        }

        #[cfg(unix)]
        if _is_directio {
            use any_base::util::directio_on;
            directio_on(file_fd)?;
        }

        loop {
            let file = file.clone();
            let data: Result<(usize, Vec<u8>)> = tokio::task::spawn_blocking(move || {
                let mut buffer = vec![0u8; buffer_len];
                let size = file
                    .get_mut()
                    .read(&mut buffer.as_mut_slice()[..])
                    .map_err(|e| anyhow!("err:file.read => e:{}", e))?;
                Ok((size, buffer))
            })
            .await?;
            let (size, mut buffer) = data?;

            if size > 0 {
                if size != buffer_len {
                    unsafe { buffer.set_len(size) }
                }
                client_buf_stream.write_all(buffer.as_ref()).await?;
            }
            if size != buffer_len {
                break;
            }
        }
        client_buf_stream.flush().await?;
        continue;
    }
}
