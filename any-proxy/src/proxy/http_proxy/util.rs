use crate::proxy::http_proxy::bitmap::update_bitset;
use crate::proxy::http_proxy::http_cache_file::{
    ProxyCache, ProxyCacheFileNodeExpires, ProxyCacheFileNodeManage,
};
use crate::proxy::http_proxy::http_cache_file_node::ProxyCacheFileNode;
use crate::proxy::http_proxy::http_stream_request::{
    CacheFileStatus, HttpBodyBuf, HttpBodyBufFilter, HttpCacheStatus, HttpResponseBody,
    HttpStreamRequest,
};
use any_base::file_ext::{unlink, FileCacheBytes};
use any_base::io::async_write_msg::AsyncWriteMsgExt;
use any_base::io::async_write_msg::{MsgWriteBuf, MsgWriteBufBytes};
use any_base::typ::{ArcMutex, ArcRwLockTokio};
use anyhow::anyhow;
use anyhow::Result;
use bytes::Bytes;
use http::Response;
use hyper::body::HttpBody;
use hyper::Body;
use std::collections::VecDeque;
use std::fs::File;
use std::io::IoSlice;
use std::io::Write;
use std::mem::swap;
use std::sync::Arc;
#[cfg(feature = "anyio-file")]
use std::time::Instant;

pub async fn response_body_read(
    r: &Arc<HttpStreamRequest>,
    response_body: &mut HttpResponseBody,
) -> Result<Option<HttpBodyBufFilter>> {
    match response_body {
        HttpResponseBody::Body(response_body) => {
            let data = response_body.data().await;
            if data.is_none() {
                return Ok(None);
            } else {
                let data = data.unwrap();
                match data {
                    Err(e) => {
                        return Err(anyhow!("err:stream_rx close => e:{}", e));
                    }
                    Ok(buf) => {
                        let buf = buf.to_bytes().unwrap();
                        let seek = r.ctx.get().r_in.curr_slice_start;
                        let size = buf.len() as u64;
                        Ok(Some(HttpBodyBufFilter::from_bytes(
                            MsgWriteBufBytes::from_bytes(buf),
                            seek,
                            size,
                        )))
                    }
                }
            }
        }
        HttpResponseBody::File(response_body) => {
            if response_body.is_none() {
                return Ok(None);
            }
            let r_ctx = &*r.ctx.get();
            let response_info = r_ctx.r_out.response_info.as_ref().unwrap();
            let mut buf = response_body.take().unwrap();
            let seek = r_ctx.r_in.curr_slice_start;
            let size = response_info.range.range_end - r_ctx.r_in.curr_slice_start + 1;
            buf.seek = seek;
            buf.size = size;
            Ok(Some(HttpBodyBufFilter::from_file(buf, seek, size)))
        }
    }
}

pub async fn write_cache_file(
    r: &Arc<HttpStreamRequest>,
    file: ArcMutex<File>,
    file_seek: u64,
    file_cache_bytes: FileCacheBytes,
) -> Result<usize> {
    let _session_id = r.session_id;
    #[cfg(feature = "anyio-file")]
    let cur_slice_index = r.ctx.get().r_in.cur_slice_index;
    tokio::task::spawn_blocking(move || {
        let size = file_cache_bytes.remaining();
        let file = &mut *file.get_mut();
        use std::io::Seek;
        #[cfg(feature = "anyio-file")]
        let start_time = Instant::now();
        file.seek(std::io::SeekFrom::Start(file_seek))?;
        if false {
            const MAX_WRITEV_BUFS: usize = 64;
            let mut iovs = [IoSlice::new(&[]); MAX_WRITEV_BUFS];
            let len = file_cache_bytes.chunks_vectored(&mut iovs);
            if len <= 0 {
                log::error!("len <= 0");
                return Ok(0);
            }
            file.write_vectored(&mut iovs[0..len])?;
        } else {
            let data = file_cache_bytes.chunk_all_bytes();
            file.write_all(data.as_ref())?;
        }

        #[cfg(feature = "anyio-file")]
        log::debug!(
            "r.session_id:{}, cur_slice_index:{}, data to file elapsed_time:{} => len:{}",
            _session_id,
            cur_slice_index,
            start_time.elapsed().as_millis(),
            size,
        );

        #[cfg(feature = "anyio-file")]
        if start_time.elapsed().as_millis() > 100 {
            log::info!(
                "r.session_id:{}, cur_slice_index:{}, data to file elapsed_time:{} => len:{}",
                _session_id,
                cur_slice_index,
                start_time.elapsed().as_millis(),
                size
            );
        }
        Ok(size)
    })
    .await?
}

