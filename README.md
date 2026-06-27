# anyproxy

**用 Rust 编写的高性能、可扩展网络代理与边缘加速平台。**

anyproxy 将 **四层反向代理**、**七层 HTTP/HTTPS 代理**、**HTTP 缓存存储**、**静态文件服务** 与 **WebAssembly 热更新脚本** 集成在同一套配置体系中。语法与 nginx 相近，运维习惯可平滑迁移；在协议互转、缓存能力、脚本扩展与配置热加载等方面针对现代边缘/CDN 场景做了深度优化。

> 适用场景：CDN 边缘节点、多协议网关、高延迟链路加速、带缓存的反向代理、可编程 WAF/防盗链、内网穿透与隧道加速等。

---

## 目录

- [核心能力](#核心能力)
- [为什么选择 anyproxy](#为什么选择-anyproxy)
- [架构概览](#架构概览)
- [快速开始（5 分钟验证）](#快速开始5-分钟验证)
- [进阶：三节点联调](#进阶三节点联调)
- [配置说明](#配置说明)
- [常用场景配置](#常用场景配置)
- [运维与生产部署](#运维与生产部署)
- [WebAssembly 扩展](#webassembly-扩展)
- [any-tunnel 加速协议](#any-tunnel-加速协议)
- [编译选项](#编译选项)
- [项目结构](#项目结构)
- [文档索引](#文档索引)
- [常见问题](#常见问题)

---

## 核心能力

| 类别 | 能力 |
|------|------|
| **四层代理** | TCP / SSL / QUIC / any-tunnel 互转；port 端口模式与 domain 多域名复用（SNI/Host） |
| **七层代理** | HTTP / HTTPS / HTTP2 / WebSocket over TCP、QUIC、SSL、any-tunnel |
| **HTTP 缓存** | 切片回源、Range、并发合并回源、purge、304 校验、多盘存储与热点迁移 |
| **静态服务** | sendfile / directio、条件请求（304/412）、rewrite |
| **回源** | upstream 多节点、6 种负载均衡、心跳检查、动态 DNS |
| **扩展** | WASM 多阶段插件（类 OpenResty lua 阶段），reload 热更新 |
| **运维** | 配置热加载、二进制热升级、全链路 request_id、详细 access_log |
| **Linux 增强** | eBPF 内核态加速、SO_REUSEPORT、零拷贝 sendfile |

**支持平台**：Linux（推荐生产环境）、Windows、macOS。

---

## 为什么选择 anyproxy

### 与 nginx / OpenResty 的定位差异

| 维度 | anyproxy | nginx / OpenResty |
|------|----------|-------------------|
| 实现语言 | Rust（内存安全） | C / LuaJIT |
| 四层 + 七层 + 缓存 | 统一配置体系 | 需 stream + http + cache 模块组合 |
| 协议互转 | TCP/QUIC/SSL/tunnel 原生互转 | 需额外模块或外部组件 |
| 脚本扩展 | WASM 多阶段，热 reload | Lua（OpenResty） |
| HTTP 缓存 | 内置高级缓存（切片、合并回源、purge, range不额外存储节省磁盘 等） | proxy_cache，能力相对基础 |
| 高延迟加速 | any-tunnel 隧道协议 | 无内置等价方案 |
| 链路追踪 | proxy_protocol_hello + request_id | 需 PROXY protocol 等额外配置 |
| 配置热加载 | reload 长连接可复用 | reload 长连接失效 |

### 核心优势（一句话版）

1. **性能**：异步 Rust + 多线程无锁协程，release 模式下性能对标 nginx；Linux 上可选 eBPF 与 sendfile 零拷贝。
2. **协议丰富**：同一套配置完成四层端口代理、SNI 多域名、七层反代、QUIC、隧道加速。
3. **缓存更强**：面向 CDN 的 HTTP 存储（切片、合并回源、延迟 purge、文件记忆、range不额外存储节省磁盘、reload 长连接可复用等），减少回源带宽。
4. **可编程**：WASM 插件按阶段嵌入（access / filter / serverless / access_log），无需重启即可 reload。
5. **可观测**：access_log 内置大量预制变量，跨节点 request_id 贯穿全链路。
6. **跨平台**：Windows 可用 rustls 内嵌 TLS，无需安装 OpenSSL。

---

## 架构概览

```
                    ┌─────────────────────────────────────────┐
                    │              anyproxy 进程                 │
                    │  ┌─────────┐  ┌──────────┐  ┌─────────┐ │
  Client ──────────►│  │ listen  │─►│  proxy   │─►│ upstream│─┼──► Origin
  (TCP/QUIC/SSL/    │  │ net/    │  │ L4 / L7  │  │ LB/DNS  │ │
   HTTP/WS)         │  │ server  │  │ + cache  │  │         │ │
                    │  └─────────┘  └────┬─────┘  └─────────┘ │
                    │                    │ WASM 插件（可选）      │
                    │                    ▼                      │
                    │              access_log / metrics         │
                    └─────────────────────────────────────────┘
```

**配置三级结构**：`net`（全局/监听） → `server`（域名/端口绑定） → `local`（具体业务：反代、静态、缓存、WASM）。

**模块加载流程**：解析配置 → 各模块预解析/合并/继承 → 创建工作线程 → 开始监听。

---

## 快速开始（5 分钟验证）

以下步骤在 **Linux / macOS / Windows** 均可完成。目标：启动一个 HTTP 静态文件服务并用 curl 验证。

### 1. 环境准备

**Rust 工具链（固定 1.96.0）**

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh   # 未安装时
rustup install 1.96.0
rustup default 1.96.0
```

| 平台 | 额外依赖 |
|------|----------|
| **Linux** | `gcc g++ make pkg-config openssl libssl-dev` |
| **Windows** | 推荐 `--features anyproxy-rustls`，免装 OpenSSL |
| **WASM 开发**（可选） | `cargo install cargo-component` |

> **重要**：生产与压测必须使用 `--release` 编译，debug 与 release 性能可差数十倍。

### 2. 编译

```bash
git clone https://github.com/yefy/any-proxys.git
cd any-proxys

# Linux / macOS（OpenSSL）
cargo build --release --bin anyproxy

# Windows 或不想装 OpenSSL（内嵌 rustls，推荐）
cargo +1.96.0 build --release --bin anyproxy \
  --no-default-features --features "anyproxy-rustls"
```

产物：`target/release/anyproxy`（Windows 为 `anyproxy.exe`）

### 3. 创建最小运行目录

```bash
mkdir -p demo/conf demo/html demo/logs demo/cert
echo "<h1>anyproxy works!</h1>" > demo/html/index.html
cp target/release/anyproxy demo/    # Windows: copy target\release\anyproxy.exe demo\
```

### 4. 写入配置文件

创建 `demo/conf/anyproxy.conf`：

```conf
common {
    shutdown_timeout u64 = 10;
}

net {
    server {
        domain str = "www.example.cn";

        net_server_http raw = r```
        ```r;

        domain_listen_tcp raw = r```
            address = "127.0.0.1:19090"
        ```r;

        local {
            net_server_http_static raw = r```
                path = "./html"
            ```r;
        }
    }
}
```

### 5. 配置 hosts

```
127.0.0.1  www.example.cn
```

- Linux / macOS：`/etc/hosts`
- Windows：`C:\Windows\System32\drivers\etc\hosts`（需管理员权限）

### 6. 启动并验证

```bash
cd demo
./anyproxy -c conf/anyproxy.conf          # Windows: anyproxy.exe -c conf\anyproxy.conf
```

另开终端：

```bash
# 检查配置（不启动服务）
./anyproxy -t -c conf/anyproxy.conf

# 功能验证
curl http://www.example.cn:19090/         # 期望返回 HTML
curl http://www.example.cn:19090/index.html -v
```

看到 `anyproxy works!` 即表示 **编译、配置、监听、静态服务** 全链路正常。

---

## 进阶：三节点联调

仓库内置 **源站 → 中转 → 边缘** 完整示例，用于验证四层/七层代理、隧道、缓存等组合能力。

### 拓扑

```
  curl ──► 边缘 (any-example/anyproxy)
              │
              ▼
         中转 (any-example/anyproxy_p)
              │
              ▼
         源站 (any-example/anyproxy_o)  ← 也可换 nginx 作源站
```

### 步骤

**① 编译并分发二进制**

```bash
cargo build --release --bin anyproxy
cp target/release/anyproxy any-example/anyproxy_o/
cp target/release/anyproxy any-example/anyproxy_p/
cp target/release/anyproxy any-example/anyproxy/
```

**② 配置 hosts**

```
127.0.0.1  www.example.cn
127.0.0.1  www.upstream.cn
```

**③ 修改静态文件路径（必做）**

示例配置中 `net_server_http_static` 的 `path` 指向作者本机目录，需改为你本地的静态资源路径，例如 nginx 的 html 目录或自建 `./html`：

```conf
net_server_http_static raw = r```
    path = "./html"
```r;
```

涉及文件：

- `any-example/anyproxy_o/conf/anyproxy_origin.conf`
- `any-example/all_conf/anyproxy_simple_*.conf`（若使用最小示例）

**④ 按顺序启动三个节点**

```bash
# 终端 1 — 源站（HTTP :19090，HTTPS :19091）
cd any-example/anyproxy_o
./anyproxy -c conf/anyproxy_origin.conf

# 终端 2 — 中转
cd any-example/anyproxy_p
./anyproxy -c conf/anyproxy_proxy_to_origin.conf

# 终端 3 — 边缘
cd any-example/anyproxy
./anyproxy -c conf/anyproxy_edge_to_proxy.conf
```

**⑤ 验证命令**

```bash
# 直连源站
curl http://www.example.cn:19090 -v
curl https://www.example.cn:19091 -k -v

# 经边缘代理（具体端口见各 conf 文件末尾 curl 注释）
curl http://www.example.cn:10001 -v          # 四层 port 代理
curl http://www.example.cn:33100/1.txt -v    # 七层 HTTP 反代（http_simple）
```

**运行目录约定**（每个节点相同）：

| 目录 | 用途 |
|------|------|
| `cert/` | TLS 证书（示例已自带 `www.example.cn` 自签证书） |
| `conf/` | 配置文件 |
| `logs/` | `access.log`、`anyproxy.log`、`anyproxy.pid` |
| `tmp/` | 临时文件 / 上传缓存 |
| `html/` | 静态资源（自行创建） |

> 也可用 nginx 作源站，见 [nginx 源站配置说明](any-proxy/doc/nginx源站支持http和https.md)。

---

## 配置说明

### 顶层结构

语法类似 nginx：块结构、`include`、行注释 `#`、块注释 `r### ... ###r`、跨平台字段。

```conf
common { }              # 全局：shutdown_timeout、reuseport 等
socket { }              # TCP/QUIC 默认参数
upstream { server { } } # 回源池 + 负载均衡

net {
    server {            # 监听 + 域名
        local { }       # 业务：反代 / 静态 / 缓存 / WASM
    }
}
```

### 监听模式

| 模式 | 配置项 | 路由方式 | 典型用途 |
|------|--------|----------|----------|
| **port** | `port_listen_tcp/ssl/quic` | 按端口 | 一端口一服务，四层转发 |
| **domain** | `domain_listen_tcp/ssl/quic` | SNI / HTTP Host / 内部 hello 头 | 同端口多域名 |

domain 模式支持泛域名，例如 `"$$(,*).example.cn"`。

### 配置类型速查

```conf
# 标量
timeout u32 = 30s;
limit u32 = 10m;

# 跨平台（仅对应平台解析）
path linux_raw  = r``` /var/www/html ```r;
path window_raw = r``` C:/www/html ```r;

# 原始块（TOML/JSON 等，交给子模块）
proxy_pass_tcp raw = r```
    address = "127.0.0.1:8080"
```r;
```

完整语法：[配置文件结构](any-proxy/doc/配置文件结构.md)

---

## 常用场景配置

最小可运行示例均在 `any-example/all_conf/`，可直接 `-c` 加载（记得改静态路径和 hosts）。

### 四层 port 代理

`anyproxy_simple_port.conf` — 监听 `:30001`，TCP 转发到源站：

```conf
net {
    server {
        domain str = "www.example.cn";
        port_listen_tcp raw = r```
            address = "0.0.0.0:30001"
        ```r;
        local {
            proxy_pass_upstream str = "//tcp//www.upstream.cn:19090//round_robin";
        }
    }
}
```

```bash
curl http://www.example.cn:30001 -v
```

### 四层 domain 代理（HTTPS + SNI）

`anyproxy_simple_domain.conf` — 同端口按 SNI 分发：

```bash
curl https://www.example.cn:30101 -k -v
```

### 七层 HTTP 反向代理

`anyproxy_simple_http_proxy.conf`：

```bash
curl http://www.example.cn:20201 -v
```

### HTTP 静态服务

`anyproxy_simple_http_static.conf`：

```bash
curl http://www.example.cn:19090 -v
```

### 多节点回源 + 负载均衡

```conf
upstream {
    server {
        name str = upstream_server_0;
        balancer str = round_robin;   # weight / random / ip_hash / ip_hash_active / fair
        proxy_pass_tcp raw = r```
            address = "10.0.0.1:8080"
        ```r;
        proxy_pass_tcp raw = r```
            address = "10.0.0.2:8080"
        ```r;
    }
}

net {
    server {
        local {
            proxy_pass_upstream str = "upstream_server_0";  # 引用 upstream 块
        }
    }
}
```

简写（单条等价配置）：

```conf
proxy_pass_upstream str = "//tcp//www.upstream.cn:8080//round_robin";
```

详见 [upstream 和 proxy_pass](any-proxy/doc/upstream和proxy_pass.md)、[负载均衡](any-proxy/doc/负载均衡.md)。

### Linux eBPF 四层加速

```bash
cargo build --release --features "anyproxy-openssl,anyproxy-ebpf" --no-default-features
```

配置参考 `any-example/all_conf/anyproxy_simple_ebpf.conf`。需根据流量调整 TCP 缓冲区，**读缓冲消耗速率须大于写缓冲**。

---

## 运维与生产部署

### 命令行

| 命令 | 说明 |
|------|------|
| `anyproxy -c <path>` | 指定配置文件（默认 `./conf/anyproxy.conf`） |
| `anyproxy -t [-c <path>]` | 检查配置语法，不启动服务 |
| `anyproxy -s reload` | 热加载配置（不重建线程，推荐日常使用） |
| `anyproxy -s reinit` | 重建线程 + 热加载（影响 UDP/QUIC/tunnel） |
| `anyproxy -s quit` | 优雅退出（默认等待 30s，`common.shutdown_timeout` 可调） |
| `anyproxy -s stop` | 立即停止，强制断流 |
| `anyproxy --hot <pidfile>` | 二进制热升级：启动新进程并关闭旧进程 |

### 信号（Linux）

| 信号 | 等价命令 |
|------|----------|
| `HUP` | reload |
| `USR1` | reinit |
| `USR2` | 检查配置 |
| `QUIT` / `INT` | 优雅停止 |
| `TERM` | 快速停止 |

### 日志

- **错误日志**：`logs/anyproxy.log`（log4rs 配置见各示例目录 `conf/log4rs.yaml`）
- **访问日志**：在配置中声明 `access raw = r``` ... ```r`，支持 `${request_id}`、`${client_addr}`、`${upstream_addr}`、`${http_cache_status}` 等 [预制变量](any-proxy/doc/预制变量.md)
- **日志切割**：[日志切割说明](any-proxy/doc/日志切割.md)

### systemd 参考

```ini
[Service]
PIDFile=/usr/local/anyproxy/logs/anyproxy.pid
ExecStartPre=/usr/local/anyproxy/anyproxy -t
ExecStart=/usr/local/anyproxy/anyproxy -c /usr/local/anyproxy/conf/anyproxy.conf
ExecReload=/bin/kill -s HUP $MAINPID
ExecStop=/bin/kill -s QUIT $MAINPID
```

rpm 包自带 service 文件：`any-proxy/rpm/anyproxy.service`

### 生产建议

1. 始终 `--release` 编译；Linux 生产环境优先部署在裸机或容器内，开启 `reuseport`。
2. 变更配置前先 `anyproxy -t`；日常发配置用 `reload`，变更监听/线程模型时用 `reinit`。
3. 发版使用 `--hot` 热升级，避免大量长连接同时断开。
4. TLS 生产环境替换 `cert/` 下自签证书；OpenSSL 与 rustls 通过编译 feature 切换。
5. 跨 anyproxy 节点转发时启用 `proxy_protocol_hello` 传递真实客户端 IP 与 `request_id`（**仅限节点间**，外部服务不可识别）。

---

## WebAssembly 扩展

anyproxy 内置无锁异步 WASM 运行时，通过配置挂载脚本，**reload 即可热更新**，无需重启进程。

### 适用场景

- IP 黑白名单、防盗链、简易 WAF（`wasm_access` / `wasm_http_access`）
- 无独立后端的边缘计算（`wasm_serverless`）
- 请求/响应头改写（`wasm_http_in_headers` / `wasm_http_filter_headers*`）
- 自定义 access 日志与流量统计（`wasm_access_log`）

### 配置示例

```conf
wasm_access raw = r```
    [[wasm]]
    wasm_path = "./plugins/my_filter.wasm"
    wasm_main_config = ''' name = "ip_allowlist" '''
    is_open = true
```r;
```

### 开发

- 示例项目：`wasm/http-demo`、`wasm/http-filter-headers`、`wasm/http-serverless`
- 接口定义：`wasm/wit/`（`wasm_std`、`wasm_http`、`wasm_socket`、`wasm_websocket`、`wasm_log`、`wasm_store`）
- 详细文档：[webassembly.md](any-proxy/doc/webassembly.md)

---

## any-tunnel 加速协议

**any-tunnel** 构建在 TCP / SSL / QUIC 之上，用多条连接组成隧道，解决：

- 高延迟链路下单连接带宽受限
- 短连接频繁握手（TCP 三次握手 / TLS 多次握手）

可选连接池进一步复用隧道，降低握手开销。在配置中通过 `tunnel { }` 全局块和 `proxy_pass_tunnel` 使用。

独立示例：

```bash
cargo run --release --example any_tunnel_server
cargo run --release --example any_tunnel_client
```

> **any-tunnel2** 为实验性内网穿透方案（M:N 长连接），稳定性与性能仍在验证，生产环境请优先使用 any-tunnel。

文档：[any-tunnel/README.md](any-tunnel/README.md) · [any-tunnel2/README.md](any-tunnel2/README.md)

---

## 编译选项

| Cargo Feature | 说明 |
|---------------|------|
| `anyproxy-openssl`（默认） | 使用系统 OpenSSL |
| `anyproxy-rustls` | 内嵌 rustls，免系统 OpenSSL（Windows 友好） |
| `anyproxy-ebpf` | Linux eBPF 内核态四层加速 |
| `anyproxy-reuseport` | SO_REUSEPORT 多进程/线程绑定 |
| `anyproxy-check` | 开发调试（死锁检测等，勿用于生产） |

```bash
# 典型 Linux 生产构建
cargo build --release --bin anyproxy

# 典型 Windows 构建
cargo +1.96.0 build --release --bin anyproxy --no-default-features --features "anyproxy-rustls"

# Linux + eBPF
cargo build --release --bin anyproxy --no-default-features --features "anyproxy-openssl,anyproxy-ebpf"
```

---

## 项目结构

```
any-proxys/
├── any-proxy/              # 核心：anyproxy 主程序与 doc/ 文档
├── any-base/               # 基础库：模块系统、配置解析、异步执行器
├── any-tunnel/             # 隧道协议（链路加速）
├── any-tunnel2/            # 实验性内网穿透
├── 3rdparty/hyper-0.14.23/ # 二次开发 hyper（含 Linux sendfile）
├── wasm/                   # WASM 插件示例
├── any-example/            # 可运行示例（all_conf/ 最小配置 + 三节点联调）
├── any-test/               # 压力测试
└── any-tools/              # 辅助工具
```

---

## 文档索引

| 主题 | 文档 |
|------|------|
| 编译与运行 | [anyproxy编译运行.md](any-proxy/doc/anyproxy编译运行.md) |
| 配置结构 | [配置文件结构.md](any-proxy/doc/配置文件结构.md) |
| 全量配置参考 | [全部配置文件详细说明.md](any-proxy/doc/全部配置文件详细说明.md) + `any-example/all_conf/anyproxy_demo.conf` |
| 四层 port/domain | [四层代理port与domain模式配置.md](any-proxy/doc/四层代理port与domain模式配置.md) |
| 监听配置 | [listen配置.md](any-proxy/doc/listen配置.md) |
| 回源与负载均衡 | [upstream和proxy_pass.md](any-proxy/doc/upstream和proxy_pass.md) · [负载均衡.md](any-proxy/doc/负载均衡.md) |
| 预制变量 | [预制变量.md](any-proxy/doc/预制变量.md) |
| access_log | [access_log支持.md](any-proxy/doc/access_log支持.md) |
| 信号与热加载 | [信号处理.md](any-proxy/doc/信号处理.md) |
| WebAssembly | [webassembly.md](any-proxy/doc/webassembly.md) |
| 内部协议头 | [proxy_protocol_hello.md](any-proxy/doc/proxy_protocol_hello.md) |
| 项目地图 | [project-map.md](project-map.md) |

---

## 常见问题

**Q: curl 连接被拒绝？**

- 确认 `anyproxy` 进程已启动且监听地址/端口与 curl 一致。
- 检查 hosts 是否配置、`防火墙` 是否放行。
- Windows 注意以管理员身份修改 hosts。

**Q: 配置检查失败？**

```bash
./anyproxy -t -c conf/anyproxy.conf
```

根据报错行号修正；常见原因是 `raw` 块内 TOML 语法错误或路径不存在。

**Q: 示例静态文件 404？**

示例 conf 中 `path` 多为作者本机绝对路径，必须改为你本地的 `./html` 或实际目录。

**Q: HTTPS 证书错误？**

示例使用自签证书，curl 需加 `-k`。生产环境替换 `cert/` 下 pem 文件并在 `domain_listen_ssl` 中引用。

**Q: Windows 编译 OpenSSL 报错？**

使用 rustls 构建：`--no-default-features --features "anyproxy-rustls"`。


**Q: 性能远低于预期？**

确认使用 `--release` 构建；Linux 上检查是否开启 sendfile；压测工具见 `any-test/`。

---

## 版本与许可

- 当前 crate 版本：`any-proxy` 3.0.0（见 `any-proxy/Cargo.toml`）
- 许可证：见仓库根目录 [LICENSE](LICENSE)

---

**下一步建议**：完成 [快速开始](#快速开始5-分钟验证) 后，按需阅读 `any-example/all_conf/` 中与你的场景最接近的配置，再对照 [全部配置文件详细说明](any-proxy/doc/全部配置文件详细说明.md) 做生产裁剪。
