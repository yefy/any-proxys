use super::http_hyper_connector::HttpHyperConnector;
use any_base::typ::ShareRw;
use std::collections::HashMap;
use std::sync::Arc;

pub struct HttpContext {
    //protocol addr http_version
    pub client_map: ShareRw<HashMap<String, Arc<hyper::Client<HttpHyperConnector>>>>,
}

impl HttpContext {
    pub fn new() -> HttpContext {
        HttpContext {
            client_map: ShareRw::new(HashMap::new()),
        }
    }
}
