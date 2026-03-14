# Roche — Product Requirements Document

**版本:** 0.1.0
**日期:** 2026-03-13
**状态:** Draft

---

## 1. 产品定位

Roche 是一个**通用 sandbox 编排工具**，为 AI agent 提供安全的代码执行环境。它在多种 sandbox provider（Docker、Firecracker、WASM）之上提供统一抽象，以 AI 优化的安全默认值运行不受信任的代码。

**命名：** 取自天文学家 Édouard Roche。洛希极限（Roche limit）是天体不可逾越的物理瓦解边界；Roche 是代码不可逾越的执行边界。

---

## 2. 问题陈述

### 2.1 当前痛点

每个 AI agent 框架（LangChain、CrewAI、AutoGen 等）都在独立集成 sandbox provider（Docker、E2B、Modal），形成 **N×M** 的集成矩阵：

```
LangChain ──┐         ┌── Docker
CrewAI   ───┤  N × M  ├── E2B
AutoGen  ───┘         └── Modal
```

- **重复工程：** 每个框架各自实现 Docker 容器管理、生命周期控制、安全配置。
- **安全不一致：** 各框架安全默认值不同，有些甚至默认开放网络。
- **Provider 锁定：** 框架与 sandbox provider 紧耦合，切换 provider 需要大量改动。

### 2.2 目标状态

Roche 将 N×M 降为 **N+M**：

```
LangChain ──┐              ┌── Docker
CrewAI   ───┤── Roche() ───├── Firecracker
AutoGen  ───┘              └── WASM
```

- 框架只需集成 Roche，Roche 内部适配多个 provider。
- 统一的 AI 安全默认值。
- Provider 切换只需改一个参数。

---

## 3. 目标用户

| 角色 | 描述 | 核心需求 |
|------|------|----------|
| **Agent 开发者** | 构建 AI agent 的工程师 | 简单 API、安全默认值、快速启动 |
| **框架作者** | LangChain/CrewAI 等框架的维护者 | 统一 sandbox 抽象、减少维护负担 |
| **Platform 工程师** | 部署和运维 AI agent 平台 | 资源控制、多 provider 选择、可观测性 |
| **安全团队** | 审计 AI agent 的代码执行 | 默认安全、网络隔离、只读文件系统 |

---

## 4. 核心功能

### 4.1 三个核心操作

Roche 的 API 极简——只有三个核心操作加一个查询操作：

| 操作 | 描述 |
|------|------|
| `create` | 创建 sandbox 实例，返回唯一 ID |
| `exec` | 在已有 sandbox 中执行命令 |
| `destroy` | 销毁 sandbox，释放资源 |
| `list` | 列出所有活跃的 sandbox |

### 4.2 AI 安全默认值

Roche 为 AI agent 场景设计，默认值偏向安全：

| 配置项 | 默认值 | 理由 |
|--------|--------|------|
| 网络 | **禁用** | 防止数据泄露、C2 通信 |
| 文件系统 | **只读** | 防止持久化攻击、文件篡改 |
| 超时 | **300 秒** | 防止资源耗尽、无限循环 |
| 镜像 | `python:3.12-slim` | 最小化攻击面 |

需要开放时，用户必须**显式 opt-in**（`--network`、`--writable`）。

### 4.3 Provider 抽象

统一的 `SandboxProvider` trait，所有 provider 实现相同接口：

- **Docker**（MVP）：通过 Docker CLI 管理容器
- **Firecracker**（Phase 2）：microVM 级别隔离
- **WASM**（Phase 2）：wasmtime 轻量级沙箱
- **E2B / K8s**（Phase 3）：云端 / 编排平台集成

### 4.4 资源限制

| 资源 | 配置方式 | 示例 |
|------|----------|------|
| 内存 | `--memory` | `512m`, `1g` |
| CPU | `--cpus` | `0.5`, `1.0`, `2.0` |
| 超时 | `--timeout` | `60`, `300`, `600` |
| 环境变量 | `--env` | `KEY=VALUE` |

### 4.5 多语言 SDK

