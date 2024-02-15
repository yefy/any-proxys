#proxy_pass
````
参考这个配置
any-proxys/any-proxy/examples/anyproxy/conf/more_conf/anyproxy_simple_port.conf
配置里面使用proxy_pass_tcp走tcp协议回源，这种配置不支持负载均衡
````

#upstream
````
参考这个配置
any-proxys/any-proxy/examples/anyproxy/conf/more_conf/anyproxy_upstream.conf
使用proxy_pass_upstream str = "upstream_name"; 使用upstream_name引用upstream配置，可以支持负载均衡
````

#proxy_pass和upstream区别
````
proxy_pass是简易版upstream，不支持负载均衡，只能配置一个回源
upstream是多个proxy_pass组合，支持负载均衡，可以配置多个proxy_pass回源
````