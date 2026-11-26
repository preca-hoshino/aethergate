# Æthergate — sing-box 多节点代理控制面与守护系统架构设计报告

> **项目代号**：Æthergate  
> **定位**：面向多地域 `sing-box` 代理节点的集中式控制面（Rust + React `shadcn-admin`）与轻量级节点守护程序（Rust），支持 OIDC（Logto）自助注册与默认零权限隔离、管理员精细化节点与用户启停/授权、三维流量与瞬时带宽实时监控，以及首期单节点四协议（`Hysteria 2`、`VLESS + REALITY`、`TUIC v5`、`AnyTLS`）统一编排。  
> **目标内核版本**：`sing-box` v1.12.0+ / v1.13.x（强制启用 `with_v2ray_api`、`with_clash_api` 等构建标签）

---

## 1. 文档导航

本设计报告由 6 份结构化文档组成，覆盖从架构决策、数据建模、核心协议编译、节点守护与开放 API，到管理面板前后端、生产部署安全与工程落地路线图的完整闭环：

1. **文档总览与项目蓝图**（本文档）：需求追溯矩阵、单节点四协议规格矩阵、技术栈选型与 Monorepo 工程结构。
2. [01 - 系统总体架构与核心工作流](./01-system-architecture.md)：全局物理/逻辑拓扑、9 项核心架构决策记录（ADR-001 ~ ADR-009）与 5 大端到端业务时序图。
3. [02 - 主程序后端架构、数据库与配置编译引擎](./02-backend-and-database.md)：`aethergate-server` 分层架构、单文件内嵌、`Workspace` 严苛 Lint 规范、PostgreSQL 16/17 + TimescaleDB 完整 DDL、OIDC BFF 认证、三维流量与瞬时带宽引擎、`sing-box` 1.12+/1.13+ 四协议配置编译器。
4. [03 - 节点守护程序与类 Hysteria 2 开放 API 规范](./03-node-daemon-and-open-api.md)：`aethergate-agent` 状态机与子模块设计、Pre-Reload 零丢失原子冲刷、V2Ray gRPC（`protox` 无外部依赖编译）与 Clash API 协同、WAL 断网补传、`/api/open/v1/*` 完整契约，以及 `systemd` / 节点容器双部署模式。
5. [04 - 管理面板 API 规范与 React shadcn-admin 前端规划](./04-panel-api-and-frontend.md)：`/api/v1/*` 面板与门户 REST/WebSocket 接口规范、四协议订阅分发格式，以及基于 `satnaing/shadcn-admin` 与严苛 TypeScript/ESLint 标准的路由、组件、状态、构建与实时图表架构。
6. [05 - 生产部署、安全威胁建模与 CI/CD 发布路线图](./05-deployment-security-roadmap.md)：**单文件多平台 Release 分发 + 主机/节点双 `scratch` 多架构容器**、本地 `just check` / `check.ps1` 门禁、GitHub Actions `ci-release.yml` / `deploy.yml`、STRIDE 安全模型与 Phase 0 ~ Phase 5 实施计划。

---

## 2. 需求追溯矩阵