pub async fn slice_update_bitset(
    r: &Arc<HttpStreamRequest>,
    cache_file_node: &Arc<ProxyCacheFileNode>,
) -> Result<bool> {
    let (range_start, range_end, skip_bitset_index, file_length) = {
        let r_ctx = &mut *r.ctx.get_mut();
        if r_ctx.r_in.bitmap_curr_slice_start <= 0
            || r_ctx.r_in.bitmap_curr_slice_start == r_ctx.r_in.bitmap_last_slice_start
        {
            return Ok(false);
        }

        let range_start = r_ctx.r_in.bitmap_last_slice_start;
        let range_end = r_ctx.r_in.bitmap_curr_slice_start - 1;
        let skip_bitset_index = r_ctx.r_in.skip_bitset_index;
        let file_length = r_ctx.r_in.range.raw_content_length;
        (range_start, range_end, skip_bitset_index, file_length)
    };

    let bitmap = cache_file_node.ctx_thread.get().bitmap.clone();
    let is_ok = {
        let bitmap_ = &mut *bitmap.get_mut();
        update_bitset(
            bitmap_,
            range_start,
            range_end,
            cache_file_node.cache_file_info.cache_file_slice,
            skip_bitset_index,
            file_length,
        )?
    };

    if is_ok {
        cache_file_node
            .ctx_thread
            .get_mut()
            .bitmap_to_file
            .push_back(bitmap);
    }
    Ok(is_ok)
}

pub async fn bitmap_to_cache_file(
    r: &Arc<HttpStreamRequest>,
    cache_file_node: &Arc<ProxyCacheFileNode>,
) -> Result<()> {
    let cache_file_node_ctx = cache_file_node.ctx_thread.clone();
    let bitmap_start = cache_file_node.fix.bitmap_start;
    let file = cache_file_node.get_file_ext().file.clone();
    #[cfg(feature = "anyio-file")]
    let session_id = r.session_id;
    let ret: Result<()> = tokio::task::spawn_blocking(move || {
        let bitmap = {
            let bitmap = {
                let cache_file_node_ctx = &mut *cache_file_node_ctx.get_mut();
                let bitmap = cache_file_node_ctx.bitmap_to_file.pop_front();
                cache_file_node_ctx.bitmap_to_file.clear();
                bitmap
            };
            if bitmap.is_none() {
                return Ok(());
            }
            bitmap.unwrap().get().clone()
        };
        let file = &mut *file.get_mut();
        use std::io::Seek;
        #[cfg(feature = "anyio-file")]
        let start_time = Instant::now();
        file.seek(std::io::SeekFrom::Start(bitmap_start as u64))?;
        file.write_all(bitmap.as_slice())?;
        #[cfg(feature = "anyio-file")]
        if start_time.elapsed().as_millis() > 100 {
            log::info!(
                "r.session_id:{}, bitmap to file time:{}",
                session_id,
                start_time.elapsed().as_millis()
            );
        }
        Ok(())
    })
    .await?;
    ret?;

    let r_ctx = &mut *r.ctx.get_mut();
    r_ctx.r_in.bitmap_last_slice_start = r_ctx.r_in.bitmap_curr_slice_start;
    Ok(())
}

