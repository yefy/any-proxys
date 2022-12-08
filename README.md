# anyproxy
rust编写高性能四层代理服务器，支持tcp、quic、any-tunnel、any-tunnel2、ebpf 

# 特点
底层tokio框架  
多线程无锁并发  
高性能  
内存安全  
tcp、quic、any-tunnel、any-tunnel2协议互转  
热加载配置  
支持any-tunnel、any-tunnel2在高延迟网络加速  
支持access_log  
支持ebpf导流  

# 已经支持
纯端口代理模式  
域名代理模式  
tcp代理  
quic代理  
tcp ssl代理  
any-tunnel代理加速   
anu-tunnel2代理加速  
proxy_protocol协议  
tcp, tcp ssl, quic, any-tunnel, any-tunnel2互转  
access log和变量  
reload配置热加载  
reinit配置热加载  
流量统计  
支持泛域名  
支持配置范围端口监听   
临时文件缓存  
限流  
proxy_pass回源  
upstream 配置多主机回源  
支持心跳检查  
支持动态域名解析  
支持ebpf导流  

# 未来支持  
http 域名获取  
http2 域名获取 
支持多跳回源 

# wiki
[anyproxy编译运行](https://github.com/yefy/any-proxy/wiki/anyproxy%E7%BC%96%E8%AF%91%E8%BF%90%E8%A1%8C)  
[nginx源站支持http和https](https://github.com/yefy/any-proxy/wiki/nginx%E6%BA%90%E7%AB%99%E6%94%AF%E6%8C%81http%E5%92%8Chttps)  
[port与domain模式配置](https://github.com/yefy/any-proxy/wiki/port%E4%B8%8Edomain%E6%A8%A1%E5%BC%8F%E9%85%8D%E7%BD%AE)  
[access_log支持](https://github.com/yefy/any-proxy/wiki/access_log%E6%94%AF%E6%8C%81)  