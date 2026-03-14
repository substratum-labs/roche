# Roche — Architecture

**版本:** 0.1.0
**日期:** 2026-03-13

---

## 1. 系统总览

Roche 是一个分层的 sandbox 编排系统。用户通过 CLI 或 SDK 发出指令，核心库将指令路由到具体的 sandbox provider 执行。

```
┌─────────────────────────────────────────────────────┐
│                   用户接入层                          │
│                                                     │
│   CLI (roche)    Python SDK    TypeScript SDK (P2)  │
│                                                     │
└───────────────────────┬─────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────┐
│                 roche-core (库)                       │
│                                                     │
│  ┌────────────────────────────────────────────────┐  │
│  │           SandboxProvider trait                 │  │
│  │  create() / exec() / destroy() / list()        │  │
│  └────────────────────┬───────────────────────────┘  │
│                       │                              │
│         ┌─────────────┼──────────────┐               │
│         ▼             ▼              ▼               │
│  ┌───────────┐ ┌────────────┐ ┌───────────┐         │
│  │  Docker   │ │ Firecracker│ │   WASM    │         │
│  │ Provider  │ │ Provider   │ │ Provider  │         │
│  │  (MVP)    │ │  (P2)      │ │  (P2)     │         │
│  └───────────┘ └────────────┘ └───────────┘         │
│                                                     │
└─────────────────────────────────────────────────────┘
                        │
                        ▼
┌─────────────────────────────────────────────────────┐
│               Sandbox Runtime                        │
│                                                     │
│    Docker Engine    Firecracker VMM    wasmtime      │
│                                                     │
└─────────────────────────────────────────────────────┘
```

---

## 2. Crate 结构

采用 Rust workspace，两个 crate 清晰分离库和二进制：

```
roche/
├── Cargo.toml                 # workspace 根
├── crates/
│   ├── roche-core/            # 库 crate
│   │   ├── Cargo.toml
│   │   └── src/
│   │       ├── lib.rs         # 公共 API re-export
│   │       ├── types.rs       # 核心数据类型
│   │       └── provider/
│   │           ├── mod.rs     # SandboxProvider trait + ProviderError
│   │           └── docker.rs  # DockerProvider 实现
│   └── roche-cli/             # 二进制 crate
│       ├── Cargo.toml
│       └── src/
│           └── main.rs        # clap CLI: create/exec/destroy/list
└── sdk/
    └── python/                # Python SDK
        ├── pyproject.toml
        └── roche/
            └── __init__.py
```

### 2.1 roche-core

**职责：** 定义核心抽象（trait、类型、错误），实现具体 provider。

- **不包含** CLI 逻辑、IO 格式化、用户交互
- **对外暴露：** `SandboxProvider` trait + 所有公共类型 + provider 实现
- **依赖：** tokio（async）、serde（序列化）、thiserror（错误类型）

### 2.2 roche-cli

**职责：** 命令行界面，解析参数，调用 roche-core。

- **不包含** 业务逻辑——只做参数解析、provider 实例化、输出格式化
- **依赖：** roche-core、clap（CLI 解析）、tokio（async runtime）、serde_json（JSON 输出）

### 2.3 依赖关系

```
roche-cli ──depends──> roche-core ──depends──> tokio, serde, thiserror
              │
              └──depends──> clap, serde_json
```

roche-core 不依赖 roche-cli。SDK 直接依赖 roche-core 的类型定义（通过 JSON schema 或 gRPC proto）。

---

## 3. 核心抽象

### 3.1 SandboxProvider trait

所有 provider 实现这一个 trait，四个 async 方法：

```rust
pub trait SandboxProvider {
    async fn create(&self, config: &SandboxConfig) -> Result<SandboxId, ProviderError>;
    async fn exec(&self, id: &SandboxId, request: &ExecRequest) -> Result<ExecOutput, ProviderError>;
    async fn destroy(&self, id: &SandboxId) -> Result<(), ProviderError>;
    async fn list(&self) -> Result<Vec<SandboxInfo>, ProviderError>;
}
```

**设计原则：**
- 方法签名尽量简洁——复杂配置放在 `SandboxConfig` 和 `ExecRequest` 中
- 所有方法返回 `Result`，使用统一的 `ProviderError`
- `&self` 而非 `&mut self`——provider 应该是无状态的（状态在外部运行时中）

### 3.2 核心类型