pub async fn write_body_to_client(
    r: &Arc<HttpStreamRequest>,
    body_buf: Option<HttpBodyBufFilter>,
    client_write_tx: &mut any_base::stream_channel_write::Stream,
) -> Result<()> {
    if body_buf.is_none() {
        return Ok(());
    }
    let body_buf = body_buf.unwrap();
    match body_buf.buf {
        HttpBodyBuf::Bytes(buf) => {
            let buf = MsgWriteBuf::from_bytes(buf.to_bytes());
            let n = client_write_tx
                .write_msg(buf)
                .await
                .map_err(|e| anyhow!("err:Bytes client_write_tx.write_msg => e:{}", e))?;
            //___wait___是否需要继续接收流进行存储
            if n == 0 {
                return Err(anyhow!("err:Bytes client close"));
            }
        }
        HttpBodyBuf::File(mut buf_file) => {
            //___wait___
            // r.header_ext
            //     .get_mut()
            //     .insert(buf.file_fd, buf.file_ext.clone());
            let mut wait_rx = Vec::with_capacity(10);
            loop {
                if !buf_file.has_remaining() {
                    break;
                }
                log::debug!(
                    "r.session_id:{}, HttpBodyBuf buf_file.seek:{}, buf_file.size:{}",
                    r.session_id,
                    buf_file.seek,
                    buf_file.size
                );
                let (size, mut buf) = buf_file.to_msg_write_buf();
                buf_file.advance(size);
                if let MsgWriteBuf::File(buf) = &mut buf {
                    let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                    buf.notify_tx.as_mut().unwrap().push(tx);
                    wait_rx.push(rx);
                }
                let n = client_write_tx
                    .write_msg(buf)
                    .await
                    .map_err(|e| anyhow!("err:File client_write_tx.write_msg => e:{}", e))?;
                if n == 0 {
                    return Err(anyhow!("err:File client close"));
                }
            }
            log::debug!("wait file start:{}", r.local_cache_req_count);
            for mut rx in wait_rx {
                let _ = rx.recv().await;
            }
            log::debug!("wait file end:{}", r.local_cache_req_count);
        }
    }
    Ok(())
}

pub async fn create_cache_file(
    r: &Arc<HttpStreamRequest>,
    cache_file_node_manage: &ArcRwLockTokio<ProxyCacheFileNodeManage>,
    raw_content_length: u64,
) -> Result<()> {
    log::trace!(target: "ext", "r.session_id:{}-{}, Response create file",
                r.session_id, r.local_cache_req_count);
    let (
        add_cache_file_size,
        client_uri,
        cache_file_info,
        response_info,
        response,
        cache_file_node_version,
    ) = {
        let r_ctx = &mut *r.ctx.get_mut();
        let mut response = Response::builder().body(Body::default())?;
        *response.status_mut() = r_ctx.r_out.status_upstream.clone();
        *response.version_mut() = r_ctx.r_out.version_upstream.clone();
        *response.headers_mut() = r_ctx.r_out.headers_upstream.clone();
        let response_info = r_ctx.r_out.response_info.clone().unwrap();
        let add_cache_file_size =
            response_info.range.raw_content_length as i64 - raw_content_length as i64;
        (
            add_cache_file_size,
            r_ctx.r_in.uri.clone(),
            r.http_cache_file.cache_file_info.clone(),
            response_info,
            response,
            r.http_cache_file.ctx_thread.get().cache_file_node_version,
        )
    };
    let cache_file_node = ProxyCacheFileNode::create_file(
        client_uri.clone(),
        cache_file_info.clone(),
        response_info.clone(),
        response,
        cache_file_node_version,
    )
    .await?;
    let cache_file_node = Arc::new(cache_file_node);

    let is_ok = r
        .http_cache_file
        .set_cache_file_node(cache_file_node.clone())
        .await?;
    if is_ok {
        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
        cache_file_node_manage.version_expires += 1;

        let version_expires = cache_file_node_manage.version_expires;
        let md5 = cache_file_node.cache_file_info.md5.clone();
        let proxy_cache_path = cache_file_node.cache_file_info.proxy_cache_path.clone();
        let proxy_cache = r.http_cache_file.proxy_cache.as_ref().unwrap();
        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
        proxy_cache_ctx.curr_size += add_cache_file_size;
        proxy_cache_ctx
            .cache_file_node_expires
            .push_back(ProxyCacheFileNodeExpires::new(
                md5,
                version_expires,
                proxy_cache_path,
                client_uri,
                response_info.range.raw_content_length,
                response_info.expires_time,
            ));
        r.http_cache_file.ctx_thread.get_mut().cache_file_node = Some(cache_file_node);
        return Ok(());
    }

    r.ctx.get_mut().r_out.is_cache_err = true;

    #[cfg(feature = "anyio-file")]
    let session_id = r.session_id;
    let _: Result<()> = tokio::task::spawn_blocking(move || {
        #[cfg(feature = "anyio-file")]
        let start_time = Instant::now();
        let file_ext = cache_file_node.get_file_ext();
        file_ext.unlink();
        #[cfg(feature = "anyio-file")]
        if start_time.elapsed().as_millis() > 100 {
            log::info!(
                "r.session_id:{}, remove_file file time:{} => {:?}",
                session_id,
                start_time.elapsed().as_millis(),
                cache_file_info.proxy_cache_path_tmp.as_str()
            );
        }
        Ok(())
    })
    .await?;

    return Ok(());
}

