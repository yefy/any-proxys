use super::http_hyper_connector::HttpHyperConnector;
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;

pub struct HttpContext {
    //protocol addr http_version
    pub client_map: RefCell<HashMap<String, Rc<hyper::Client<HttpHyperConnector>>>>,
}

impl HttpContext {
    pub fn new() -> HttpContext {
        HttpContext {
            client_map: RefCell::new(HashMap::new()),
        }
    }
}