```
SandboxConfig ─── 创建 sandbox 的完整配置
  ├── provider: String          provider 名称 ("docker")
  ├── image: String             容器镜像 (default: "python:3.12-slim")
  ├── memory: Option<String>    内存限制 ("512m")
  ├── cpus: Option<f64>         CPU 限制 (1.0)
  ├── timeout_secs: u64         超时秒数 (default: 300)
  ├── network: bool             网络访问 (default: false)
  ├── writable: bool            可写文件系统 (default: false)
  └── env: HashMap<String,String>  环境变量

ExecRequest ─── 执行命令的请求
  ├── command: Vec<String>      命令和参数
  └── timeout_secs: Option<u64> 可选超时覆盖

ExecOutput ─── 命令执行结果
  ├── exit_code: i32            退出码
  ├── stdout: String            标准输出
  └── stderr: String            标准错误

SandboxInfo ─── sandbox 状态快照
  ├── id: SandboxId             唯一标识
  ├── status: SandboxStatus     Running / Stopped / Failed
  ├── provider: String          provider 名称
  └── image: String             使用的镜像

SandboxId = String              不透明的 sandbox 标识符
```

### 3.3 错误模型

```
ProviderError (thiserror)
  ├── NotFound(SandboxId)       sandbox 不存在
  ├── CreateFailed(String)      创建失败（镜像拉取、资源不足等）
  ├── ExecFailed(String)        执行失败（容器已停止等）
  ├── Unavailable(String)       provider 不可用（Docker 未安装等）
  └── Timeout(u64)              操作超时
```

---

## 4. 数据流

### 4.1 create 流程

```
用户: roche create --provider docker --memory 512m
  │
  ▼
CLI: 解析参数 → 构建 SandboxConfig (应用默认值)
  │
  ▼
CLI: 根据 config.provider 实例化 DockerProvider
  │
  ▼
DockerProvider::create(&config)
  │
  ├── 构建 docker create 命令
  │   ├── --memory 512m
  │   ├── --read-only (if !config.writable)
  │   ├── --network none (if !config.network)
  │   ├── --label roche.managed=true
  │   └── python:3.12-slim sleep infinity
  │
  ├── tokio::process::Command 执行 docker create
  │
  ├── 获取 container ID
  │
  ├── docker start <container_id>
  │
  └── 返回 Ok(SandboxId)
  │
  ▼
CLI: 输出 sandbox ID
```

### 4.2 exec 流程

```
用户: roche exec --sandbox <id> python3 -c "print('hello')"
  │
  ▼
CLI: 解析参数 → 构建 ExecRequest
  │
  ▼
DockerProvider::exec(&id, &request)
  │
  ├── 构建 docker exec 命令
  │   └── docker exec <id> python3 -c "print('hello')"
  │
  ├── 设置 timeout (exec 级别或 sandbox 级别)
  │
  ├── tokio::process::Command 执行
  │   ├── 捕获 stdout
  │   ├── 捕获 stderr
  │   └── 获取 exit code
  │
  └── 返回 Ok(ExecOutput { exit_code, stdout, stderr })
  │
  ▼
CLI: 输出 stdout/stderr + exit code
```

### 4.3 destroy 流程

```
用户: roche destroy <id>
  │
  ▼
DockerProvider::destroy(&id)
  │
  ├── docker stop <id>     (graceful stop)
  ├── docker rm -f <id>    (强制移除)
  └── 返回 Ok(())
  │
  ▼
CLI: 输出确认消息
```

### 4.4 list 流程

```
用户: roche list
  │
  ▼
DockerProvider::list()
  │
  ├── docker ps --filter label=roche.managed=true --format json
  ├── 解析 JSON 输出
  ├── 映射到 Vec<SandboxInfo>
  └── 返回 Ok(sandboxes)
  │
  ▼
CLI: 表格或 JSON 输出
```

---

## 5. Provider 架构

### 5.1 Provider 选择

MVP 阶段，CLI 根据 `--provider` 参数（或 `SandboxConfig.provider` 字段）选择 provider：

```rust
// CLI 中的 provider 路由 (伪代码)
let provider: Box<dyn SandboxProvider> = match config.provider.as_str() {
    "docker" => Box::new(DockerProvider::new()),
    // "firecracker" => Box::new(FirecrackerProvider::new()),  // Phase 2
    // "wasm" => Box::new(WasmProvider::new()),                // Phase 2
    _ => return Err("unsupported provider"),
};
```

### 5.2 Docker Provider 实现策略

MVP 使用 **Docker CLI** 而非 Docker Engine API：

