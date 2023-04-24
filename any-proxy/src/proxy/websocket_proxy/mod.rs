pub mod websocket_server;

use lazy_static::lazy_static;

lazy_static! {
    pub static ref WEBSOCKET_HELLO_KEY: String = "websocket_hello".to_string();
}
