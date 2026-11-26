# 04 - 管理面板 API 规范、四协议订阅分发与前端架构规划

> 本文档分为两部分：**Part A** 定义控制面暴露给前端面板及代理客户端的 REST / WebSocket / 订阅接口规范（`/api/v1/*` 与 `/sub/{token}`）；**Part B** 严格基于 `satnaing/shadcn-admin` 架构范式规划 React + TypeScript 前端工程、双角色权限路由与三维实时图表优化方案。

---

## Part A：管理面板与用户门户 API 规范 (`/api/v1/*`)

### 1. 全局通用约定

1. **认证与 CSRF 防护**：
   - 浏览器面板接口统一使用 `HttpOnly; Secure; SameSite=Lax` 的 Session Cookie（`ag_session`）。
   - 所有状态修改请求（`POST` / `PUT` / `PATCH` / `DELETE`）必须在请求头携带 `X-CSRF-Token`，与 `ag_csrf` Cookie 及服务端会话记录严格比对。
2. **统一分页查询参数**：`page`（默认 `1`）、`page_size`（默认 `20`，最大 `100`）、`sort_by`、`sort_dir`（`asc` | `desc`）。
3. **统一响应结构与错误格式**：
   - 列表返回：`{ "items": [...], "total": 120, "page": 1, "page_size": 20 }`
   - 错误返回（HTTP `4xx` / `5xx`）：
     ```json
     {
       "error": {
         "code": "PORT_CONFLICT",
         "message": "TCP 端口 443 已被同节点入站 vless-reality-in 占用",
         "details": { "transport_layer": "tcp", "port": 443 },
         "request_id": "req_01936922a81b"
       }
     }
     ```

---

### 2. 核心接口分组规范

#### 2.1 认证与当前用户 (`/api/v1/auth/*`)

| Method & Path | 权限 | 说明 |
| :--- | :--- | :--- |
| `GET /api/v1/auth/providers` | 公开 | 返回启用的 OIDC 登录入口信息（`oidc_enabled`、`provider_name`、`login_url`），供分栏登录页渲染 OIDC 跳转按钮 |
| `GET /api/v1/auth/oidc/login` | 公开 | 发起 OIDC Authorization Code + PKCE 登录（302 重定向到 Logto 等 IdP） |
| `GET /api/v1/auth/oidc/callback` | 公开 | 处理 OIDC 回调，完成 JIT 建档与 Session 签发，302 跳转至面板 |
| `POST /api/v1/auth/logout` | 已登录 | 吊销本机 `user_sessions` 并返回可选的 IdP RP-Initiated Logout URL |
| `GET /api/v1/auth/me` | 已登录 | 返回当前用户完整档案与权限状态 |

`GET /api/v1/auth/me` 响应示例（自助注册但未审批激活的新用户）：
```json
{
  "id": "11111111-1111-4111-8111-111111111111",
  "email": "alice@example.com",
  "display_name": "Alice",
  "avatar_url": "https://cdn.logto.io/avatar/alice.png",
  "role": "user",
  "panel_status": "active",
  "proxy_enabled": false,
  "stat_month": "2026-11",
  "month_up_bytes": 0,
  "month_down_bytes": 0,
  "expires_at": null,
  "granted_node_count": 0
}
```

#### 2.2 管理员 - 节点生命周期与四协议入站管理 (`/api/v1/admin/nodes/*`)

*(注：遵循 Axum 0.8 路由参数规范，全部使用 `{id}` 与 `{inbound_id}`)*

