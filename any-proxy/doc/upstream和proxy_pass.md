#upstream
````
[[upstream]]
    name = "upstream_config_2"
    dispatch = {type="weight"}
    server = [
        #tcp回源
        {type = "tcp", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
        #quic回源
        {type = "quic", ssl_domain= "www.yefyyun.cn", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
        #tcp tunnel回源
        {type = "tunnel", tunnel = "tcp", tunnel_max_connect = 8, address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
        #quic tunnel回源
        {type = "tunnel", tunnel = "quic", tunnel_max_connect = 8, ssl_domain= "www.yefyyun.cn", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
        #tcp tunnel2回源
        {type = "tunnel2", tunnel = "tcp", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
        #quic tunnel2回源        
        {type = "tunnel2", tunnel = "quic", ssl_domain= "www.yefyyun.cn", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
    ]
````
#proxy_pass
````
[[_server]]
    proxy_pass = {type = "tcp", address = "www.upstream.cn:11131"}#, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}}
[[_server]]
    proxy_pass = {type = "upstream", ups_name="upstream_config_1"},

upstream中的server配置中每一条就是proxy_pass
每个配置项都包含, weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}

weight = 10 在dispatch = {type="weight"} 生效  
is_proxy_protocol_hello = true 回源添加hello  
heartbeat = {  interval = 10, timeout = 10, fail = 3} 需要开启心跳检查  
dynamic_domain = {interval = 10} 动态域名解析  
````

#tcp回源
{type = "tcp", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
#quic回源
{type = "quic", ssl_domain= "www.yefyyun.cn", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
#tcp tunnel回源
{type = "tunnel", tunnel = "tcp", tunnel_max_connect = 8, address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
tunnel_max_connect = 8 表示最大使用8个tcp链接加速
#quic tunnel回源
{type = "tunnel", tunnel = "quic", tunnel_max_connect = 8, ssl_domain= "www.yefyyun.cn", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
tunnel_max_connect = 8 表示最大使用8个quic链接加速
#tcp tunnel2回源
{type = "tunnel2", tunnel = "tcp", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
#quic tunnel2回源        
{type = "tunnel2", tunnel = "quic", ssl_domain= "www.yefyyun.cn", address = "www.upstream.cn:11131", weight = 10, is_proxy_protocol_hello = true, heartbeat = {  interval = 10, timeout = 10, fail = 3}, dynamic_domain = {interval = 10}},
#upstream回源，只能在proxy_pass中配置
proxy_pass = {type = "upstream", ups_name="upstream_config_1"},