| SDK | 包名 | 阶段 |
|-----|------|------|
| CLI | `roche` | MVP |
| Python | `roche-python` (PyPI) | MVP |
| TypeScript | `roche-js` (npm) | Phase 2 |
| Rust | `roche` (crates.io) | Phase 2 |

---

## 5. 非目标（Scope Out）

以下明确**不在** Roche 范围内：

| 非目标 | 理由 |
|--------|------|
| Agent 调度和编排 | 那是 Castor 等 agent 框架的职责 |
| 工具注册和权限管理 | 那是 agent kernel 的职责 |
| LLM API 调用 | Roche 只管代码执行隔离 |
| HITL 审批流程 | 那是更高层的 agent 安全关注点 |
| 代码审计和分析 | Roche 只负责隔离执行，不审查内容 |

**Roche 是基础设施工具，不是 agent 框架。** 它和 Castor 的关系是正交的——Castor 管逻辑安全（capability、HITL、replay），Roche 管物理安全（进程/容器/VM 隔离）。

---

## 6. 与 Castor 的关系

```
Roche 不知道 Castor 存在。
Castor 不依赖 Roche。
Operator 可以选择用 Roche 来管理 Castor 的 tool server sandbox。
```

- **Castor** = 逻辑安全层（capability, HITL, replay）
- **Roche** = 物理安全层（进程/容器/VM 隔离）

两者缺一不可，但职责不同。Capability 管"能不能调"，Sandbox 管"调的时候能干什么"。

---

## 7. 竞品对比

| | Roche | E2B | 直接用 Docker | Modal |
|---|---|---|---|---|
| Provider | 多种（Docker, Firecracker, WASM...） | 仅 Firecracker | 仅 Docker | 仅 Modal |
| 部署 | 本地优先，可离线 | 云优先，需联网 | 本地 | 云服务 |
| 安全默认值 | AI agent 优化（禁网络/只读 FS） | 通用 | 默认不安全 | 通用 |
| 框架绑定 | 无，通用 | 有 E2B SDK 耦合 | 无 | 有 Modal SDK |
| 开源 | Apache-2.0 | 开源 | 开源 | 部分开源 |

**Roche 的差异化：** 本地优先、AI 安全默认值、多 provider 统一抽象、框架无关。

---

## 8. 分阶段路线图

### Phase 1 — MVP

> 目标：Docker provider 可用，CLI + Python SDK 完整。

- [ ] Docker provider 实现（create / exec / destroy / list via Docker CLI）
- [ ] CLI 接入 DockerProvider
- [ ] Python SDK（`roche-python`）
- [ ] 集成测试
- [ ] 文档和 README

### Phase 2 — 多 Provider + Daemon 模式

- [ ] gRPC API + daemon 模式（`roched`）
- [ ] Firecracker provider
- [ ] WASM provider（wasmtime）
- [ ] TypeScript SDK
- [ ] Sandbox 池（预热，减少冷启动延迟）

### Phase 3 — 生态集成

- [ ] E2B provider（hosted sandbox 兼容）
- [ ] Kubernetes provider
- [ ] GPU 支持
- [ ] `castor-sandbox` 封装层（Roche + Castor ATSP 自动注册）
- [ ] Metrics / OpenTelemetry 集成

---

## 9. 成功指标

| 指标 | MVP 目标 |
|------|----------|
| `roche create` → 可用 sandbox | < 5 秒 (Docker) |
| `roche exec` 往返延迟 | < 500ms (不含命令执行时间) |
| 安全默认值覆盖率 | 100%（网络禁用、FS 只读、超时强制） |
| Python SDK 功能对等 | CLI 所有操作可通过 SDK 完成 |

---

## 10. 技术约束

- **语言：** Rust（2021 edition），编译为单 binary
- **分发：** `cargo install` / brew / 预编译 binary
- **Docker 依赖：** MVP 要求主机安装 Docker Engine
- **异步运行时：** tokio
- **最低 Rust 版本：** stable（不使用 nightly feature）
- **License:** Apache-2.0