| Method & Path | 说明 |
| :--- | :--- |
| `GET /api/v1/admin/nodes?month=YYYY-MM` | 节点列表（默认本月 `stat_month`，含 `desired_state`, `runtime_state`, `agent_online`, 已启用协议列表, 实时瞬时带宽 `live_up_bps`/`live_down_bps`, 当月累计流量） |
| `POST /api/v1/admin/nodes` | 创建节点（返回一次性的明文 `api_token` 与 `hmac_secret`） |
| `GET /api/v1/admin/nodes/{id}` | 获取节点详情、TLS 配置、所有入站与最近状态 |
| `PATCH /api/v1/admin/nodes/{id}` | 修改节点名称、域名/IP、区域、共享 `tls_profile_json` 与路由模板 |
| `DELETE /api/v1/admin/nodes/{id}` | 删除节点及其入站和授权映射 |
| `POST /api/v1/admin/nodes/{id}/rotate-token` | 轮换节点开放 API Token 与 HMAC Secret（仅本次响应返回明文） |
| `GET /api/v1/admin/nodes/{id}/install-script` | 生成一键部署 `aethergate-agent` + `sing-box` 的 Shell 命令（可附带一次性安装 Token） |
| `POST /api/v1/admin/nodes/{id}/start` | 设置 `desired_state='running'`，下发 `start` 指令拉起 `sing-box` |
| `POST /api/v1/admin/nodes/{id}/stop` | 设置 `desired_state='stopped'`，下发 `stop` 指令冲刷流量并停机 `sing-box` |
| `POST /api/v1/admin/nodes/{id}/restart` | 下发 `restart` 指令重启 `sing-box` |
| `POST /api/v1/admin/nodes/{id}/reload` | 强制重新编译配置并下发 `reload` 指令 |
| `GET /api/v1/admin/nodes/{id}/config-preview` | 在控制面实时预览当前编译出的完整 `sing-box` 1.12+ JSON（隐藏私钥明文或管理员显式揭示） |
| `GET /api/v1/admin/nodes/{id}/commands` | 查询节点历史控制指令及回执状态 |
| `GET /api/v1/admin/nodes/{id}/logs` | 触发或获取 Agent 缓存的最新 500 行 `sing-box` 日志 |
| **`GET /api/v1/admin/nodes/{id}/inbounds`** | 获取该节点配置的四协议入站列表 |
| **`POST /api/v1/admin/nodes/{id}/inbounds`** | 新增单个协议入站（自动执行 TCP/UDP 端口冲突与 Hy2 跳跃区间校验，触发防抖重载） |
| **`POST /api/v1/admin/nodes/{id}/inbounds/preset-4`** | 一键为节点初始化标准四协议组合（VLESS+REALITY TCP 443 + Hy2 UDP 443 + AnyTLS TCP 8443 + TUIC UDP 9443），自动生成 REALITY 密钥对 |
| **`PATCH /api/v1/admin/nodes/{id}/inbounds/{inbound_id}`** | 修改某协议入站端口、启用开关、REALITY 参数、Hy2 限速/混淆等 |
| **`DELETE /api/v1/admin/nodes/{id}/inbounds/{inbound_id}`** | 删除入站配置 |
| `POST /api/v1/admin/tools/generate-keys` | 为前端构建器实时生成 X25519 REALITY 密钥对 (`private_key`/`public_key`)、`short_id` 或随机密码 |

#### 2.3 管理员 - 节点分组 (`/api/v1/admin/node-groups/*`)

| Method & Path | 说明 |
| :--- | :--- |
| `GET /api/v1/admin/node-groups` | 列出所有节点分组与包含节点数 |
| `POST /api/v1/admin/node-groups` | 创建节点分组 |
| `PUT /api/v1/admin/node-groups/{id}/members` | 覆盖式更新分组内节点成员（自动为已授权该组的用户触发受影响节点配置重编译） |
| `DELETE /api/v1/admin/node-groups/{id}` | 删除分组 |

#### 2.4 管理员 - 用户管理与节点授权 (`/api/v1/admin/users/*`)

