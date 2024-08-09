# 配置格式
```
net {
   #层级：main|server|local
    #access_log 日志，内置toml格式扩展
    access raw = r```
        #如果没有配置就继承父类的配置
        [[access]]
            #是否打开 #default true
            access_log = true
            #文件路径    default "./logs/access.log"
            access_log_file = "./logs/access.log"
            #支持预制变量
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
                http_max_upstream_count:${http_max_upstream_count} http_is_cache:${http_is_cache} \
                http_request_url:${http_request_method}@${http_request_url} http_cache_file_path:${http_cache_file_path}"""

            #是否打印到屏幕，调试用， default false
            access_log_stdout = false
            #是否只是打印错误日志， default true
            is_err_access_log = true
    ```r;
}
```