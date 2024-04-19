#配置文件格式
```
#层级：main|server|local
#wasm_access 阶段添加webassembly热更新脚本，内置toml格式扩展
wasm_access raw = r```
    [[wasm]]
    #webassembly脚本路径
    wasm_path = "../../../wasm/http-demo/target/wasm32-wasi/release/wasm_server.wasm"
    #webassembly main接口配置，由webassembly自己解析可以是toml， json， yaml等格式
    wasm_main_config = '''
        name = "wasm_access_test"
        '''
    #webassembly是否打开， 默认打开
    is_open = true
    #webassembly main_timeout接口配置，由webassembly自己解析可以是toml， json， yaml等格式, 默认不配置关闭
    wasm_main_timeout_config = ""
    #webassembly main_ext1接口配置，由webassembly自己解析可以是toml， json， yaml等格式, 默认不配置关闭
    wasm_main_ext1_config = ""
    #webassembly main_ext2接口配置，由webassembly自己解析可以是toml， json， yaml等格式, 默认不配置关闭
    wasm_main_ext2_config = ""
    #webassembly main_ext3接口配置，由webassembly自己解析可以是toml， json， yaml等格式, 默认不配置关闭
    wasm_main_ext3_config = ""
```r;
```

#返回值
```
每个插件可以配置多个脚本， 可以配置多个插件
enum error {
    //继续进行
    ok,
    //结束当前循环
    break,
    //结束所有循环
    finish,
    //错误退出请求
    error,
    //退出请求
    return,
    ext1,
    ext2,
    ext3,
}
```

#六个插件阶段
```
wasm_access： 支持所有返回值
wasm_serverless： 支持所有返回值，但是插件结束一定返回return，程序结束
wasm_access_log： 不支持error，return
wasm_http_in_headers： 不支持error，return
wasm_http_filter_headers_pre： 不支持error，return
wasm_http_filter_headers： 不支持error，return

wasm_access阶段可以开发防盗链校验、ip限制、防攻击开发
wasm_serverless阶段可以解析协议做复杂业务开发，并且是无锁并发的
wasm_http_in_headers阶段可以修改请求的原始http数据
wasm_http_filter_headers_pre阶段可以修改响应的原始http数据
wasm_http_filter_headers阶段可以修改响应给客户端http数据
wasm_access_log阶段可以获取全部预备变量，可以开发流量采集
```

#内置异步接口
##/wasm/wit/wasm_server.wit
```
参考/wasm/http-demo项目
/wasm/http-demo/src/component.rs
内置异步接口：
    wasm_main：参数是配置文件中的wasm_main_config
    wasm_main_timeout：参数是配置文件中的wasm_main_timeout_config， 如果没有配置不开启
    wasm_main_ext1：参数是配置文件中的wasm_main_ext1_config， 如果没有配置不开启
    wasm_main_ext2：参数是配置文件中的wasm_main_ext2_config， 如果没有配置不开启
    wasm_main_ext3：参数是配置文件中的wasm_main_ext3_config， 如果没有配置不开启
上面异步函数是不同的插件运行的，虽然代码在一个项目，但是不能共享全局变量，它们共享的是主服务器链接信息
```

#异步标准库
##/wasm/wit/wasm_std.wit
```
    #获取版本信息
    anyproxy-version: func() -> result<string, string>;
    
    variable: func(key: string) -> result<option<string>, string>;
    sleep: func(time-ms: u64);
    curr-session-id: func() -> u64;
    curr-fd: func() -> u64;
    new-session-id: func() -> u64;
    session-send: func(session-id: u64, value: string) -> result<_, string>;
    session-recv: func() -> result<string, string>;
    add-timer: func(time-ms: u64, key: u64, value: string);
    del-timer: func(key: u64);
    get-timer-timeout:func(time-ms: u64) -> list<string>;
```
##/wasm/wit/wasm_http.wit
```
参考 /wasm对应项目代码
```
##/wasm/wit/wasm_log.wit
##/wasm/wit/wasm_socket.wit
##/wasm/wit/wasm_store.wit
##/wasm/wit/wasm_websocket.wit






