use crate::proxy::http_proxy::http_context::HttpContext;
use std::rc::Rc;

pub struct DomainContext {
    pub http_context: Rc<HttpContext>,
}

impl DomainContext {
    pub fn new() -> DomainContext {
        DomainContext {
            http_context: Rc::new(HttpContext::new()),
        }
    }
}