| 核心业务需求 | 系统拆解与实现机制 | 对应文档章节 |
| :--- | :--- | :--- |
| **主程序后端（Rust）** | 基于 Axum 0.8 + Tokio + SQLx 0.8（PostgreSQL/TimescaleDB）构建单一高性能控制面 `aethergate-server`，内置配置编译器、三维带宽热路径内存缓冲与冷路径持久化引擎 | [02 - 后端架构](./02-backend-and-database.md) |
| **主程序前端（React `shadcn/admin`）** | 基于 `satnaing/shadcn-admin` 技术栈（Vite + React 19 + TanStack Router/Query/Table + shadcn/ui + Tailwind CSS v4 + Recharts + Zustand），内置管理员全局视图与普通用户门户 | [04 - 前端规划](./04-panel-api-and-frontend.md#part-b-react-shadcn-admin-前端架构规划) |
| **节点启停管理** | 采用声明式 `desired_state` (`running`/`stopped`) 与运行态 `runtime_state` 分离模型；停止节点仅优雅关闭被守护的 `sing-box` 子进程，`aethergate-agent` 常驻监听指令 | [01 - ADR-003](./01-system-architecture.md#adr-003声明式期望状态-desired_state-与实际运行态-runtime_state-分离) / [03 - 守护程序](./03-node-daemon-and-open-api.md) |
| **用户启用与关闭** | 用户具备三个正交权限字段：`role`、`panel_status`、`proxy_enabled`。禁用代理立即将其从各节点编译配置移除，并通过 Clash API `DELETE /connections/{id}` 亚秒级踢断活跃连接 | [01 - ADR-005](./01-system-architecture.md#adr-005通用-oidc-bff-会话模式与默认拒绝代理权限) / [03 - Clash 踢人](./03-node-daemon-and-open-api.md#24-clash-api-连接观测与即时踢人模块-clash_clientrs) |
| **用户可使用节点授权管理** | 支持「用户 ↔ 节点直接授权（`user_node_grants`）」与「用户 ↔ 节点分组授权（`user_node_group_grants`）」，变更触发受影响节点防抖配置重载与越权连接即时踢除 | [02 - 权限计算](./02-backend-and-database.md#24-有效授权用户计算与权限校验) |
| **三维流量与瞬时带宽统计（用户 / 节点 / 用户+节点）** | Hot Path 采用 `DashMap` 5 分钟环形缓冲（5s 槽位 + 过期衰减归零 + 持久 WS 脏推送）；Cold Path 采用 TimescaleDB Hypertable + `1m`/`1h`/`1d` Continuous Aggregates + **按自然月分桶（每月 1 号 00:00 从 0 起算、保留历月历史）的四张月度原子汇总表** | [02 - 统计引擎](./02-backend-and-database.md#4-三维流量与瞬时带宽核心引擎) |
| **节点守护程序（守护 `sing-box`）** | Rust 静态编译守护进程 `aethergate-agent`，管理 `sing-box` 生命周期、预校验配置（`sing-box check`）、Pre-Reload 流量冲刷、本机 `v2ray_api` 2s 采样与本地 WAL 断网续传 | [03 - 守护程序](./03-node-daemon-and-open-api.md#2-aethergate-agent-核心子模块详细设计) |
| **类 Hy2 开放 API（鉴权 / 拉取配置 / 上报状态）** | `/api/open/v1/*` 采用 Bearer Token + 可选 HMAC-SHA256 防重放签名，提供启动鉴权、原生兼容 Hysteria 2 External HTTP Auth 的客户端实时校验、ETag 配置/用户拉取、状态/遥测上报及出向 SSE 指令推送 | [03 - 开放 API](./03-node-daemon-and-open-api.md#3-类-hysteria-2-开放-api-规范-apiopenv1) |
| **OIDC（Logto）自助注册且仅可访问面板** | 采用标准 OIDC Authorization Code + PKCE BFF 模式，兼容 Logto 与主流 IdP；首次登录 JIT 自动建档，默认强制 `role='user'`、`proxy_enabled=false` 且零节点授权，前端展示待激活状态页 | [01 - 时序图 1](./01-system-architecture.md#31-用户-oidc-自助注册与-jit-建档) / [04 - 门户](./04-panel-api-and-frontend.md) |
| **管理员全局视图与管理能力** | 管理员可查看全局/任意用户/任意节点/用户×节点交叉矩阵的流量与瞬时带宽，支持协议分布透视、批量审批与授权、节点配置生成与实时日志排障 | [04 - 面板与前端](./04-panel-api-and-frontend.md) |
| **首期单节点四协议支持** | 单个节点运行 1 个 `sing-box` 实例同时承载 `Hysteria 2`、`VLESS + REALITY`、`TUIC v5`、`AnyTLS`，各自独立开关与端口校验，共享节点级 TLS 证书配置 | [本文档 §3](#3-首期单节点四协议规格矩阵) / [02 - 编译引擎](./02-backend-and-database.md#5-sing-box-112113-四协议共享配置编译引擎) |
| **单文件发布与双容器 CI/CD** | `aethergate-server` 通过 `rust-embed` 内嵌前端与迁移脚本实现单文件交付；`aethergate-agent` 静态 `musl` 单文件；自动化流水线产出全部单文件（附 Attestation）及主机/节点各自独立的多架构 `FROM scratch` Docker 镜像 | [01 - ADR-008](./01-system-architecture.md#adr-008单文件静态交付-rust-embed-与双-scratch-多架构容器化策略) / [05 - 交付与 CI/CD](./05-deployment-security-roadmap.md) |
| **低出网成本优化（阿里云按量 ECS 友好）** | 共享编译器下沉至 Agent 本地展开并增量同步 `/users?since_rev=`、HTTP/2 + `204` 空回包、无人看面板 Eco 降频、保留持久 WS 并启用脏推送与 `permessage-deflate`、前端 Brotli 预压缩 + SPA 白名单 404 防扫描 | [01 - ADR-009](./01-system-architecture.md#adr-009面向按量计费云主机阿里云-ecs-仅出方向计费的控制面极省出网优化架构) / [05 - 省流清单](./05-deployment-security-roadmap.md#15-阿里云按量计费-ecs-成本优化与带宽兜底清单) |

---

## 3. 首期单节点四协议规格矩阵

Æthergate 首期聚焦于当前抗封锁与传输性能最具代表性的 4 个现代协议，在单个节点内统一编排运行于同一个 `sing-box`（v1.12+）进程中。数据库模型与 Rust 枚举已预留扩展空间（`node_inbounds.protocol` + tagged enum），后续可平滑扩充其他协议。

| 维度 | `vless_reality` (VLESS + REALITY) | `hysteria2` (Hysteria 2) | `tuic` (TUIC v5) | `anytls` (AnyTLS) |
| :--- | :--- | :--- | :--- | :--- |
| **传输层** | TCP | UDP (QUIC) | UDP (QUIC) | TCP (TLS) |
| **推荐默认端口** | `443` | `443`（与 TCP 443 零冲突）或 `8443`（支持端口跳跃范围） | `9443` | `8443` 或 `2053` |
| **是否需要 TLS 证书** | 否（使用 X25519 REALITY 密钥对 + `handshake` 目标域名伪装） | 是（强制要求，引用节点级共享 TLS 配置） | 是（强制要求，引用节点级共享 TLS 配置） | 是（强制要求，引用节点级共享 TLS 配置） |
| **sing-box 最低版本要求** | `>= 1.8.0`（1.12+ 路由/DNS 兼容） | `>= 1.8.0` | `>= 1.8.0` | `>= 1.12.0`（1.12.0 原生引入） |
| **sing-box 用户认证键** | `uuid` (`proxy_uuid`) + `flow: "xtls-rprx-vision"` | `password` (`proxy_password`) | `uuid` (`proxy_uuid`) + `password` (`proxy_password`) | `password` (`proxy_password`) |
| **sing-box 统计标识** | `users[].name = proxy_username` | `users[].name = proxy_username` | `users[].name = proxy_username` | `users[].name = proxy_username` |
| **协议特色能力** | 无需域名与证书、XTLS Vision 防 TLS-in-TLS 特征、防主动探测回落 | 抗高丢包弱网、Brutal 拥塞控制、`salamander`/`gecko` 混淆、`masquerade` 伪装、端口跳跃 | 0-RTT 握手、原生 UDP 中继 (`native`/`quic`)、BBR 拥塞控制 | 内置流复用与可配置 `padding_scheme`、消除 TLS-in-TLS 握手长度特征 |
| **订阅 URI Scheme** | `vless://` | `hy2://`（兼容 `hysteria2://`） | `tuic://` | `anytls://` |

### 四协议关键设计约束

1. **端口冲突校验规则**：
   - 同节点内 **TCP 协议集合**（`vless_reality`, `anytls`）的监听端口不可重叠；
   - 同节点内 **UDP 协议集合**（`hysteria2`, `tuic`）的监听端口及 `hysteria2` 声明的端口跳跃区间（`port_hopping_range`）不可重叠；
   - **TCP 与 UDP 允许同端口共存**（例如最经典布局：`VLESS + REALITY` 占用 `TCP 443` + `Hysteria 2` 占用 `UDP 443`，`AnyTLS` 占用 `TCP 8443`，`TUIC v5` 占用 `UDP 9443`）。
2. **全局唯一代理口令约束**：
   - 由于 `sing-box` 的 `hysteria2` 与 `anytls` 入站仅通过客户端传来的 `password` 查找对应用户条目并读取 `users[].name` 计入 V2Ray 统计，若两用户密码相同，后加载条目会覆盖前一条或产生统计串账。
   - 因此后端强约束：每名用户的 `proxy_password` 必须在全系统内唯一（密码学随机 24 字节 URL-safe 字符串），并在数据库 `users` 表对 `proxy_password_hash` 建立 `UNIQUE` 约束。
3. **跨协议流量自动合一 + 按协议细分**：
   - 同一用户在单节点全部 4 个入站配置中均写入相同的 `name: "{proxy_username}"`，`sing-box` 的 `v2ray_api` 会自动把该用户在 Hy2、VLESS、TUIC、AnyTLS 上产生的流量全部累加至 `user>>>{proxy_username}>>>traffic>>>{uplink|downlink}`，保证「用户 / 节点 / 用户+节点」三维统计精确无误；
   - 同时通过 `inbound>>>{inbound_tag}>>>traffic>>>{uplink|downlink}` 收集节点四个协议各自的吞吐量，额外支撑面板的「节点协议流量占比」视图。

---

## 4. 全栈技术栈选型清单

> 依赖选型与版本基线深度参考同系列工程项目 `Linguist-New` 经过静态 `musl`（`+crt-static`）与严格 CI 门禁验证的稳定版本组合。

| 层级 | 组件 / 库 | 版本基线 | 选型依据与关键约定 |
| :--- | :--- | :--- | :--- |
| **控制面后端** | Rust + Tokio + Axum + Clap | Rust Edition 2024 (1.85+), `tokio 1.45`, `axum 0.8` (`ws`), `clap 4.5`, `tower-http 0.6`, `chrono-tz` | 高并发低内存开销；**严格注意 Axum 0.8 路径参数使用 `/{id}` 与 `/{*rest}` 语法**（旧版 `/:id` 会启动 panic）；`clap` 提供 `serve` / `check-config` / `migrate` CLI；`chrono-tz` 精准切分统计时区自然月边界 |
| **单文件内嵌资产** | `rust-embed` + `mime_guess` | `rust-embed 8.7` (`include-exclude`, `compression`), `mime_guess 2.0` | 将 `frontend/dist` 与许可声明直接嵌入 `aethergate-server` 二进制，支持 Gzip/Brotli 预压缩与 `ETag`，实现真正单文件运行 |
| **数据库访问** | SQLx | `sqlx 0.8` (`runtime-tokio`, `tls-rustls`, `postgres`, `chrono`, `uuid`, `json`, `macros`, `migrate`) | 编译期 SQL 类型校验、内置 `sqlx::migrate!()` 自动迁移、`sqlx.toml` 多 crate 配置支持 |
| **主存储与时序库** | PostgreSQL + TimescaleDB | PostgreSQL 16/17 + TimescaleDB 2.x | 统一关系业务与时间序列存储；Hypertable 自动分区、Continuous Aggregates（`1m`/`1h`/`1d`）与压缩策略（可降级为纯 PG 原生分区） |
| **可选分布式缓存** | Redis (Phase 2+ HA) | `redis 1.8` (`tokio-comp`, `connection-manager`, `tokio-rustls-comp`, `tls-rustls-webpki-roots`, `acl`, `script`) | 单实例零外部缓存依赖（进程内 `DashMap` + `arc-swap` + `broadcast`）；多实例横向扩展时作为实时带宽汇聚与 WebSocket Pub/Sub 总线 |
| **TLS 与 HTTP 客户端** | `reqwest` + `rustls` | `reqwest 0.12` (`rustls-tls-webpki-roots`, `rustls-tls-native-roots`, `stream`, `json`), `rustls 0.23`, `rustls-native-certs 0.8` | 纯 Rust TLS 栈保证静态 `musl` 与 `FROM scratch` 容器无 OpenSSL 依赖，同时兼容内置 WebPKI 公网根证书与 `SSL_CERT_FILE` 私有 CA |
| **身份认证 (OIDC)** | OIDC Discovery + JWKS | 标准 OIDC 1.0 / OAuth 2.1 (`jsonwebtoken`, `ring 0.17`, `reqwest 0.12`) | 通用 OIDC 协议接入（**Logto** 首选），支持 Discovery、PKCE S256 与 JWKS 轮转缓存；采用安全 BFF HttpOnly Cookie 会话 |
| **密码学与安全** | `aes-gcm`, `ring`, `x25519-dalek` | `ring 0.17`, `aes-gcm`, `x25519-dalek`, `argon2`, `hmac`, `sha2` | AES-256-GCM 静态加密用户密码/节点密钥；SHA-256/HMAC-SHA256 令牌摘要与防重放；X25519 生成 REALITY 密钥对；`uuid 1.16` (`v4`, `v7`) |
| **节点守护程序** | `aethergate-agent` (Rust) | 静态 `musl` 单二进制 | 共享 `aethergate-protocol` crate；`tonic` + `prost` + **`protox`**（纯 Rust 编译 `v2rayapi.proto`，零外部 `protoc` 依赖）；`sysinfo 0.40` 采集主机负载；`toml 0.8` |
| **代理核心** | `sing-box` | v1.12.0+ / v1.13.x | 官方释放二进制或含 `-tags with_v2ray_api,with_clash_api,with_quic,with_utls,with_reality_server` 编译产物 |
| **管理面板前端** | React + `shadcn-admin` | Node.js 24, `npm`, React 19, Vite 6.1+, TypeScript 5.7+, Tailwind CSS `^4.3.5` | 严格遵循 `satnaing/shadcn-admin` 架构：`@tanstack/react-router`、`@tanstack/react-query`、`@tanstack/react-table ^8.21`、`@tailwindcss/vite`、`tw-animate-css`、`@radix-ui/react-*`、`recharts ^3.8`、`zustand`、`react-hook-form` + `zod`、`sonner ^2.0`、`lucide-react`、`cmdk` |

---

## 5. Monorepo 目录工程蓝图

```text
Æthergate/
├── Cargo.toml                              # Rust Workspace (resolver = "2", workspace.lints, profile.release)
├── Cargo.lock
├── sqlx.toml                               # SQLx 0.8 工作区配置
├── justfile                                # 本地强门禁命令聚合 (check-format/lint/types/deps/test)
├── config.server.example.toml              # 控制面本地与容器参考配置
├── config.agent.example.toml               # 节点守护程序参考配置
├── .env.example
├── .github/
│   └── workflows/
│       ├── ci-release.yml                  # 质量门禁 -> 跨平台单文件构建 -> 双容器测试 -> Release + Attestation
│       └── deploy.yml                      # 控制面 Atomic Docker Compose 自动部署与探针回滚
├── crates/
│   ├── aethergate-protocol/                # 共享契约与编译库 (Server 与 Agent 共用)
│   │   ├── Cargo.toml
│   │   ├── proto/
│   │   │   └── v2rayapi.proto              # sing-box experimental/v2rayapi StatsService Protobuf
│   │   └── src/
│   │       ├── lib.rs
│   │       ├── open_api.rs                 # /api/open/v1 请求/响应结构体 (含 Hy2 Auth 兼容与增量用户 DTO)
│   │       ├── protocol.rs                 # 四协议枚举与强类型配置模型 (hysteria2/vless_reality/tuic/anytls)
│   │       ├── compiler.rs                 # 共享 sing-box 1.12+ 四协议配置编译器与端口冲突校验器 (Agent 本地展开 + Server 预览)
│   │       ├── telemetry.rs                # 批量上报、瞬时带宽与三维指标 DTO
│   │       ├── sse.rs                      # 出向 SSE 事件载荷与指令枚举 (含 telemetry_mode 自适应频控)
│   │       └── hmac.rs                     # 统一的 HMAC-SHA256 请求签名与验签逻辑
│   │
│   ├── aethergate-server/                  # 控制面主程序 (单文件内嵌 frontend/dist 与 migrations)
│   │   ├── Cargo.toml
│   │   ├── migrations/                     # SQLx 迁移脚本 (核心表 + TimescaleDB Hypertable/CAGG)
│   │   └── src/
│   │       ├── main.rs                     # clap CLI 入口 (serve / check-config / migrate)
│   │       ├── config.rs                   # TOML + *_file 密钥文件 + 环境变量加载
│   │       ├── state.rs                    # AppState (PgPool, LiveMetricsStore, NodeEventHub, RealtimeHub)
│   │       ├── error.rs                    # 统一 AppError -> HTTP JSON 映射
│   │       ├── web_assets.rs               # rust-embed 内嵌 Brotli/Gzip 前端资产与 SPA 路由白名单防扫描回落
│   │       ├── crypto/                     # AES-256-GCM、Token Hash、REALITY X25519 生成器
│   │       ├── auth/                       # OIDC BFF、JWKS 缓存、Session/CSRF、RBAC 提取器
│   │       ├── telemetry/                  # DashMap 5m 环形缓冲、原子入库聚合、TimescaleDB 查询
│   │       ├── services/                   # User/Node/Inbound/Group/Grant/Sub/Audit 领域服务与配置预览调用
│   │       ├── api/
│   │       │   ├── panel/                  # /api/v1/* 面板与用户门户 REST API (Axum 0.8 路由)
│   │       │   ├── open/                   # /api/open/v1/* 类 Hy2 节点开放 API + SSE Push (HTTP/2 + 204)
│   │       │   ├── sub/                    # /sub/{token} 多格式订阅分发 (ETag/304 + Brotli + 限流)
│   │       │   └── ws/                     # /api/v1/ws/realtime 持久 WebSocket 实时增量推流
│   │       └── workers/                    # Watchdog、WsDeltaDispatcher、TelemetryCadenceController、清理任务
│   │
│   └── aethergate-agent/                   # 节点守护程序
│       ├── Cargo.toml
│       ├── build.rs                        # protox + tonic-build 纯 Rust 编译 v2rayapi.proto
│       └── src/
│           ├── main.rs                     # clap CLI 入口 (run / check-config / version)
│           ├── config.rs                   # agent.toml 解析
│           ├── supervisor.rs               # sing-box 子进程守护、退避重启、stderr 环形日志
│           ├── stager.rs                   # 本地合并模板与增量用户 -> 共享 compiler 展开 -> check 校验 -> 冲刷与重载
│           ├── tls_helper.rs               # 本地自签证书自动生成与 pinSHA256 计算
│           ├── stats_grpc.rs               # tonic 对接 127.0.0.1 v2ray_api 2s 采样
│           ├── clash_client.rs             # 对接 127.0.0.1 clash_api 连接统计与即时踢人
│           ├── wal.rs                      # 断网本地 WAL 环形队列存储与补传
│           ├── sync_loop.rs                # 启动鉴权、SSE 事件监听、Eco/Realtime 频控上报与兜底心跳
│           └── client.rs                   # HTTP/2 开放 API 客户端 + HMAC 签名 + 压缩传输
│
├── frontend/                               # 前端 SPA (Node 24 + npm + React + satnaing/shadcn-admin 架构)
│   ├── package.json
│   ├── package-lock.json
│   ├── eslint.config.js                    # typescript-eslint strictTypeChecked 严苛规则
│   ├── tsconfig.json                       # strict + noUncheckedIndexedAccess + noImplicitAny
│   ├── vite.config.ts                      # manualChunks 拆包 + third-party-notices.txt 生成
│   └── src/
│       ├── main.tsx
│       ├── styles/index.css                # Tailwind CSS v4 (@import "tailwindcss"; @theme {...})
│       ├── components/                     # ui/, layout/, data-table/, charts/
│       ├── features/                       # dashboard/, nodes/, node-groups/, users/, analytics/, portal/, audit-logs/, settings/
│       ├── routes/                         # TanStack Router 文件路由 (__root, _authenticated, _admin)
│       ├── stores/                         # Zustand (实时带宽环形缓冲、会话与 UI 状态)
│       └── lib/                            # api-client, ws-manager, formatters (bps vs Bytes)
│
├── scripts/
│   ├── check.ps1                           # Windows 原生全量检查门禁脚本 (等价 just check)
│   ├── test-docker-server.sh               # 控制面 scratch 容器自动化验收套件
│   └── test-docker-agent.sh                # 节点守护容器 + Mock Control Plane 自动化验收套件
│
├── deploy/
│   ├── Dockerfile.server                   # 主机控制面多阶段 -> FROM scratch (65532:65532)
│   ├── Dockerfile.agent                    # 节点守护 + sing-box 多阶段 -> FROM scratch (65532:65532)
│   ├── compose.server.yaml                 # 主机容器加固编排 (read_only, cap_drop ALL, host PG/TimescaleDB)
│   ├── compose.agent.yaml                  # 节点容器加固编排 (network_mode: host, NET_BIND_SERVICE)
│   ├── systemd/aethergate-agent.service    # 裸机 systemd 最小权限部署单元
│   └── install-agent.sh                    # 裸机一键部署脚本
└── docs/                                   # 本架构与详细设计报告目录
```