pub async fn update_expired_cache_file(
    r: &Arc<HttpStreamRequest>,
    cache_file_node_manage: &ArcRwLockTokio<ProxyCacheFileNodeManage>,
    cache_file_node: &Arc<ProxyCacheFileNode>,
) -> Result<bool> {
    let out_response_info = r.ctx.get().r_out.response_info.clone().unwrap();
    {
        let cache_file_node_response_info = &cache_file_node.response_info;
        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
        let cache_file_node_ctx = &mut *cache_file_node.ctx_thread.get_mut();
        match &cache_file_node_ctx.cache_file_status {
            &CacheFileStatus::Expire => {}
            &CacheFileStatus::Exist => {
                return Ok(true);
            }
        }
        log::trace!(target: "ext", "r.session_id:{}-{}, Response Expire",
                    r.session_id, r.local_cache_req_count);
        let expires_time = cache_file_node_ctx.expires_time;
        if !(cache_file_node_response_info.last_modified_time
            == out_response_info.last_modified_time
            && cache_file_node_response_info.last_modified == out_response_info.last_modified
            && cache_file_node_response_info.e_tag == out_response_info.e_tag
            && cache_file_node_response_info.range.raw_content_length
                == out_response_info.range.raw_content_length
            && out_response_info.expires_time > expires_time)
        {
            return Ok(false);
        }
        log::trace!(target: "ext", "r.session_id:{}-{}, Response update Expire",
                    r.session_id, r.local_cache_req_count);
        cache_file_node_manage.is_upstream = false;
        let mut upstream_waits = VecDeque::with_capacity(10);
        swap(
            &mut upstream_waits,
            &mut cache_file_node_manage.upstream_waits,
        );
        for tx in upstream_waits {
            let _ = tx.send(());
        }

        cache_file_node_ctx.cache_control_time = out_response_info.cache_control_time;
        cache_file_node_ctx.expires_time = out_response_info.expires_time;
        cache_file_node_ctx.cache_file_status = CacheFileStatus::Exist;
    }

    let file_head_time = ProxyCacheFileNode::get_file_head_time_str(
        out_response_info.cache_control_time,
        out_response_info.expires_time,
    );
    let file = cache_file_node.get_file_ext().file.clone();
    #[cfg(feature = "anyio-file")]
    let session_id = r.session_id;
    let ret: Result<()> = tokio::task::spawn_blocking(move || {
        use std::io::Seek;
        //更新文件时间头
        let file = &mut *file.get_mut();
        #[cfg(feature = "anyio-file")]
        let start_time = Instant::now();
        file.seek(std::io::SeekFrom::Start(0))?;
        file.write_all(file_head_time.as_slice())?;

        #[cfg(feature = "anyio-file")]
        if start_time.elapsed().as_millis() > 100 {
            log::info!(
                "r.session_id:{}, update file head time:{}",
                session_id,
                start_time.elapsed().as_millis()
            );
        }
        Ok(())
    })
    .await?;
    ret?;
    return Ok(true);
}