| Method & Path | 说明 |
| :--- | :--- |
| `GET /api/v1/admin/users?month=YYYY-MM` | 用户列表（默认本月 `stat_month` 流量），支持过滤：`filter=pending_approval`（`panel_status='active' AND proxy_enabled=false`）、`proxy_enabled`、`panel_status`、`role`、`search` |
| `GET /api/v1/admin/users/{id}?month=YYYY-MM` | 用户详情，含已授权节点列表、直连与分组来源、指定月份在各节点的流量与实时瞬时带宽 |
| `PATCH /api/v1/admin/users/{id}/proxy-status` | `{ "proxy_enabled": true|false }` 启用/关闭用户代理；关闭时立即向关联在线节点发送 `kick_users` 并触发配置重编译 |
| `PATCH /api/v1/admin/users/{id}/panel-status` | `{ "panel_status": "active"|"banned" }` 封禁/解封面板登录；封禁时立即吊销全部 `user_sessions` 并执行 `proxy_enabled=false` 踢线流程 |
| `PATCH /api/v1/admin/users/{id}` | 更新 `role`、`expires_at`、`remark` |
| `PUT /api/v1/admin/users/{id}/grants` | `{ "node_ids": [...], "group_ids": [...] }` 全量同步用户的节点与节点组权限，自动计算差集：新增节点触发重载，被移除节点立即发送 `kick_users` + 重载 |
| `POST /api/v1/admin/users/{id}/kick` | 立即向该用户全部已授权在线节点下发 `kick_users`（不修改配置，仅强制断掉当前连接） |
| `POST /api/v1/admin/users/{id}/reset-credentials` | 重新生成 `proxy_uuid`、`proxy_password` 与 `subscription_token`，触发关联节点重载与旧会话踢下线 |

#### 2.5 三维流量与瞬时带宽统计 (`/api/v1/admin/stats/*` 与 `/api/v1/my/stats/*`)

> 普通用户调用 `/api/v1/my/stats/*` 时，服务端强制将 `user_id` 锁定为当前会话用户 ID 且仅允许查看其拥有有效授权的节点数据。所有月度汇总接口均支持可选参数 `?month=YYYY-MM`（未传则默认取统计时区当前自然月，每月 1 号 00:00 自动从 `0` 起算）。

1. `GET /api/v1/admin/stats/months`（及 `/api/v1/my/stats/months`）
   - 返回已有流量记录的可用月份数组（如 `["2026-11", "2026-10", "2026-09"]`），供前端月份选择器渲染。
2. `GET /api/v1/admin/stats/overview?month=YYYY-MM`
   - 返回全局 KPI：活跃节点数 / 总节点数、已启用代理用户数 / 待审批用户数、总在线连接数、当前全集群瞬时总上行/下行带宽 (`bps`)、今日与所选自然月累计总流量 (`bytes`)、当月 Top 5 用户与 Top 5 节点。
3. `GET /api/v1/admin/stats/traffic?dimension={user|node|user_node}&month=YYYY-MM&user_id=...&node_id=...&from=...&to=...&granularity={auto|1m|1h|1d}`
   - 传入 `month` 时返回整自然月汇总（O(1) 直查 `*_monthly_traffic` 表）及该月按日分桶趋势；传入 `from..to` 可查询任意自定义时段。
4. `GET /api/v1/admin/stats/bandwidth/series?dimension={global|user|node|user_node}&user_id=...&node_id=...&from=...&to=...&granularity={auto|5s|1m|1h|1d}`
   - 返回历史瞬时/区间平均及峰值带宽曲线：
     ```json
     {
       "granularity": "1m",
       "series": [
         {
           "ts": "2026-11-26T14:20:00Z",
           "avg_up_bps": 12450000,
           "avg_down_bps": 184200000,
           "peak_up_bps": 19800000,
           "peak_down_bps": 245000000,
           "up_bytes": 93375000,
           "down_bytes": 1381500000
         }
       ]
     }
     ```
5. `GET /api/v1/admin/stats/bandwidth/live?dimension={global|user|node|user_node}&user_id=...&node_id=...`
   - 从内存 `LiveMetricsStore` 毫秒级返回当前瞬时带宽快照与最近 5 分钟高密波形。
6. **`GET /api/v1/admin/stats/matrix?month=YYYY-MM&from=...&to=...&node_ids=...&user_ids=...`**
   - 返回**「用户 × 节点」交叉统计矩阵**（默认当前自然月，O(1) 直查 `user_node_monthly_traffic` 表，同时支持传 `from..to` 任意时间窗），每个单元格同时包含该月（或周期）累计上下行流量、当前瞬时带宽与活跃连接数：
     ```json
     {
       "stat_month": "2026-11",
       "nodes": [{ "id": "...", "name": "HK-01", "region": "HK" }],
       "users": [{ "id": "...", "display_name": "Alice", "proxy_username": "u_k9x3m2p8qa" }],
       "cells": [
         {
           "user_id": "...",
           "node_id": "...",
           "up_bytes": 1073741824,
           "down_bytes": 21474836480,
           "live_up_bps": 2400000,
           "live_down_bps": 48500000,
           "active_conns": 6
         }
       ]
     }
     ```
