# anyproxy介绍
```
采用异步rust语言编写高性能、内存安全、高度模块化、插件化、配置化、脚本化、跨平台多线程无锁并发协程框架，
支持四层反向代理服务：包括TCP、QUIC、SSL、any-tunnel协议互转加速，内核态ebpf加速，
支持七层反向代理服务：包括协议（HTTP、HTTP2、WebSocket）over（TCP、QUIC、SSL、any-tunnel），
支持http文件缓存代理服务、负载均衡和故障转移策略，优雅的热加载配置文件，
无锁异步webassembly热更新脚本框架，支持rust、c/c++、JavaScript、Python、Java、Go做为脚本语言进行开发
适用于构建高效、脚本热更新、灵活的网络代理和加速服务。
```
# 特点
```
内存安全的跨平台（Linux、Window、MacOS等）异步协程框架，性能媲美c/c++、开源nginx  
Linux reuseport多线程无锁并发  
没有reuseport平台支持多线程有锁并发  
支持动态添加模块，能快速添加新协议监听和回源（包括tcp、quic、ssl、srt、kcp、http、websocket等）   
无锁异步webassembly热更新脚本框架 
支持net、server、local三级配置，启动阶段配置文件由各自模块独立解析包括预解析、初始化、共享数据合并和继承 
可以快速基于tcp、quic、ssl、http、websocket添加插件和脚本开发业务  
支持tcp、quic、ssl、any-tunnel四层代理协议相互转换加速  
支持七层协议（http、https、http2、https2、websocket、websockets）over （TCP、QUIC、SSL、any-tunnel）
支持ebpf内核态四层代理，即能做到用户态的便利性解析，又能有内核态高性能    
优雅的配置文件热加载，linux reuseport环境支持程序热升级    
支持any-tunnel在高延迟网络加速，socket流缓存    
支持非常详细的access_log、err_log、跨服务器链路日志 
纯端口代理模式、域名代理模式（多域名端口复用）  
支持负载均衡和故障转移策略、心跳检查、动态域名解析  
支持linux零拷贝技术sendfile和文件directio高速缓存控制
支持预制变量和函数数据，可以由配置文件和webassembly脚本使用  
```
# 功能模块
## 四层反向代理服务
```
支持多种协议（包括TCP、QUIC、SSL、any-tunnel）互转加速
支持ebpf内核态四层代理，即能做到用户态的便利性解析，又能有内核态高性能
支持proxy_protocol_hello协议头，可以携带更多信息给上游服务
支持临时文件缓存、流量限制
支持配置多主机回源并支持负载均衡、心跳检查、动态域名解析  
支持port端口代理模式
支持domain域名代理模式，多域名端口复用，相同端口可以配置多个域名（域名解析ssl的sni、http协议等获取）
```
## 七层反向代理服务
```
支持（http、https、http2、https2) over （TCP、QUIC、SSL、any-tunnel）
支持（websocket、websockets) over （TCP、QUIC、SSL、any-tunnel）
支持proxy_protocol_hello协议头，可以携带更多信息给上游服务
支持临时文件缓存、流量限制
支持配置多主机回源并支持负载均衡、心跳检查、动态域名解析  
```
## http静态文件服务（目前处于测试和完善阶段）
```
支持（http、https、http2、https2) over （TCP、QUIC、SSL、any-tunnel）
支持异步文件io
支持linux零拷贝技术sendfile和文件directio高速缓存控制
支持range请求
支持流量限制
支持301状态码
支持last_modified if-match 412状态码
支持if-modified-since if-none-match 304状态码
```
## http文件缓存代理服务（目前处于测试和完善阶段）
```
支持（http、https、http2、https2) over （TCP、QUIC、SSL、any-tunnel）
支持proxy_protocol_hello协议头，可以携带更多信息给上游服务
支持流量限制
支持非缓存情况下开启临时文件缓存
支持配置多主机回源并支持负载均衡、心跳检查、动态域名解析  
支持文件缓存、过期文件校验、过期淘汰、缓存文件磁盘大小统计并启用过载淘汰、启动阶段预加载缓存文件
支持并发请求合并回源
支持range请求
支持配置切片大小回源加快响应速度
支持配置多个存储路径
支持热点文件负载存储，防止单磁盘io瓶颈
支持linux零拷贝技术sendfile和文件directio高速缓存控制
支持异步文件io
支持文件句柄缓存
支持301状态码
支持last_modified if-match 412状态码
支持if-modified-since if-none-match 304状态码
支持method类型缓存
支持错误码缓存
```
## 无锁异步webassembly热更新脚本框架
```
支持无锁异步开发
支持rust、c/c++、JavaScript、Python、Java、Go做为脚本语言进行开发
支持reload优雅热升级，无需重启进程
支持wasm_http异步http over （tcp quic ssl）网络库
支持wasm_log打印日志
支持wasm_store进程内全局共享存储
支持wasm_socket异步tcp，quic，ssl网络库
支持wasm_std库与主服务交互，包括预制的变量读取、http数据修改
支持wasm_websocket异步websocket over （tcp quic ssl）网络库
支持WebAssembly脚本之间进行异步channel通讯
支持异步定时器
主程序支持多阶段嵌入webassembly脚本开发：
    四层协议TCP、QUIC、SSL、any-tunnel有三个阶段：
        wasm_access阶段可以开发防盗链校验、ip限制、防攻击开发
        wasm_serverless阶段可以解析协议做复杂业务开发，并且是无锁异步并发
        wasm_access_log阶段可以获取全部预备变量，可以开发流量采集
    http协议有六个阶段：
        wasm_access阶段可以开发防盗链校验、ip限制、防攻击开发
        wasm_serverless阶段可以解析协议做复杂业务开发，并且是无锁并发的
        wasm_http_in_headers阶段可以修改请求的原始http数据
        wasm_http_filter_headers_pre阶段可以修改响应的原始http数据
        wasm_http_filter_headers阶段可以修改响应给客户端http数据
        wasm_access_log阶段可以获取全部预备变量，可以开发流量采集
    websocket协议有四个阶段：
        wasm_access阶段可以开发防盗链校验、ip限制、防攻击开发
        wasm_serverless阶段可以解析协议做复杂业务开发，并且是无锁异步并发
        wasm_http_in_headers阶段可以修改请求的原始http数据
        wasm_access_log阶段可以获取全部预备变量，可以开发流量采集
```
## 负载均衡算法
```
支持加权轮询、轮询、随机、固定哈希、动态哈希、Fair 等算法
```
## any-tunnel协议
```
支持高延迟网络加速，socket流缓存的协议    
支持any-tunnel over （TCP、QUIC、SSL）
```
# anyproxy doc
[文档](https://github.com/yefy/any-proxys/tree/main/any-proxy/doc)
# any-tunnel doc
[文档](https://github.com/yefy/any-proxys/blob/main/any-tunnel/README.md)
# 详细技术实现
```
支持socket 高性能writev函数
domain代理支持http_v1协议解析获取域名  
http文件缓存代理服务  
无锁异步webassembly热更新脚本框架  
支持预制变量和函数数据，可以由配置文件和webassembly脚本使用
用户态的stream delay，可以减少内存复制和系统调用    
hyper库支持linux零拷贝技术sendfile文件发送  
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
代理协议有tcp、ssl、quic、any-tunnel、http、https、http2.0、https2.0、websocket、websockets
支持tcp、quic、ssl、any-tunnel四层代理协议相互转换加速  
支持ebpf内核态四层代理，即能做到用户态的便利性解析，又能有内核态高性能   
proxy_protocol_hello协议头    
支持非常详细的access_log、err_log、跨服务器链路日志、详细统计信息   
reload配置文件热加载、超时断流   
reinit重新创建线程、配置文件热加载、超时断流    
流量统计  
支持泛域名  
支持配置范围端口监听   
quic支持绑定端口  
上传下载临时文件缓存、限流  
proxy_pass和upstream 配置多主机回源并支持负载均衡  
负载均衡算法--加权轮询,轮询,随机,固定hash, 动态hash,fair   
支持心跳检查  
支持动态域名解析  
linux零拷贝技术sendfile和文件directio高速缓存控制
多线程cpu绑定  
linux reuseport绑定端口  
配置文件支持include、跨平台标识、块注释等    
命令:anyproxy -s quit 正常关闭，可设置超时时间  
命令:anyproxy -s stop 快速关闭  
命令:anyproxy -s reload 配置文件热加载  
命令:anyproxy -s reinit 重新创建线程、配置文件热加载、超时断流    
命令:anyproxy -t 配置文件正确性检查  
命令:anyproxy -c xxx.conf 指定配置文件路径  
命令:anyproxy --hot xxx.pid 指定pid文件路径, 并进行热升级、正常关闭旧进程  
sni域名解析  
异步文件io  
内存管理  
用户态stream delay 合并发送
支持文件io page_size对齐  
支持openssl ssl和rustls ssl  
支持any-tunnel包括socket多链接加速，socket流缓存，解决tcp三次握手， ssl多次握手等问题  
支持net、server、local三级配置，启动阶段配置文件由各自模块独立解析包括预解析、初始化、共享数据合并和继承 
支持tcp、quic协议配置设置  
支持配置主线程、线程池个数  
```
# 未来支持
```
srt、kcp协议  
支持多跳回源  
```