pub async fn update_or_create_cache_file(r: &Arc<HttpStreamRequest>) -> Result<()> {
    let (http_cache_status, cache_file_node_manage, cache_file_node) = {
        if !r.ctx.get().r_out.is_cache {
            let cache_file_node_manage = r
                .http_cache_file
                .ctx_thread
                .get()
                .cache_file_node_manage
                .clone();
            if cache_file_node_manage.is_none().await {
                return Ok(());
            }
            let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;
            cache_file_node_manage.is_upstream = false;
            let mut upstream_waits = VecDeque::with_capacity(10);
            swap(
                &mut upstream_waits,
                &mut cache_file_node_manage.upstream_waits,
            );
            for tx in upstream_waits {
                let _ = tx.send(());
            }
            return Ok(());
        }
        let req_cache_file_ctx = r.http_cache_file.ctx_thread.get();
        (
            r.ctx.get().r_in.http_cache_status.clone(),
            req_cache_file_ctx.cache_file_node_manage.clone(),
            req_cache_file_ctx.cache_file_node.clone(),
        )
    };
    match http_cache_status {
        HttpCacheStatus::Create => {
            create_cache_file(&r, &cache_file_node_manage, 0).await?;
        }
        HttpCacheStatus::Expired => {
            let cache_file_node = cache_file_node.unwrap();
            if update_expired_cache_file(&r, &cache_file_node_manage, &cache_file_node).await? {
                return Ok(());
            }
            let raw_content_length = cache_file_node.response_info.range.raw_content_length;
            create_cache_file(&r, &cache_file_node_manage, raw_content_length).await?;
        }
        HttpCacheStatus::Miss => {
            return Ok(());
        }
        HttpCacheStatus::Hit => {
            return Ok(());
        }
        HttpCacheStatus::Bypass => {
            return Ok(());
        }
    }
    return Ok(());
}

pub async fn del_expires_cache_file(md5: &Bytes, proxy_cache: &Arc<ProxyCache>) -> Result<()> {
    if proxy_cache.cache_conf.max_size <= 0 {
        return Ok(());
    }

    let cache_file_node_expires = {
        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
        if proxy_cache_ctx.curr_size < proxy_cache.cache_conf.max_size {
            return Ok(());
        }
        let cache_file_node_expires = proxy_cache_ctx.cache_file_node_expires.pop_front();
        if cache_file_node_expires.is_none() {
            log::warn!("cache_file_node_expires.is_none()");
            return Ok(());
        }
        let cache_file_node_expires = cache_file_node_expires.unwrap();
        if cache_file_node_expires.md5 == md5 {
            proxy_cache_ctx
                .cache_file_node_expires
                .push_back(cache_file_node_expires);
            return Ok(());
        }
        cache_file_node_expires
    };

    let manage = proxy_cache
        .cache_file_node_map
        .get()
        .get(&cache_file_node_expires.md5)
        .cloned();
    if manage.is_none() {
        return Ok(());
    }
    let manage = manage.unwrap();
    let manage = &mut *manage.get_mut().await;

    if cache_file_node_expires.version_expires != manage.version_expires {
        return Ok(());
    }
    if manage.cache_file_node.is_some() {
        let cache_file_node = manage.cache_file_node.as_ref().unwrap();
        if !cache_file_node.is_file_node_expires_del() {
            proxy_cache
                .ctx
                .get_mut()
                .cache_file_node_expires
                .push_back(cache_file_node_expires);
            return Ok(());
        }

        let file_ext = cache_file_node.get_file_ext();
        file_ext.unlink();
        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
        proxy_cache_ctx.curr_size -= cache_file_node.response_info.range.raw_content_length as i64;
    } else {
        if let Err(e) = unlink(cache_file_node_expires.proxy_cache_path.as_str()) {
            log::error!(
                "err:unlink => path:{}, err:{}",
                cache_file_node_expires.proxy_cache_path.as_str(),
                e
            );
        }
        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
        proxy_cache_ctx.curr_size -= cache_file_node_expires.raw_content_length as i64;
    }
    proxy_cache
        .cache_file_node_map
        .get_mut()
        .remove(&cache_file_node_expires.md5);
    log::debug!(target: "ext", "del file uri:{}, path:{}", cache_file_node_expires.uri, cache_file_node_expires.proxy_cache_path);

    return Ok(());
}
