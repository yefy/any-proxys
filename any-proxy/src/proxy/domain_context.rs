use crate::proxy::http_proxy::http_context::HttpContext;
use std::sync::Arc;

pub struct DomainContext {
    pub http_context: Arc<HttpContext>,
}

impl DomainContext {
    pub fn new() -> DomainContext {
        DomainContext {
            http_context: Arc::new(HttpContext::new()),
        }
    }
}