| 方式 | 优点 | 缺点 |
|------|------|------|
| Docker CLI (`docker run`) | 零依赖、简单、可调试 | 进程开销、解析输出 |
| Docker Engine API (Unix socket) | 高效、结构化响应 | 需要 HTTP client + JSON 解析 |

**MVP 选择 Docker CLI**。理由：
- 简单可靠，适合 MVP 验证
- 用户机器上一定有 `docker` 命令
- 未来切换到 Unix socket API 只需改 provider 内部实现，trait 接口不变

### 5.3 容器标签 (Labels)

Docker provider 使用标签追踪 Roche 管理的容器：

```
roche.managed=true          标记为 Roche 管理
roche.sandbox_id=<id>       sandbox 唯一 ID
roche.created_at=<ts>       创建时间戳
```

`list` 操作通过 `--filter label=roche.managed=true` 只返回 Roche 管理的容器。

---

## 6. 安全架构

### 6.1 默认拒绝原则

Roche 的安全策略是 **deny by default**：

```
网络:     默认 --network none      → 需要时 --network 显式开启
文件系统:  默认 --read-only         → 需要时 --writable 显式开启
超时:     默认 300s                 → 无法禁用，只能调整
```

### 6.2 Docker 安全配置

DockerProvider 在 `create` 时应用的安全配置：

```bash
docker create \
  --network none \            # 禁用网络 (default)
  --read-only \               # 只读根文件系统 (default)
  --memory 512m \             # 内存限制
  --cpus 1.0 \                # CPU 限制
  --pids-limit 256 \          # 防止 fork bomb
  --security-opt no-new-privileges \  # 禁止提权
  --label roche.managed=true \
  python:3.12-slim
```

### 6.3 超时强制

两级超时机制：

1. **Sandbox 级别：** `SandboxConfig.timeout_secs`（默认 300s），sandbox 创建后超时自动销毁
2. **Exec 级别：** `ExecRequest.timeout_secs`，单次命令执行超时

超时通过 `tokio::time::timeout` 包裹 `docker exec` 调用实现，超时后发送 SIGKILL。

---

## 7. SDK 架构

### 7.1 Python SDK (MVP)

Python SDK 是 CLI 的 thin wrapper，通过 `subprocess` 调用 `roche` 二进制：

```python
class Roche:
    def create(self, config: SandboxConfig) -> str:
        """创建 sandbox，返回 ID"""

    def exec(self, sandbox_id: str, command: list[str]) -> ExecOutput:
        """在 sandbox 中执行命令"""

    def destroy(self, sandbox_id: str) -> None:
        """销毁 sandbox"""

    def list(self) -> list[SandboxInfo]:
        """列出活跃 sandbox"""
```

**Phase 2** 切换到 gRPC client，直连 daemon。

### 7.2 未来 SDK 架构 (Phase 2+)

Daemon 模式下，SDK 通过 gRPC 直连 `roched`：

```
SDK (Python/TS/Rust)
  │
  │ gRPC (protobuf)
  ▼
roched (daemon)
  │
  │ provider 内部实现
  ▼
Docker / Firecracker / WASM
```

---

## 8. 演进路径

### 8.1 MVP → Daemon 模式

```
MVP (当前):
  CLI (roche) ── 直接调用 ──> DockerProvider ──> Docker CLI

Phase 2 (daemon):
  CLI (roche) ── gRPC ──> roched ── 内部调用 ──> Provider ──> Runtime
  SDK          ── gRPC ──┘
```

Daemon 模式的优势：
- **Sandbox 池：** 预热容器减少冷启动
- **状态管理：** daemon 维护 sandbox 生命周期，支持断线恢复
- **并发控制：** 集中管理资源配额
- **多 SDK 支持：** gRPC 天然多语言

### 8.2 新增 Provider 的扩展点

添加新 provider 只需：

1. 在 `provider/` 下新建模块（如 `firecracker.rs`）
2. 实现 `SandboxProvider` trait
3. 在 CLI / daemon 中注册 provider 名称

**roche-core 的 trait 定义无需改动。** 这是 Provider 模式的核心价值。

---

## 9. 测试策略

| 层级 | 工具 | 覆盖范围 |
|------|------|----------|
| 单元测试 | `cargo test` | 类型构造、默认值、错误映射 |
| 集成测试 | `cargo test` + Docker | DockerProvider 端到端 |
| CLI 测试 | `assert_cmd` | CLI 参数解析、输出格式 |
| SDK 测试 | `pytest` | Python SDK 功能验证 |

集成测试需要 Docker daemon 运行。CI 中通过 GitHub Actions 的 Docker service 容器支持。
