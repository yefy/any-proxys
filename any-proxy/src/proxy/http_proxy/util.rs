use crate::proxy::http_proxy::bitmap::update_bitset;
use crate::proxy::http_proxy::http_cache_file::{
    HttpCacheFile, ProxyCache, ProxyCacheFileNodeData, ProxyCacheFileNodeExpires,
    ProxyCacheFileNodeManage,
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
use std::sync::atomic::Ordering;
use std::sync::Arc;
#[cfg(feature = "anyio-file")]
use std::time::Instant;

pub async fn response_body_read(
    r: &Arc<HttpStreamRequest>,
    response_body: &mut HttpResponseBody,
) -> Result<Option<HttpBodyBufFilter>> {
    match response_body {
        HttpResponseBody::Body(response_body) => {
            if r.ctx.get().r_out.left_content_length <= 0 {
                return Ok(None);
            }
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
                        if buf.len() <= 0 {
                            return Ok(None);
                        }
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
    let curr_slice_index = r.ctx.get().r_in.curr_slice_index;
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
        log::debug!(target: "main",
            "r.session_id:{}, curr_slice_index:{}, data to file elapsed_time:{} => len:{}",
            _session_id,
            curr_slice_index,
            start_time.elapsed().as_millis(),
            size,
        );

        #[cfg(feature = "anyio-file")]
        if start_time.elapsed().as_millis() > 100 {
            log::info!(
                "r.session_id:{}, curr_slice_index:{}, data to file elapsed_time:{} => len:{}",
                _session_id,
                curr_slice_index,
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
                log::debug!(target: "main",
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
            log::debug!(target: "main", "wait file start: session_id:{}-{}", r.session_id, r.local_cache_req_count);
            for mut rx in wait_rx {
                let _ = rx.recv().await;
            }
            log::debug!(target: "main", "wait file end:session_id:{}-{}", r.session_id, r.local_cache_req_count);
        }
    }
    Ok(())
}

pub async fn create_cache_file(r: &Arc<HttpStreamRequest>, raw_content_length: u64) -> Result<()> {
    use crate::config::net_core_proxy;
    //当前可能是main  server local， ProxyCache必须到main_conf中读取
    let net_core_proxy = net_core_proxy::main_conf(r.scc.ms()).await;

    log::trace!(target: "ext", "r.session_id:{}-{}, Response create file",
                r.session_id, r.local_cache_req_count);
    let (
        add_cache_file_size,
        client_method,
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
            r_ctx.r_in.method.clone(),
            r_ctx.r_in.uri.clone(),
            r.http_cache_file.cache_file_info.clone(),
            response_info,
            response,
            r.http_cache_file.ctx_thread.get().cache_file_node_version,
        )
    };

    let proxy_cache_name = r.http_cache_file.proxy_cache.as_ref().unwrap().name.clone();
    let cache_file_node = ProxyCacheFileNode::create_file(
        client_method.clone(),
        client_uri.clone(),
        cache_file_info.clone(),
        response_info.clone(),
        response,
        cache_file_node_version,
        proxy_cache_name,
    )
    .await?;
    let cache_file_node = Arc::new(cache_file_node);
    let cache_file_node_data = Arc::new(ProxyCacheFileNodeData::new(None));
    {
        let (_, cache_file_node_manage) = HttpCacheFile::read_cache_file_node_manage(
            r.http_cache_file.proxy_cache.as_ref().unwrap(),
            &r.http_cache_file.cache_file_info.md5,
            net_core_proxy.cache_file_node_queue.clone(),
        );
        let cache_file_node_manage_ = cache_file_node_manage.clone();
        let cache_file_node_manage = &mut *cache_file_node_manage.get_mut().await;

        let is_ok = r
            .http_cache_file
            .set_cache_file_node(&r, cache_file_node.clone(), cache_file_node_manage)
            .await?;
        if is_ok {
            cache_file_node_manage.version_expires += 1;
            let cache_file_node_head = cache_file_node_manage.cache_file_node_head.as_ref();

            let version_expires = cache_file_node_manage.version_expires;
            let cache_file_node_version = cache_file_node_manage.cache_file_node_version;
            let md5 = cache_file_node_head.md5.clone();
            let trie_url = cache_file_node_head.trie_url.clone();
            let proxy_cache = r.http_cache_file.proxy_cache.as_ref().unwrap();

            let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
            proxy_cache_ctx.curr_size += add_cache_file_size;
            proxy_cache_ctx
                .cache_file_node_expires
                .push_back(ProxyCacheFileNodeExpires::new(
                    md5.clone(),
                    version_expires,
                    cache_file_node_version,
                    cache_file_node_head.clone(),
                ));
            net_core_proxy.trie_insert(trie_url, md5.clone())?;
            net_core_proxy.index_insert(md5, proxy_cache.cache_conf.name.clone());

            r.http_cache_file.ctx_thread.get_mut().cache_file_node = Some(cache_file_node);
            r.http_cache_file.ctx_thread.get_mut().cache_file_node_data =
                Some(cache_file_node_data);
            r.http_cache_file
                .ctx_thread
                .get_mut()
                .cache_file_node_manage = cache_file_node_manage_;
            return Ok(());
        }
    }

    r.ctx.get_mut().r_out.is_cache_err = true;

    use crate::config::common_core;
    let common_core_conf = common_core::main_conf(&r.scc.ms).await;
    let tmpfile_id = common_core_conf.tmpfile_id.fetch_add(1, Ordering::Relaxed);

    #[cfg(feature = "anyio-file")]
    let session_id = r.session_id;
    let _: Result<()> = tokio::task::spawn_blocking(move || {
        #[cfg(feature = "anyio-file")]
        let start_time = Instant::now();
        let file_ext = cache_file_node.get_file_ext();
        file_ext.unlink(Some(tmpfile_id));
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
        let expires_time = cache_file_node_manage.cache_file_node_head.expires_time();
        if !(cache_file_node_response_info.last_modified_time
            == out_response_info.last_modified_time
            && cache_file_node_response_info.last_modified == out_response_info.last_modified
            && cache_file_node_response_info.e_tag == out_response_info.e_tag
            && cache_file_node_response_info.range.raw_content_length
                == out_response_info.range.raw_content_length
            && out_response_info.expires_time > expires_time)
        {
            if log::log_enabled!(target: "is_ups", log::Level::Trace) {
                log::trace!(target: "is_ups", "session_id:{}, r_in url:{}", r.session_id, r.ctx.get().r_in.uri.to_string());
                log::trace!(target: "is_ups", "session_id:{}, r_in method:{}", r.session_id, r.ctx.get().r_in.method);
                log::trace!(target: "is_ups", "session_id:{}, r_in curr_upstream_method:{:?}", r.session_id, r.ctx.get().r_in.curr_upstream_method);
                log::trace!(target: "is_ups", "session_id:{}, r_out status:{}", r.session_id, r.ctx.get().r_out.status.as_u16());
                log::trace!(target: "is_ups", "session_id:{}, cache_file_node_response_info: last_modified_time:{}, last_modified:{}, e_tag:{}, raw_content_length:{}, expires_time:{}", r.session_id,
                            cache_file_node_response_info.last_modified_time,
                            cache_file_node_response_info.last_modified.to_str()?,
                            cache_file_node_response_info.e_tag.to_str()?,
                            cache_file_node_response_info.range.raw_content_length,
                            expires_time);
                log::trace!(target: "is_ups", "session_id:{}, out_response_info: last_modified_time:{}, last_modified:{}, e_tag:{}, raw_content_length:{}, expires_time:{}", r.session_id,
                            out_response_info.last_modified_time,
                            out_response_info.last_modified.to_str()?,
                            out_response_info.e_tag.to_str()?,
                            out_response_info.range.raw_content_length,
                            out_response_info.expires_time);
            }
            return Ok(false);
        }
        log::trace!(target: "ext", "r.session_id:{}-{}, Response update Expire",
                    r.session_id, r.local_cache_req_count);
        cache_file_node_manage.is_upstream = false;
        if r.ctx.get().is_upstream_add {
            r.ctx.get_mut().is_upstream_add = false;
            cache_file_node_manage
                .upstream_count
                .fetch_sub(1, Ordering::Relaxed);
        }
        let mut upstream_waits = VecDeque::with_capacity(10);
        swap(
            &mut upstream_waits,
            &mut cache_file_node_manage.upstream_waits,
        );
        for tx in upstream_waits {
            let _ = tx.send(());
        }

        cache_file_node_manage
            .cache_file_node_head
            .set_cache_control_time(out_response_info.cache_control_time);
        cache_file_node_manage
            .cache_file_node_head
            .set_expires_time(out_response_info.expires_time);
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

pub async fn check_miss(
    r: &Arc<HttpStreamRequest>,
    _cache_file_node_manage: &ArcRwLockTokio<ProxyCacheFileNodeManage>,
    cache_file_node: &Arc<ProxyCacheFileNode>,
) -> Result<bool> {
    let out_response_info = r.ctx.get().r_out.response_info.clone().unwrap();
    {
        let cache_file_node_response_info = &cache_file_node.response_info;
        log::trace!(target: "ext", "r.session_id:{}-{}, check_miss",
                    r.session_id, r.local_cache_req_count);
        if !(cache_file_node_response_info.last_modified_time
            == out_response_info.last_modified_time
            && cache_file_node_response_info.last_modified == out_response_info.last_modified
            && cache_file_node_response_info.e_tag == out_response_info.e_tag
            && cache_file_node_response_info.range.raw_content_length
                == out_response_info.range.raw_content_length)
        {
            return Ok(false);
        }
    }
    return Ok(true);
}

pub async fn update_expired_cache_file_304(
    r: &Arc<HttpStreamRequest>,
    cache_file_node_manage: &ArcRwLockTokio<ProxyCacheFileNodeManage>,
    cache_file_node: &Arc<ProxyCacheFileNode>,
    ups_response: &http::Response<Body>,
) -> Result<bool> {
    let e_tag = e_tag(ups_response.headers()).map_err(|e| anyhow!("err:e_tag =>e:{}", e))?;
    let (last_modified, last_modified_time) = last_modified(ups_response.headers())
        .map_err(|e| anyhow!("err:last_modified =>e:{}", e))?;
    let (cache_control_time, expires_time, _) = cache_control_time(ups_response.headers())
        .map_err(|e| anyhow!("err:cache_control_time =>e:{}", e))?;

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

        let _expires_time = cache_file_node_manage.cache_file_node_head.expires_time();
        if !(cache_file_node_response_info.last_modified_time == last_modified_time
            && cache_file_node_response_info.last_modified == last_modified
            && cache_file_node_response_info.e_tag == e_tag
            && expires_time > _expires_time)
        {
            return Ok(false);
        }
        log::trace!(target: "ext", "r.session_id:{}-{}, Response update Expire",
                    r.session_id, r.local_cache_req_count);
        cache_file_node_manage.is_upstream = false;
        if r.ctx.get().is_upstream_add {
            r.ctx.get_mut().is_upstream_add = false;
            cache_file_node_manage
                .upstream_count
                .fetch_sub(1, Ordering::Relaxed);
        }
        let mut upstream_waits = VecDeque::with_capacity(10);
        swap(
            &mut upstream_waits,
            &mut cache_file_node_manage.upstream_waits,
        );
        for tx in upstream_waits {
            let _ = tx.send(());
        }

        cache_file_node_manage
            .cache_file_node_head
            .set_cache_control_time(cache_control_time);
        cache_file_node_manage
            .cache_file_node_head
            .set_expires_time(expires_time);
        cache_file_node_ctx.cache_file_status = CacheFileStatus::Exist;
    }

    let file_head_time =
        ProxyCacheFileNode::get_file_head_time_str(cache_control_time, expires_time);
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
            if r.ctx.get().is_upstream_add {
                r.ctx.get_mut().is_upstream_add = false;
                cache_file_node_manage
                    .upstream_count
                    .fetch_sub(1, Ordering::Relaxed);
            }
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
            req_cache_file_ctx.cache_file_node(),
        )
    };
    match http_cache_status {
        HttpCacheStatus::Create => {
            log::trace!(target: "is_ups", "session_id:{}, cache_file_node Create", r.session_id);
            create_cache_file(&r, 0).await?;
        }
        HttpCacheStatus::Expired => {
            let cache_file_node = cache_file_node.unwrap();
            if update_expired_cache_file(&r, &cache_file_node_manage, &cache_file_node).await? {
                log::trace!(target: "is_ups", "session_id:{}, cache_file_node Expired", r.session_id);
                return Ok(());
            }

            {
                let r_ctx = &mut *r.ctx.get_mut();
                if r_ctx.r_out.status.is_server_error() {
                    r_ctx.r_out.is_cache_err = true;
                    log::trace!(target: "is_ups", "session_id:{}, cache_file_node Expired is_cache_err", r.session_id);
                    return Ok(());
                }
            }

            log::trace!(target: "is_ups", "session_id:{}, cache_file_node Expired to create", r.session_id);
            let raw_content_length = cache_file_node.response_info.range.raw_content_length;
            create_cache_file(&r, raw_content_length).await?;
        }
        HttpCacheStatus::Miss => {
            let cache_file_node = cache_file_node.unwrap();
            if check_miss(&r, &cache_file_node_manage, &cache_file_node).await? {
                log::trace!(target: "is_ups", "session_id:{}, cache_file_node Miss", r.session_id);
                return Ok(());
            }

            {
                let r_ctx = &mut *r.ctx.get_mut();
                if r_ctx.r_out.status.is_server_error() {
                    r_ctx.r_out.is_cache_err = true;
                    log::trace!(target: "is_ups", "session_id:{}, cache_file_node Miss is_cache_err", r.session_id);
                    return Ok(());
                }
            }

            log::trace!(target: "is_ups", "session_id:{}, cache_file_node Miss to create", r.session_id);
            let raw_content_length = cache_file_node.response_info.range.raw_content_length;
            create_cache_file(&r, raw_content_length).await?;
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

pub async fn update_or_create_cache_file_304(
    r: &Arc<HttpStreamRequest>,
    ups_response: &http::Response<Body>,
) -> Result<bool> {
    let (http_cache_status, cache_file_node_manage, cache_file_node) = {
        let req_cache_file_ctx = r.http_cache_file.ctx_thread.get();
        (
            r.ctx.get().r_in.http_cache_status.clone(),
            req_cache_file_ctx.cache_file_node_manage.clone(),
            req_cache_file_ctx.cache_file_node(),
        )
    };
    match http_cache_status {
        HttpCacheStatus::Create => {}
        HttpCacheStatus::Expired => {
            let cache_file_node = cache_file_node.unwrap();
            if update_expired_cache_file_304(
                &r,
                &cache_file_node_manage,
                &cache_file_node,
                ups_response,
            )
            .await?
            {
                return Ok(true);
            }
        }
        HttpCacheStatus::Miss => {}
        HttpCacheStatus::Hit => {}
        HttpCacheStatus::Bypass => {}
    }
    return Ok(false);
}

use crate::proxy::http_proxy::http_header_parse::{cache_control_time, e_tag, last_modified};
use any_base::module::module::Modules;
use std::path::Path;

pub async fn del_expires_cache_file(
    md5: &Bytes,
    proxy_cache: &Arc<ProxyCache>,
    ms: &Modules,
) -> Result<()> {
    let mut cache_file_node_expires = {
        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();

        let cache_file_node_expires = proxy_cache_ctx.cache_file_node_expires.pop_front();
        if cache_file_node_expires.is_none() {
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

    if cache_file_node_expires.cache_file_node_version != manage.cache_file_node_version {
        return Ok(());
    }

    if cache_file_node_expires.version_expires != manage.version_expires {
        cache_file_node_expires.version_expires = manage.version_expires;
        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
        proxy_cache_ctx
            .cache_file_node_expires
            .push_back(cache_file_node_expires);
        return Ok(());
    }

    if manage.cache_file_node_head.is_none() || !manage.cache_file_node_head.is_expires() {
        if proxy_cache.cache_conf.max_size <= 0 {
            let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
            proxy_cache_ctx
                .cache_file_node_expires
                .push_front(cache_file_node_expires);
            return Ok(());
        }

        {
            let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
            if proxy_cache_ctx.curr_size < proxy_cache.cache_conf.max_size {
                proxy_cache_ctx
                    .cache_file_node_expires
                    .push_front(cache_file_node_expires);
                return Ok(());
            }
        }
    }

    del_cache_file(manage, proxy_cache, ms).await?;

    return Ok(());
}

pub async fn del_md5(ms: &Modules) -> Result<()> {
    use crate::config::net_core_proxy;
    //当前可能是main  server local， ProxyCache必须到main_conf中读取
    let net_core_proxy = net_core_proxy::main_conf(ms).await;
    let md5 = net_core_proxy.del_md5.get_mut().pop_front();
    if md5.is_none() {
        return Ok(());
    }
    let md5 = md5.unwrap();
    let proxy_cache_names = net_core_proxy
        .proxy_cache_index_map
        .get()
        .get(&md5)
        .cloned();
    if proxy_cache_names.is_none() {
        return Ok(());
    }
    let proxy_cache_names = proxy_cache_names.unwrap();
    let proxy_cache_names = proxy_cache_names
        .get()
        .iter()
        .map(|data| data.clone())
        .collect::<Vec<String>>();
    for proxy_cache_name in proxy_cache_names {
        let proxy_cache = net_core_proxy
            .proxy_cache_map
            .get(&proxy_cache_name)
            .cloned();
        if proxy_cache.is_none() {
            continue;
        }
        let proxy_cache = proxy_cache.unwrap();

        log::trace!(target: "ext",
                    "del_cache_file md5:{}",
                   String::from_utf8_lossy(md5.as_ref()));

        let manage = proxy_cache.cache_file_node_map.get().get(&md5).cloned();
        if manage.is_none() {
            return Ok(());
        }
        let manage = manage.unwrap();
        let manage = &mut *manage.get_mut().await;
        del_cache_file(manage, &proxy_cache, ms).await?;
    }

    return Ok(());
}

pub async fn del_cache_file(
    manage: &mut ProxyCacheFileNodeManage,
    proxy_cache: &Arc<ProxyCache>,
    ms: &Modules,
) -> Result<()> {
    if manage.cache_file_node_head.is_none() {
        return Ok(());
    }
    let cache_file_node_head = manage.cache_file_node_head.clone();
    let proxy_cache_path = cache_file_node_head.proxy_cache_path.clone();
    let proxy_cache_path_tmp = cache_file_node_head.proxy_cache_path_tmp.clone();
    let cache_file_node_old = manage.cache_file_node.clone();

    use crate::config::common_core;
    let common_core_conf = common_core::main_conf(ms).await;
    let tmpfile_id = common_core_conf.tmpfile_id.fetch_add(1, Ordering::Relaxed);

    if Path::new(proxy_cache_path.as_str()).exists() {
        let mut tmp = proxy_cache_path_tmp.to_string();
        tmp.push_str(&format!("_{}_tmp", tmpfile_id));

        log::info!(target: "ext", "del_cache_file exists rename {},{}",
                   proxy_cache_path.as_str(), tmp.as_str());
        std::fs::rename(proxy_cache_path.as_str(), tmp.as_str())?;
        let _ = unlink(tmp.as_str(), None);
        if cache_file_node_old.is_some() {
            let cache_file_node_old = cache_file_node_old.unwrap();
            cache_file_node_old.file_ext.file_path.set(tmp.into());
            cache_file_node_old.file_ext.unlink(None);
        }

        let proxy_cache_ctx = &mut *proxy_cache.ctx.get_mut();
        proxy_cache_ctx.curr_size -= cache_file_node_head.raw_content_length as i64;
    }

    proxy_cache
        .cache_file_node_map
        .get_mut()
        .remove(&cache_file_node_head.md5);
    manage.cache_file_node_queue_clear();
    log::debug!(target: "ext", "del file uri:{}, path:{}", cache_file_node_head.uri, cache_file_node_head.proxy_cache_path);

    use crate::config::net_core_proxy;
    //当前可能是main  server local， ProxyCache必须到main_conf中读取
    let net_core_proxy = net_core_proxy::main_conf(ms).await;
    net_core_proxy.trie_del(&cache_file_node_head.trie_url, &cache_file_node_head.md5);
    net_core_proxy.index_del(&cache_file_node_head.md5, &proxy_cache.cache_conf.name);

    return Ok(());
}

pub async fn del_max_open_cache_file(ms: &Modules) -> Result<()> {
    use crate::config::net_core_proxy;
    let net_core_proxy_main_conf = net_core_proxy::main_conf(ms).await;
    if net_core_proxy_main_conf.proxy_max_open_file <= 0 {
        return Ok(());
    }
    let len = net_core_proxy_main_conf
        .cache_file_node_queue
        .get_mut()
        .len();
    if len <= net_core_proxy_main_conf.proxy_max_open_file {
        return Ok(());
    }

    let data = net_core_proxy_main_conf
        .cache_file_node_queue
        .get_mut()
        .pop_front();
    if data.is_none() {
        return Ok(());
    }
    let data = data.unwrap();

    if data.count.load(Ordering::Relaxed) > 0 {
        data.count.store(0, Ordering::Relaxed);
        net_core_proxy_main_conf
            .cache_file_node_queue
            .get_mut()
            .push_back(data);
        return Ok(());
    }

    data.node.set_nil();

    return Ok(());
}
