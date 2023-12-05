#upstream
````
upstream {
    server {
        #配置名字，可以根据名字来引用配置
        name str = upstream1;
        #负载均衡 weight round_robin random ip_hash ip_hash_active fair
        balancer str = random;
        proxy_pass_tcp raw = r```
            address = "www.upstream.cn:10001"
            is_proxy_protocol_hello = true
        ```r;
        proxy_pass_ssl raw = r```
            ssl_domain= "www.example.cn"
            address = "www.upstream.cn:10011"
            is_proxy_protocol_hello = true
        ```r;
        proxy_pass_quic raw = r```
            ssl_domain= "www.example.cn"
            address = "www.upstream.cn:10011"
            is_proxy_protocol_hello = true
        ```r;
        proxy_pass_tunnel_tcp raw = r```
            tunnel = { max_stream_size = 5, min_stream_cache_size = 50, channel_size = 64}
            address = "www.upstream.cn:10001"
            #heartbeat = {interval = 10, timeout = 10, fail = 3}
            #dynamic_domain = {interval = 10}
            is_proxy_protocol_hello = true
        ```r;
        proxy_pass_tunnel_ssl raw = r```
            tunnel = { max_stream_size = 5, min_stream_cache_size = 50, channel_size = 64}
            ssl_domain= "www.example.cn"
            address = "www.upstream.cn:10011"
            #heartbeat = {interval = 10, timeout = 10, fail = 3}
            #dynamic_domain = {interval = 10}
            is_proxy_protocol_hello = true
        ```r;
        proxy_pass_tunnel_quic raw = r```
            tunnel = { max_stream_size = 5, min_stream_cache_size = 50, channel_size = 64}
            ssl_domain= "www.example.cn"
            address = "www.upstream.cn:10011"
            #heartbeat = {interval = 10, timeout = 10, fail = 3}
            #dynamic_domain = {interval = 10}
            is_proxy_protocol_hello = true
        ```r;
    }
}
````
#proxy_pass
````
proxy_pass_tcp raw = r```
            address = "www.upstream.cn:10001"
            is_proxy_protocol_hello = true
```r;

upstream中的server配置中每一条就是proxy_pass
每个配置项都包含, weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}

weight = 10 在dispatch = {type="weight"} 生效  
is_proxy_protocol_hello = true 回源添加hello  
heartbeat = {  interval = 10, timeout = 10, fail = 3} 需要开启心跳检查  
dynamic_domain = {interval = 10} 动态域名解析  
````