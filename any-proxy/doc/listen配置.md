#port模式
````
server {
    stream_cache_size  usize = 131072;
    domain str = "www.example.cn";
    port_listen_tcp raw = r```
        address = "0.0.0.0:10001"
    ```r;
    port_listen_ssl raw = r```
        address = "0.0.0.0:10011"
        ssl = {key = "./cert/www.example.cn.key.pem", cert = "./cert/www.example.cn.cert.pem", ssl_domain = "www.example.cn"}
    ```r;
    port_listen_quic raw = r```
        address = "0.0.0.0:10011"
        ssl = {key = "./cert/www.example.cn.key.pem", cert = "./cert/www.example.cn.cert.pem", ssl_domain = "www.example.cn"}
    ```r;
    proxy_pass_upstream str = "upstream_tcp_http";
    local {
    }
}
````

#domain模式
````
server {
    domain str = "www.example.cn";
     domain_listen_tcp raw = r```
        address = "0.0.0.0:10101"
    ```r;
    domain_listen_ssl raw = r```
        address = "0.0.0.0:10111"
        ssl = {key = "./cert/www.example.cn.key.pem", cert = "./cert/www.example.cn.cert.pem"}
    ```r;
    domain_listen_quic raw = r```
        address = "0.0.0.0:10111"
        ssl = {key = "./cert/www.example.cn.key.pem", cert = "./cert/www.example.cn.cert.pem"}
    ```r;
    proxy_pass_upstream str = "upstream_tcp_https";
    local {
    }
}
````

#启动监听
````
port模式：配置中的domain可以为空， 索引需要多配置 ssl_domain = "www.example.cn" 启动时设置域名和证书， 提供给quic协议获取域名后获取证书
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