use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_header_parse::{
    cache_control_time, content_length, content_range, e_tag, last_modified,
};
use crate::proxy::http_proxy::http_stream::HttpStream;
use crate::proxy::http_proxy::http_stream_request::{HttpResponseInfo, HttpStreamRequest};
use any_base::typ::ArcRwLockTokio;
use anyhow::anyhow;
use anyhow::Result;
use http::HeaderValue;
use lazy_static::lazy_static;
use std::sync::Arc;

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub async fn set_header_filter(plugin: PluginHttpFilter) -> Result<()> {
    if HEADER_FILTER_NEXT.is_none().await {
        HEADER_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

pub async fn http_filter_header_parse(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!("r.session_id:{}, http_filter_header_parse", r.session_id);
    do_http_filter_header_parse(&r).await?;
    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        (next)(r).await?;
    }
    return Ok(());
}

pub async fn do_http_filter_header_parse(r: &HttpStreamRequest) -> Result<()> {
    let is_upstream_cache = {
        let r_ctx = &mut *r.ctx.get_mut();
        let mut content_range = content_range(&r_ctx.r_out.headers)
            .map_err(|e| anyhow!("err:content_length =>e:{}", e))?;

        let content_length = content_length(&r_ctx.r_out.headers)
            .map_err(|e| anyhow!("err:content_length =>e:{}", e))?;

        if content_range.is_range {
            if content_range.content_length != content_length {
                return Err(anyhow!(
                    "status:{:?}, version:{:?}, content_range:{:?}",
                    r_ctx.r_out.status,
                    r_ctx.r_out.version,
                    r_ctx.r_out.headers
                ));
            }
        } else {
            content_range.raw_content_length = content_length;
            content_range.content_length = content_length;
            if content_length > 0 {
                content_range.range_start = 0;
                content_range.range_end = content_length - 1;
            }
        }

        let expires = {
            let scc = r.scc.get();
            let net_core_conf = scc.net_core_conf();
            net_core_conf.expires
        };

        if expires > 0 && r_ctx.r_out.headers.get("cache-control").is_none() {
            r_ctx.r_out.headers.insert(
                "cache-control",
                HeaderValue::from_bytes(format!("max-age={}", expires).as_bytes())?,
            );
        }

        let e_tag = e_tag(&r_ctx.r_out.headers).map_err(|e| anyhow!("err:e_tag =>e:{}", e))?;
        let (last_modified, last_modified_time) = last_modified(&r_ctx.r_out.headers)
            .map_err(|e| anyhow!("err:last_modified =>e:{}", e))?;
        let (cache_control_time, expires_time, expires_time_sys) =
            cache_control_time(&r_ctx.r_out.headers)
                .map_err(|e| anyhow!("err:cache_control_time =>e:{}", e))?;

        use http::header::EXPIRES;
        if expires_time_sys.is_some() && r_ctx.r_out.headers.get(EXPIRES).is_none() {
            let v = httpdate::fmt_http_date(expires_time_sys.unwrap());
            r_ctx
                .r_out
                .headers
                .insert(EXPIRES, HeaderValue::from_str(&v)?);
        }

        r_ctx.r_in.bitmap_curr_slice_start = r_ctx.r_in.curr_slice_start;
        r_ctx.r_in.bitmap_last_slice_start = r_ctx.r_in.curr_slice_start;

        r_ctx.r_out.is_cache_err = false;
        r_ctx.r_out.is_cache = !(!r_ctx.is_request_cache
            || cache_control_time <= 0
            || e_tag.is_empty()
            || last_modified.is_empty());

        let is_upstream_cache =
            !(cache_control_time <= 0 || e_tag.is_empty() || last_modified.is_empty());

        let response_info = Arc::new(HttpResponseInfo {
            last_modified_time,
            last_modified,
            e_tag,
            cache_control_time,
            expires_time,
            range: content_range,
            head: r_ctx.r_out.head.clone(),
        });
        r_ctx.r_out.response_info = Some(response_info);
        is_upstream_cache
    };

    if r.ctx.get_mut().is_try_cache {
        HttpStream::set_is_last_upstream_cache(r, is_upstream_cache).await?;
    }

    return Ok(());
}
