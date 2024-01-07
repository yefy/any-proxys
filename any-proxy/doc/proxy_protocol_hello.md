#is_proxy_protocol_hello
如果开启is_proxy_protocol_hello会在发送代理数据前将下面数据先带过去  

版本号  
pub version: String  
请求id，整个链路是唯一的，可以根据这个值做链路日志，定位流的去向  
pub request_id: String  
实际客户端地址  
pub client_addr: SocketAddr  
当前的域名，有这个值就不用在取解析sni了  
pub domain: String, 

#注意
is_proxy_protocol_hello只能在内部使用，外部服务无法解析会导致数据异常