7. `GET /api/v1/admin/stats/protocols?node_id=...&month=YYYY-MM&from=...&to=...`
   - 传 `month` 时 O(1) 直查 `node_inbound_monthly_traffic`，传 `from..to` 时查 `node_inbound_samples_raw`，返回四协议（`hysteria2` / `vless_reality` / `tuic` / `anytls`）流量与当前带宽分布占比。

#### 2.6 实时推送 WebSocket (`/api/v1/ws/realtime` 持久连接 + 事件驱动脏推送)

1. **持久连接常驻与握手鉴权**：复用浏览器 Cookie（`ag_session`），未登录或 `panel_status='banned'` 拒绝升级（`401` / `403`）。**应用会话期间始终维持单条持久 WSS 长连接不断开**，保障节点掉线/降级秒级告警与零重连切换体验，并启用 `permessage-deflate` 压缩降噪。
2. **频道语法与 RBAC 校验**：
   - 客户端发送订阅/退订帧（仅按当前视图/视口可见行增量订阅）：
     ```json
     {
       "op": "subscribe",
       "channels": [
         "global",
         "node_status",
         "node:550e8400-e29b-41d4-a716-446655440000",
         "user:11111111-1111-4111-8111-111111111111",
         "user_node:11111111-1111-4111-8111-111111111111:550e8400-e29b-41d4-a716-446655440000"
       ]
     }
     ```
   - **严格 RBAC 过滤**：若当前会话 `role != 'admin'`，服务端拒绝订阅 `global`、`node:*`、其他用户的 `user:*` 及 `user_node:*`，仅放行 `user:{self_id}` 与属于自身已授权节点的 `user_node:{self_id}:{node_id}`。
   - **驱动服务端实时模式**：当服务端检测到任意在线 WS 会话订阅了 `global`、`node:*`、`user:*` 或 `user_node:*`（即存在活跃带宽观测者）时，`TelemetryCadenceController` 自动通过 SSE 通知节点切换为 5s `realtime` 上报；全部带宽订阅卸载保持 60s 后自动回落 `eco` 节能模式（仅依赖 `node_status` 保持长连不触发高频遥测）。
3. **事件驱动脏推送（Dirty Push，零冗余帧）**：
   - **不盲目每秒全量广播**：仅当新遥测批次到达且速率发生变化、或触发 Staleness 衰减归零时推送；持续 `0 bps` 的静止用户/节点绝不重复广播；
   - 紧凑下行速率帧：
     ```json
     {
       "type": "bandwidth_tick",
       "ts": "2026-11-26T14:32:01Z",
       "channel": "user_node:11111111-...:550e8400-...",
       "up_bps": 3250000,
       "down_bps": 84120000,
       "active_conns": 9
     }
     ```
   - 节点状态变更立即推送：
     ```json
     {
       "type": "node_state_changed",
       "node_id": "550e8400-...",
       "desired_state": "running",
       "runtime_state": "running",
       "agent_online": true
     }
     ```

#### 2.7 普通用户门户与四协议订阅分发 (`/api/v1/my/*` 与 `/sub/{token}`)

| Method & Path | 说明 |
| :--- | :--- |
| `GET /api/v1/my/nodes` | 仅当 `proxy_enabled=true` 时返回用户已获授权节点列表、各节点当前启用的四协议端点详情、用户在该节点上的四协议分享 URI 与个人流量/瞬时带宽 |
| `GET /api/v1/my/credentials` | 返回 `proxy_username`、`proxy_uuid`、`proxy_password` 及个人专属订阅 URL（仅 `proxy_enabled=true` 可见） |
| `POST /api/v1/my/subscription/rotate` | 普通用户重置自己的订阅链接 Token（防订阅泄露） |
| **`GET /sub/{token}?format={uri|singbox|clash}`** | 无需 Cookie，按 `SHA-256(token)` 鉴权。支持 **`If-None-Match` / `ETag` 返回 `304 Not Modified`**、响应启用 **Brotli/Gzip 压缩** 并携带 `Profile-Update-Interval: 24`；受单 Token（`6次/分钟`）与 IP 双限流保护（无效/超限返回空体 `403`/`429`） |

