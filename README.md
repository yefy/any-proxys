# anyproxy
rust语言编写高性能、高度模块化、插件化的负载平衡器、四层和七层反向代理服务，支持tcp、quic、ssl、any-tunnel、http、https、http2.0、https2.0、websocket、websockets、内核态ebpf等

# anyproxy doc
[文档](https://github.com/yefy/any-proxys/tree/main/any-proxy/doc)

# any-tunnel doc
[文档](https://github.com/yefy/any-proxys/blob/main/any-tunnel/README.md)

# 特点
跨平台（Linux、Window、Mac等）协程框架编码简单  
Linux reuseport多线程无锁并发  
没有reuseport情况支持多线程有锁并发  
类似nginx高度模块化和插件化框架，能快速添加新协议监听和回源（包括tcp、quic、ssl、srt、kcp、http、websocket等）   
支持动态添加模块，配置文件由各自模块独立解析包括预解析、初始化、共享数据合并和继承、配置内容合并和继承(代码参考any-proxys/any-proxy/src/config)        
可以快速基于tcp、quic、ssl、http、websocket添加插件解析私有协议  
支持ebpf内核态四层代理，即能做到用户态的便利性解析，又能有内核态高性能    
高性能，cpu占用低（和nginx相媲美的性能）    
内存安全没有泄漏风险，且内存占用低，无gc  
支持tcp、quic、ssl、any-tunnel四层代理协议相互转换  
功能非常强大配置文件解析，见[文档](https://github.com/yefy/any-proxys/blob/main/any-proxy/doc/%E9%85%8D%E7%BD%AE%E6%96%87%E4%BB%B6%E7%BB%93%E6%9E%84.md)   
配置文件热加载  
linux reuseport环境支持程序热升级    
支持any-tunnel在高延迟网络加速，socket流缓存    
支持非常详细的access_log、err_log    
纯端口代理模式  
域名代理模式  
详细统计信息    
跨服务器链路日志  
支持各种网络环境加速  
支持openssl ssl和rustls ssl  
支持负载均衡、心跳检查、动态域名解析  
支持linux零拷贝技术sendfile，在文件下载中cpu大幅度降低    

# 已经支持
hyper库支持linux零拷贝技术sendfile  
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
websockets代理  
支持ebpf内核态四层代理，即能做到用户态的便利性解析，又能有内核态高性能     
any-tunnel代理加速  
proxy_protocol_hello协议头   
支持tcp、quic、ssl、any-tunnel四层代理协议相互转换   
access_log和变量，包括各种信息统计    
reload配置文件热加载
reinit重新创建线程、配置文件热加载、超时断流    
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
linux零拷贝技术sendfile  
cpu绑定  
linux reuseport  
配置文件支持include、跨平台标识、块注释等    
命令:anyproxy -s quit 正常关闭，可设置超时时间  
命令:anyproxy -s stop 快速关闭  
命令:anyproxy -s reload 配置文件热加载  
命令:anyproxy -s reinit 重新创建线程、配置文件热加载、超时断流    
命令:anyproxy -t 配置文件正确性检查  
命令:anyproxy -c xxx.conf 指定配置文件路径  
命令:anyproxy --hot xxx.pid 指定pid文件路径, 并进行热升级、正常关闭旧进程  
支持链路日志,定位流走向，容易排查问题  
sni域名解析  
异步文件io  
内存管理  
支持小数据包合并发送  
支持文件io page_size对齐  
支持openssl ssl和rustls ssl  
支持any-tunnel包括socket多链接加速，socket流缓存，解决tcp三次握手， ssl多次握手等问题

# 未来支持
是否可以使用WebAssembly插件化和热升级模块  
srt协议  
支持多跳回源  
http cache文件缓存  