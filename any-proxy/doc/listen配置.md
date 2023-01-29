#port模式
````
[[_server]]
    domain = "www.yefyyun.cn"
    [[listen]]
        type = "tcp"
        address = "0.0.0.0:[11135~11135]"
    [[listen]]
        type = "quic"
        address = "0.0.0.0:[11135~11135]"
        ssl = {key = "./cert/www.yefyyun.cn.key", cert = "./cert/www.yefyyun.cn.pem", ssl_domain = "www.yefyyun.cn"}

#domain模式
[[_server]]
    domain = "www.yefyyun.cn $$(...).i4.cn"
    [[listen]]
        type = "tcp"
        address = "0.0.0.0:[11152~11152]"
    [[listen]]
        type = "quic"
        address = "0.0.0.0:[11152~11152]"
        ssl = {key = "./cert/www.yefyyun.cn.key", cert = "./cert/www.yefyyun.cn.pem"}
````

#启动监听
````
port模式：配置中的domain可以为空， 索引需要多配置 ssl_domain = "www.yefyyun.cn" 启动时设置域名和证书， 提供给quic协议获取域名后获取证书
domain模式：配置中domain支持完整域名和泛域名， 启动时设置域名和证书， 提供给quic协议或ssl获取域名后获取证书

protocol hello 在客户端链接后才可能有这个域名无法使用
````

#客户端访问后配置匹配
````
port模式是端口代理，根据端口获取配置
    hello_domain: 本地配置 > hello_domain
    ssl_domain:   quic协议或空
    local_domain:  本地配置 > hello_domain
   remote_domain: hello_domain > quic协议或空


domain模式是ssl代理，会根据sni获取域名，并根据域名获取到指定配置，其中quic协议代理ssl协议的情况下，存在quic域名和ssl域名
    hello_domain: hello_domain > ssl_domain > quic_domain
    ssl_domain:   quic协议 > ssl > hello_domain
    local_domain:  hello_domain > ssl > quic协议或空
    remote_domain: hello_domain > ssl > quic协议或空
````