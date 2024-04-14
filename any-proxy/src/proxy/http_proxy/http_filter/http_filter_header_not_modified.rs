use crate::config::net_core_plugin::PluginHttpFilter;
use crate::proxy::http_proxy::http_stream_request::HttpStreamRequest;
use any_base::typ::ArcRwLockTokio;
use anyhow::anyhow;
use anyhow::Result;
use http::header::IF_MODIFIED_SINCE;
use http::header::{HeaderMap, IF_UNMODIFIED_SINCE};
use http::header::{IF_MATCH, IF_NONE_MATCH};
use http::HeaderName;
use hyper::StatusCode;
use lazy_static::lazy_static;
use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

lazy_static! {
    pub static ref HEADER_FILTER_NEXT: ArcRwLockTokio<PluginHttpFilter> = ArcRwLockTokio::default();
}

pub async fn set_header_filter(plugin: PluginHttpFilter) -> Result<()> {
    if HEADER_FILTER_NEXT.is_none().await {
        HEADER_FILTER_NEXT.set(plugin).await;
    }
    Ok(())
}

fn get_if_unmodified_since(headers: &HeaderMap) -> Option<SystemTime> {
    if let Some(value) = headers.get(IF_UNMODIFIED_SINCE) {
        if let Ok(value_str) = value.to_str() {
            // 解析时间字符串
            if let Ok(timestamp) = chrono::DateTime::parse_from_rfc2822(value_str) {
                return Some(timestamp.into());
            }
        }
    }
    None
}

fn parse_if_unmodified_since(r: &HttpStreamRequest) -> Result<()> {
    let r_ctx = &mut *r.ctx.get_mut();
    let response_info = r_ctx.r_out.response_info.as_ref().unwrap();
    let if_unmodified_since = get_if_unmodified_since(&r_ctx.r_in.headers);
    if if_unmodified_since.is_some() {
        let if_unmodified_since = if_unmodified_since.unwrap();
        let if_unmodified_since = if_unmodified_since
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if response_info.last_modified_time <= 0
            || if_unmodified_since < response_info.last_modified_time
        {
            r_ctx.r_out.status = StatusCode::PRECONDITION_FAILED;
            return Err(anyhow!(""));
        }
    }
    return Ok(());
}

fn get_etag(headers: &HeaderMap, key: &HeaderName, is_weak: bool) -> Option<Vec<String>> {
    if let Some(value) = headers.get(key) {
        if let Ok(value_str) = value.to_str() {
            let value_str = if is_weak
                && value_str.len() > 2
                && &value_str[0..1] == "W"
                && &value_str[1..2] == "/"
            {
                &value_str[2..]
            } else {
                value_str
            };
            // 分割字符串并去掉引号
            let etags: Vec<String> = value_str
                .split(',')
                .map(|s| {
                    let value_str = s.trim_matches('"').trim();
                    if is_weak
                        && value_str.len() > 2
                        && &value_str[0..1] == "W"
                        && &value_str[1..2] == "/"
                    {
                        value_str[2..].to_string()
                    } else {
                        value_str.to_string()
                    }
                })
                .collect();
            return Some(etags);
        }
    }
    None
}

fn parse_if_match(r: &HttpStreamRequest) -> Result<()> {
    let r_ctx = &mut *r.ctx.get_mut();
    let response_info = r_ctx.r_out.response_info.as_ref().unwrap();
    let if_match = get_etag(&r_ctx.r_in.headers, &IF_MATCH, false);
    if if_match.is_some() {
        let if_match = if_match.unwrap();
        if if_match.len() == 1 && if_match[0] == "*" {
            return Ok(());
        }
        if response_info.e_tag.is_empty() {
            r_ctx.r_out.status = StatusCode::PRECONDITION_FAILED;
            return Err(anyhow!(""));
        }

        for v in &if_match {
            if v == &response_info.e_tag {
                return Ok(());
            }
        }
        r_ctx.r_out.status = StatusCode::PRECONDITION_FAILED;
        return Err(anyhow!(""));
    }
    return Ok(());
}

fn get_if_modified_since(headers: &HeaderMap) -> Option<SystemTime> {
    if let Some(value) = headers.get(IF_MODIFIED_SINCE) {
        if let Ok(value_str) = value.to_str() {
            if let Ok(datetime) = httpdate::parse_http_date(value_str) {
                return Some(datetime);
            }
        }
    }
    None
}

fn parse_if_modified_since(r: &HttpStreamRequest) -> Option<bool> {
    let r_ctx = r.ctx.get_mut();
    let response_info = r_ctx.r_out.response_info.as_ref().unwrap();
    let if_modified_since = get_if_modified_since(&r_ctx.r_in.headers);
    if if_modified_since.is_some() {
        let if_modified_since = if_modified_since.unwrap();
        let if_modified_since = if_modified_since
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_secs();
        if if_modified_since >= response_info.last_modified_time {
            return Some(false);
        }
        return Some(true);
    }
    return None;
}

fn parse_if_none_match(r: &HttpStreamRequest) -> Option<bool> {
    let r_ctx = r.ctx.get_mut();
    let response_info = r_ctx.r_out.response_info.as_ref().unwrap();
    let if_none_match = get_etag(&r_ctx.r_in.headers, &IF_NONE_MATCH, true);
    if if_none_match.is_some() {
        let if_none_match = if_none_match.unwrap();
        if if_none_match.len() == 1 && if_none_match[0] == "*" {
            return Some(true);
        }
        if response_info.e_tag.is_empty() {
            return Some(false);
        }

        for v in &if_none_match {
            if v == &response_info.e_tag {
                return Some(true);
            }
        }
        return Some(false);
    }
    return None;
}

pub async fn http_filter_header_not_modified(r: Arc<HttpStreamRequest>) -> Result<()> {
    log::trace!(target: "main",
        "r.session_id:{}, http_filter_header_not_modified",
        r.session_id
    );
    let _ = do_http_filter_header_not_modified(&r).await;
    let next = HEADER_FILTER_NEXT.get().await;
    if next.is_some() {
        let _ = (next)(r).await;
    }
    return Ok(());
}

pub async fn do_http_filter_header_not_modified(r: &HttpStreamRequest) -> Result<()> {
    parse_if_unmodified_since(&r)?;
    parse_if_match(&r)?;

    let if_modified_since = parse_if_modified_since(&r);
    let if_none_match = parse_if_none_match(&r);

    let mut r_ctx = r.ctx.get_mut();
    if if_modified_since.is_some() || if_none_match.is_some() {
        if if_modified_since.is_some() && if_modified_since.unwrap() == true {
            return Ok(());
        }
        if if_none_match.is_some() && if_none_match.unwrap() == false {
            return Ok(());
        }

        r_ctx.r_out.status = StatusCode::NOT_MODIFIED;
        return Err(anyhow!(""));
    }
    return Ok(());
}
