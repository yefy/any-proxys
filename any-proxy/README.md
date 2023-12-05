# anyproxy
rust编写高性能、高度模块化和插件化的四层,七层代理服务器，支持tcp、quic、ssl、any-tunnel、http、https、http2.0、https2.0、websocket、websockets、ebpf等

# any-tunnel
[文档](https://github.com/yefy/any-proxys/blob/main/any-tunnel/README.md)

# 特点
底层tokio异步io框架  
多线程无锁并发  
多线程有锁并发  
类似nginx高度模块化和插件化框架，快速添加协议监听和代理回源（包括tcp、quic、ssl、srt、http、websocket等），
动态添加模块，配置文件由各自模块独立解析包括预解析、初始化、共享数据合并和继承、配置合并和继承(代码参考any-proxys/any-proxy/src/config)      
可以快速基于tcp、quic、ssl、http、websocket添加插件解析私有协议，处理自己业务  
高性能  
内存安全  
tcp、quic、ssl、any-tunnel协议互转  
热加载配置  
支持any-tunnel在高延迟网络加速  
支持access_log  
支持ebpf导流  
纯端口代理模式  
域名代理模式  
各种信息统计  
链路日志  
支持各种网络环境加速  
支持openssl ssl和rustls ssl

# 已经支持
any-tunnel server over (tcp、ssl、 quic)  
any-tunnel client over (tcp、ssl、 quic)

anyproxy server over (tcp、ssl、 quic、any-tunnel)  
anyproxy client over (tcp、ssl、 quic、any-tunnel)

anyproxy http server over (tcp、ssl、 quic、any-tunnel)  
anyproxy http client over (tcp、ssl、 quic、any-tunnel)

anyproxy websocket server over (tcp、ssl、 quic、any-tunnel)  
anyproxy websocket client over (tcp、ssl、 quic、any-tunnel)


纯端口代理模式  
域名代理模式  
tcp代理  
ssl代理  
quic代理  
http代理  
https代理  
http2.0代理  
https2.0代理  
websocket代理  
any-tunnel代理加速  
proxy_protocol_hello协议头   
tcp, ssl, quic, any-tunnel互转  
access log和变量，包括各种信息统计    
reload配置热加载和超时断流    
reinit重新创建线程和配置热加载  
流量统计  
支持泛域名  
支持配置范围端口监听   
quic支持绑定端口  
上传下载临时文件缓存  
上传下载限流  
proxy_pass回源  
upstream 配置多主机回源并支持负载均衡  
负载均衡算法--加权轮询,轮询,随机,固定hash, 动态hash,fair加载时间长短智能的进行负载均衡   
支持心跳检查  
支持动态域名解析  
支持ebpf导流  
linux零拷贝技术sendfile  
cpu绑定  
linux reuseport  
配置支持include  
命令:anyproxy -s quit 正常关闭，可设置超时时间  
命令:anyproxy -s stop 快速关闭  
命令:anyproxy -s reload 配置热加载  
命令:anyproxy -s reinit 重新分配线程，并配置热加载  
命令:anyproxy -t 配置正确性检查  
命令:anyproxy -c 指定配置文件路径  
支持链路日志,定位流走向，容易排查问题  
sni域名解析  
异步文件io  
内存管理  
支持小数据包合并发送  
支持文件io page_size对齐  
支持openssl ssl和rustls ssl  
支持any-tunnel包括socket多链接加速，socket流缓存，解决tcp三次握手， ssl多次握手等问题  

# 未来支持
是否可以使用WebAssembly插件化  
srt协议  
支持多跳回源  
http cache文件缓存

# doc
[文档](https://github.com/yefy/any-proxys/tree/main/any-proxy/doc)  