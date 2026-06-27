#proxy_pass
````
配置里面使用proxy_pass_tcp走tcp协议回源，这种配置不支持负载均衡
````
[参考anyproxy_simple_port.conf](../../any-example/all_conf/anyproxy_simple_port.conf)  

#upstream
````
使用proxy_pass_upstream str = "upstream_name"; 使用upstream_name引用upstream配置，可以支持负载均衡
````
[参考anyproxy_upstream.conf](../../any-example/all_conf/anyproxy_upstream.conf)  

#proxy_pass和upstream区别
````
proxy_pass是简易版upstream，不支持负载均衡，只能配置一个回源
upstream是多个proxy_pass组合，支持负载均衡，可以配置多个proxy_pass回源
````

#upstream支持简单配置
[参考anyproxy_edge_to_proxy_http_simple.conf](../../any-example/anyproxy/conf/anyproxy_edge_to_proxy_http_simple.conf)  
````
格式:
proxy_pass_upstream str = "//tcp//www.upstream.cn:29100//round_robin";
与下面配置一样的功能
proxy_pass_tcp raw = r```
    balancer = "round_robin"
    address = "www.upstream.cn:29100"
```r;
````