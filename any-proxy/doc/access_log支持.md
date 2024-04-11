# 配置格式
```
net {
    access raw = r```
        #如果没有配置就继承父类的配置
        [[access]]
            #default true
            access_log = false
            #default "./logs/access.log"
            access_log_file = "./logs/access.log"
            #default
            access_format = """[${local_time}] stream_max_write_time:${write_max_block_time_ms} buffer_cache:${buffer_cache} \
                upstream_balancer:${upstream_balancer} hello:${is_proxy_protocol_hello} ebpf:${is_open_ebpf} \
                sendfile:${open_sendfile} ${local_protocol} -> ${upstream_protocol} \
                request_id:[${request_id}] client_addr:${client_addr} remote_addr:${remote_addr} local_addr:${local_addr} upstream_addr:${upstream_addr} \
                domain:${domain} upstream_host:${upstream_host} ${status} ${status_str} timeout_exit:${is_timeout_exit} \
                session_time:${session_time} upstream_connect_time:${upstream_connect_time} \
                stream_bytes:${client_bytes_received} ${upstream_bytes_sent} ${upstream_bytes_received} ${client_bytes_sent} \
                ${upstream_curr_stream_size} ${upstream_max_stream_size} ${upstream_min_stream_cache_size} \
                client_protocol_hello_size:${client_protocol_hello_size} upstream_protocol_hello_size:${upstream_protocol_hello_size} \
                stream_work_times:[${stream_work_times}] stream_stream_info:[${stream_stream_info}] \
                http_local_cache_req_count:${http_local_cache_req_count} http_cache_status:${http_cache_status} \
                http_cache_file_status:${http_cache_file_status} http_is_upstream:${http_is_upstream} \
                http_last_slice_upstream_index:${http_last_slice_upstream_index} \
                http_max_upstream_count:${http_max_upstream_count} http_is_cache:${http_is_cache}"""
            
            #default false
            access_log_stdout = false
    ```r;
}
```

# 变量在配置中使用${var}
# access_log输出时间
"local_time"
# ssl协议域名
"ssl_domain"
# port模式是本地配置文件中的domain， domain模式是ssl协议域名或is_proxy_protocol_hello中的域名
"local_domain"
# ssl协议域名或is_proxy_protocol_hello中的域名
"remote_domain"
# 监听协议（tcp、quic等）
"local_protocol"
# 回源协议（tcp、quic等）
"upstream_protocol"
# 监听地址 ip:port
"local_addr"
# 监听ip
"local_ip"
# 监听端口
"local_port"
# 链接到监听地址的远端 ip:port
"remote_addr"
# 链接到监听地址的远端 ip
"remote_ip"
# 链接到监听地址的远端 port
"remote_port"
# 全部请求结束时间
"session_time"
# 这个链路唯一id，可以用于排查链接经过哪些服务器
"request_id"
# anyproxy版本
"versions"
# 实际客户端ip:port
"client_addr"
# 实际客户端ip
"client_ip"
# 实际客户端port
"client_port"
# is_proxy_protocol_hello中的域名
"domain"
# 状态码
"status"
# 状态码对应的信息
"status_str"
# 回源配置中的域名或ip
"upstream_host"
# 回源dns解析到的ip:port
"upstream_addr"
# 回源ip
"upstream_ip"
# 回源port
"upstream_port"
# 回源connect使用时间
"upstream_connect_time"
# 打印链接个个阶段使用时间，用于排查问题和性能
"stream_work_times"
# 发送到客户端的字节
"client_bytes_sent"
# 接受到客户端的字节
"client_bytes_received"
# 发送到源站的字节
"upstream_bytes_sent"
# 接受到源站的字节
"upstream_bytes_received"
# 是否使用ebof
"is_open_ebpf"
# 是否使用sendfile
"open_sendfile"
# 回源负载均衡方法
"upstream_balancer"
# 是否开启proxy_protocol_hello
"is_proxy_protocol_hello"
# 使用缓存和临时文件
"buffer_cache"