**四协议标准 URI 导出格式规范（`format=uri` 返回 Base64 编码的换行链接列表）**：
1. **VLESS + REALITY**：
   `vless://{proxy_uuid}@{host}:{port}?encryption=none&security=reality&type=tcp&flow=xtls-rprx-vision&fp={fingerprint}&sni={server_name}&pbk={public_key}&sid={short_id}#{url_encoded_node_tag}`
2. **Hysteria 2**：
   `hy2://{proxy_password}@{host}:{port}/?sni={tls_server_name}&obfs=salamander&obfs-password={obfs_pwd}&pinSHA256={pin_if_self_signed}&mport={port_hopping_range}#{url_encoded_node_tag}`
3. **TUIC v5**：
   `tuic://{proxy_uuid}:{proxy_password}@{host}:{port}/?congestion_control={cc}&alpn=h3&sni={tls_server_name}&udp_relay_mode={mode}&allow_insecure={0_or_1}#{url_encoded_node_tag}`
4. **AnyTLS**：
   `anytls://{proxy_password}@{host}:{port}/?sni={tls_server_name}&insecure={0_or_1}#{url_encoded_node_tag}`

*(注：`format=singbox` 直接生成适用于 sing-box 1.12+ 客户端的完整 `outbounds` JSON 配置，包含上述四协议原生 outbound 与 `urltest`/`selector` 分组；`format=clash` 生成兼容 Clash Meta / Mihomo 的 YAML 节点清单)*

#### 2.8 审计日志与系统设置 (`/api/v1/admin/audit-logs`, `/api/v1/admin/settings`)

| Method & Path | 说明 |
| :--- | :--- |
| `GET /api/v1/admin/audit-logs` | 按操作者 (`actor_user_id`)、动作类型 (`action`)、目标类型/ID 及时间区间分页检索操作审计流水 |
| `GET /api/v1/admin/settings` | 读取全局默认配置（默认心跳上报间隔、节点下线判定阈值、默认待审批提示文案、是否强制节点 HMAC 防重放） |
| `PATCH /api/v1/admin/settings` | 更新系统全局配置并写入审计日志 |

---

## Part B: React shadcn-admin 前端架构规划

### 3. 技术栈与目录结构（对齐 `satnaing/shadcn-admin` 与 `Linguist-New`）

#### 3.1 核心依赖选型

- **运行时与构建**：Node.js 24 (`npm ci` + `package-lock.json`)、Vite `^6.1.0`、React `^19.0.0`、TypeScript `^5.7.3`
- **路由与状态**：`@tanstack/react-router`（文件路由 + 路由守卫）、`@tanstack/react-query` v5（服务端状态缓存）、`zustand`（实时带宽环形缓冲与全局 UI 状态）
- **表格与表单**：`@tanstack/react-table` `^8.21.3`（服务端分页、排序、列显隐、筛选）、`react-hook-form` + `zod` + `@hookform/resolvers`
- **UI 与样式**：`shadcn/ui`（基于 `@radix-ui/react-*` 原语）、**Tailwind CSS `^4.3.5`** + **`@tailwindcss/vite`** + **`tw-animate-css`**（在 `src/styles/index.css` 采用 `@import "tailwindcss";` 与 `@theme` CSS 变量配置，原生支持明亮/暗黑主题切换）、`lucide-react`、`cmdk`、`sonner` `^2.0`（指令状态 Toast）、`qrcode.react`（四协议链接扫码导入）
- **可视化图表**：`recharts` `^3.8.0`（直接引用 `--chart-1` ~ `--chart-5` CSS 变量实现暗黑模式无闪烁联动）

#### 3.2 `frontend/src/` 目录结构

```text
frontend/
├── package.json
├── package-lock.json
├── eslint.config.js                    # typescript-eslint strictTypeChecked 严苛规则
├── tsconfig.json                       # 严格 TypeScript 配置 (noUncheckedIndexedAccess)
├── vite.config.ts                      # manualChunks 拆包 + third-party-notices.txt 生成
└── src/
    ├── main.tsx
    ├── styles/
    │   └── index.css                   # Tailwind CSS v4 @theme 变量定义 (含 --chart-1..5)
    ├── components/
    │   ├── ui/                         # shadcn/ui 基础组件 (Button, Card, Dialog, Tabs, Switch, Badge...)
    │   ├── layout/                     # AppSidebar (按角色动态菜单), Header, ThemeSwitch, UserNav
    │   ├── data-table/                 # 复用型 DataTable, Pagination, FacetedFilter, Toolbar
    │   └── charts/                     # LiveBandwidthChart (流式折线), TrafficBarChart, ProtocolPieChart
    ├── features/
    │   ├── auth/                       # 登录引导页 (跳转 /api/v1/auth/oidc/login) 与登出处理
    │   ├── dashboard/                  # 管理员全局总览 (KPI、全集群实时带宽、Top 榜单、节点状态网格)
    │   ├── nodes/
    │   │   ├── components/
    │   │   │   ├── node-columns.tsx    # 状态徽章 (desired vs runtime)、协议标签、实时速率列
    │   │   │   ├── node-actions-bar.tsx# 一键启动/停止/重启/重载带二次确认与指令回执
    │   │   │   ├── inbound-builder.tsx # 四协议可视化配置卡片 (VLESS-REALITY / Hy2 / TUIC / AnyTLS)
    │   │   │   ├── tls-profile-card.tsx# 节点级共享 TLS 证书配置与 pinSHA256 展示
    │   │   │   └── install-command-modal.tsx
    │   │   └── pages/                  # 节点列表页 & 节点详情页 (含本节点全部用户明细表)
    │   ├── node-groups/                # 节点分组管理与节点成员穿梭框
    │   ├── users/
    │   │   ├── components/
    │   │   │   ├── user-columns.tsx    # 待审批高亮标记、proxy_enabled 快捷开关、累计流量与实时速率列
    │   │   │   ├── user-grants-dialog.tsx # 节点 + 节点组授权穿梭框
    │   │   │   └── user-detail-tabs.tsx# 用户详情 + "该用户在各节点" 流量/瞬时带宽明细
    │   │   └── pages/
    │   ├── analytics/                  # 三维统计探索中心 (用户 / 节点 / 用户×节点交叉矩阵 / 四协议分布)
    │   ├── portal/
    │   │   ├── pending-approval.tsx    # 普通用户 proxy_enabled=false 专用待激活引导页
    │   │   ├── my-nodes-list.tsx       # 已启用用户的授权节点与实时个人带宽卡片
    │   │   └── subscription-box.tsx    # 四协议单条分享链接/二维码 + sing-box/Clash 订阅导出
    │   ├── audit-logs/                 # 操作审计记录查询
    │   └── settings/                   # 系统全局默认参数与外观设置
    ├── routes/                         # TanStack Router 文件路由 (仅作轻量页面挂载)
    │   ├── __root.tsx
    │   ├── sign-in.tsx
    │   ├── _authenticated.tsx          # 拉取 /api/v1/auth/me，未登录重定向 /sign-in，banned 展示封禁页
    │   ├── _authenticated/index.tsx    # 自动分流: admin -> Dashboard / user -> Portal
    │   ├── _authenticated/portal/*
    │   └── _authenticated/_admin/      # 管理员守卫: role !== 'admin' 重定向 /portal
    │       ├── nodes/
    │       ├── node-groups/
    │       ├── users/
    │       ├── analytics/
    │       ├── audit-logs/
    │       └── settings/
    ├── stores/
    │   ├── use-live-metrics-store.ts   # 维护各通道最近 60 个点的实时环形缓冲
    │   └── use-ui-store.ts
    └── lib/
        ├── api-client.ts               # fetch 封装: 自动注入 X-CSRF-Token, 统一解析 AppError
        ├── ws-manager.ts               # WebSocket 单例管理器: 引用计数订阅 + 指数退避重连
        └── formatters.ts               # formatBytes (1024) 与 formatBps (1000) 严格分离
```

#### 3.3 严格 TypeScript / ESLint 检查与 Vite 构建规范

完全对齐 `Linguist-New` 的前端质量门禁与内嵌构建策略：

1. **TypeScript 严格配置 (`frontend/tsconfig.json`)**：
   ```json
   {
     "compilerOptions": {
       "target": "ES2022",
       "module": "ESNext",
       "moduleResolution": "bundler",
       "jsx": "react-jsx",
       "strict": true,
       "noImplicitAny": true,
       "noUnusedLocals": true,
       "noUnusedParameters": true,
       "noUncheckedIndexedAccess": true,
       "noFallthroughCasesInSwitch": true
     }
   }
   ```
2. **ESLint 强类型校验 (`frontend/eslint.config.js`)**：启用 `tseslint.configs.strictTypeChecked`，并显式禁止 `@typescript-eslint/no-explicit-any: "error"`、`@typescript-eslint/no-floating-promises: "error"`、`@typescript-eslint/restrict-template-expressions`（允许 number/boolean）及 `react-hooks` / `react-refresh` 规则。
3. **Vite 打包、预压缩与内嵌集成 (`frontend/vite.config.ts`)**：
   - 使用 `@tailwindcss/vite` 与 `@tanstack/router-plugin`；**面板严格使用系统无衬线/等宽字体栈，不打包自托管大体积 WebFont**；
   - `build.rollupOptions.output.manualChunks` 将 `recharts`/`d3-*`、`@tanstack/*`、`@radix-ui/*` 独立分包；
   - 打包后自动生成 **Brotli (level 11, `.br`) 与 Gzip (`.gz`) 预压缩文件**（连同许可证文件 `dist/third-party-notices.txt` 一并嵌入单文件二进制，大幅降低主站首次加载出网流量）；
   - `server.proxy` 本地开发将 `/api`（含 `ws: true`）、`/sub`、`/healthz` 代理至 `http://127.0.0.1:8080`。

---

### 4. 双角色页面路由与核心交互设计

#### 4.1 角色路由与访问隔离矩阵

| 路由路径 | 允许角色 | 交互行为与安全保障 |
| :--- | :--- | :--- |
| `/sign-in` | 访客 | 采用 `shadcn-admin` 分栏式（Split-Screen）登录页布局：左侧展示 Æthergate 品牌标识、系统架构摘要与节点流量概览卡片，右侧展示「使用 OIDC / Logto 登录或注册」主跳转按钮及安全提示（无本地密码表单），点击后直接跳转 `/api/v1/auth/oidc/login` |
| `/` & `/portal` | `user` / `admin` | 若为 `user` 且 `proxy_enabled === false`，渲染 **`PendingApprovalView`**（展示账号信息、OIDC 绑定邮箱、「您的账号已注册成功，当前仅限访问面板，代理权限与可用节点需由管理员审核开通」提示），完全隐藏订阅链接与节点详情；若 `proxy_enabled === true`，渲染节点卡片、四协议二维码、个人实时带宽与**按自然月统计的个人及分节点流量明细**（顶部内置月份选择器，每月 1 号 00:00 自动归零开启新月，支持切换历史月份） |
| `/admin/dashboard` | 仅 `admin` | 管理员全局驾驶舱：全集群实时总上/下行带宽流式双折线图、在线节点健康矩阵、待审批新用户快速通道、本月（1 号起算，可切历史月份）流量与 Top 用户/节点榜 |
| `/admin/nodes` & `/admin/nodes/$nodeId` | 仅 `admin` | 节点列表 + 详情多 Tab（含月份选择器）：**[运行态与实时带宽]**（含该节点下所有用户的瞬时带宽与月度流量表 + 踢人操作）、**[四协议入站配置]**、**[协议流量占比]**、**[编译配置预览与实时日志]** |
| `/admin/users` & `/admin/users/$userId` | 仅 `admin` | 用户列表顶部提供 `全部` / `待审批 (${count})` / `已启用` / `已禁用` 快捷标签及月份选择器；详情页包含「一键启停代理」「节点/分组授权」「该用户在各节点的月度流量与瞬时带宽明细」 |
| `/admin/analytics` | 仅 `admin` | **三维统计探索中心**：支持切换「按用户」「按节点」「**用户 × 节点交叉矩阵**」视图，默认按当前自然月（每月 1 号从 `0` 起算）展示并支持切换历史月份或自定义时间段、多实体同图曲线对比及 CSV 导出 |

#### 4.2 节点「四协议配置构建器」交互设计

在 `/admin/nodes/$nodeId` 的「协议入站」Tab 中，为降低 `sing-box` 1.12+ 配置出错概率，界面呈现 **4 个独立协议卡片 + 1 个共享 TLS 证书卡片**：

1. **顶部提示条**：若节点尚未创建任何入站，提供**「一键初始化四协议标准配置」**按钮，自动分配互不冲突的推荐端口（VLESS+REALITY TCP `443`、Hysteria 2 UDP `443`、AnyTLS TCP `8443`、TUIC UDP `9443`）并调用后端实时生成 X25519 REALITY 密钥对。
2. **协议卡片字段与实时校验**：
   - **VLESS + REALITY**：开关、端口、SNI/Handshake 服务器地址与端口、一键生成 `Private/Public Key` & `Short ID`、`Flow`（默认 `xtls-rprx-vision`）、`Fingerprint`。
   - **Hysteria 2**：开关、端口、端口跳跃范围（如 `20000-30000`）、上/下行速率上限 (`up_mbps`/`down_mbps`)、混淆模式（`none` / `salamander`）、伪装 URL (`masquerade`)。
   - **TUIC v5**：开关、端口、拥塞控制（`bbr` / `cubic` / `new_reno`）、UDP 中继模式（`native` / `quic`）、`0-RTT` 开关、心跳间隔。
   - **AnyTLS**：开关、端口、`padding_scheme` 预设模板（默认均衡 / 高混淆 / 自定义行）。
   - **实时防冲突校验（Zod）**：修改任一卡片端口或端口跳跃区间时，前端表单即时按传输层（TCP 与 UDP 分组）检测同节点冲突并标红阻断提交（同时允许 TCP 443 与 UDP 443 共存）。

#### 4.3 持久 WebSocket 策略与实时流式图表性能优化规范

1. **持久 WebSocket 单例与引用计数 (`ws-manager.ts`)**：
   - 应用全局始终**保留一条持久 WSS 连接不断开**，登录后即保持连接直至登出或页面关闭，确保节点状态变更/故障告警秒级触达与路由切换零重连延迟；
   - 组件挂载时通过 `useRealtimeChannel(channels)` 增加频道引用计数并发送增量 `subscribe`，只订阅当前视图/视口可见实体（严禁列表页全量订阅）；组件卸载归零后发送 `unsubscribe`。
2. **后台标签页优雅降载（不断连、只停重型带宽频道）**：
   - 监听 `document.visibilityState`：当页面切至后台超过 **30 秒**时，**不断开 WSS 连接并保留 `node_status` 频道**，仅临时向服务端 `unsubscribe` 暂停重型带宽曲线与交叉矩阵频道（从而使服务端在无人浏览时能触发节点 Eco 降频）；用户切回前台瞬间自动无感恢复订阅。
3. **高频更新不卡顿 React 渲染树**：WS 收到事件驱动推送的 `bandwidth_tick` 时直接写入 Zustand 外部环形缓冲区（定长 60 个样本），并基于 `1000ms` 节流触发图表数据切片刷新，避免每次数据到达触发整棵页面重渲染。
4. **主题自适应配色**：在 `src/styles/index.css` 使用 Tailwind v4 定义 `--chart-1`（上行青蓝）与 `--chart-2`（下行翡翠绿），Recharts `<Area />` / `<Line />` 直接绑定 `stroke="var(--chart-1)"`，实现系统主题切换即时响应。
