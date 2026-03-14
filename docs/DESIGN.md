# Roche — Design Details

**版本:** 0.1.0
**日期:** 2026-03-13

---

## 1. 设计哲学

### 1.1 Sandbox 是基础设施关注点

> "Linux kernel 管不管 Docker？不管。Castor daemon 也不应该管 sandbox。Sandbox 是基础设施关注点，不是 kernel 关注点。"

这一洞察来自 Castor 架构讨论，直接决定了 Roche 的定位：**独立的基础设施工具，不嵌入任何 agent 框架的内核。**

Roche 对 Castor（或任何 agent 框架）来说，只是一个 ATSP endpoint 背后的部署细节。Castord 看到的是 `localhost:9001` 上的 tool server，不知道也不关心它是跑在 Docker 里还是 Firecracker 里——那是 operator 用 Roche 做的部署决策。

### 1.2 两个正交的安全维度

```
访问控制 (Agent 框架的 Capability)
  "你能调什么 tool、花多少预算"
  逻辑层面的权限边界
  Agent kernel 执行

运行时隔离 (Roche Sandbox)
  "tool 代码能访问什么系统资源"
  物理层面的进程/容器/VM 边界
  Sandbox runtime 执行
```

两者缺一不可，但职责不同。Roche 只管第二个维度。

### 1.3 显式优于隐式

所有"危险"能力都要求显式 opt-in：

- 网络访问：必须传 `--network`
- 可写文件系统：必须传 `--writable`
- 没有"智能默认"——AI agent 场景下，安全比方便更重要

---

## 2. Docker Provider 详细设计

### 2.1 容器生命周期

```
                create()
                   │
                   ▼
   ┌───────────────────────────────┐
   │         docker create         │
   │  (应用安全配置 + 资源限制)      │
   └───────────────┬───────────────┘
                   │
                   ▼
   ┌───────────────────────────────┐
   │         docker start          │
   │       (启动容器)               │
   └───────────────┬───────────────┘
                   │
                   ▼
              ┌─────────┐
              │ Running │◄──────── exec() 可以执行
              └────┬────┘
                   │ destroy() 或 timeout
                   ▼
   ┌───────────────────────────────┐
   │     docker stop + docker rm   │
   │     (优雅停止 + 强制移除)       │
   └───────────────────────────────┘
```

### 2.2 docker create 命令构建

`DockerProvider::create()` 将 `SandboxConfig` 转换为 `docker create` 命令参数：

```rust
fn build_create_args(config: &SandboxConfig) -> Vec<String> {
    let mut args = vec!["create".to_string()];

    // 安全默认值
    if !config.network {
        args.extend(["--network".into(), "none".into()]);
    }
    if !config.writable {
        args.push("--read-only".into());
    }

    // 资源限制
    if let Some(ref memory) = config.memory {
        args.extend(["--memory".into(), memory.clone()]);
    }
    if let Some(cpus) = config.cpus {
        args.extend(["--cpus".into(), cpus.to_string()]);
    }

    // 安全加固
    args.extend([
        "--pids-limit".into(), "256".into(),           // 防止 fork bomb
        "--security-opt".into(), "no-new-privileges".into(), // 禁止提权
    ]);

    // Roche 管理标签
    args.extend([
        "--label".into(), "roche.managed=true".into(),
    ]);

    // 环境变量
    for (k, v) in &config.env {
        args.extend(["-e".into(), format!("{}={}", k, v)]);
    }

    // 镜像 + 保持运行的命令
    args.push(config.image.clone());
    args.extend(["sleep".into(), "infinity".into()]);

    args
}
```

**关键决策：**

- **`sleep infinity`：** 容器创建后保持运行，等待 `exec` 调用。这是"长期运行 sandbox"的模式——创建一次，执行多次。
- **`--pids-limit 256`：** 防止 fork bomb，256 是足够大多数合法工作负载的上限。
- **`--security-opt no-new-privileges`：** 防止容器内进程通过 setuid/setgid 提权。

### 2.3 docker exec 命令构建

```rust
fn build_exec_args(id: &SandboxId, request: &ExecRequest) -> Vec<String> {
    let mut args = vec!["exec".to_string()];

    // 容器 ID
    args.push(id.clone());

    // 用户命令
    args.extend(request.command.clone());

    args
}
```

### 2.4 超时实现

```rust
async fn exec_with_timeout(
    id: &SandboxId,
    request: &ExecRequest,
    sandbox_timeout: u64,
) -> Result<ExecOutput, ProviderError> {
    let timeout = request.timeout_secs.unwrap_or(sandbox_timeout);

    let result = tokio::time::timeout(
        Duration::from_secs(timeout),
        run_docker_exec(id, request),
    ).await;

    match result {
        Ok(inner) => inner,
        Err(_) => Err(ProviderError::Timeout(timeout)),
    }
}
```

**超时后行为：** `tokio::time::timeout` 会 drop 内部 future，进而 drop `tokio::process::Child`。需要确保在 drop 时向 Docker 进程发送 SIGKILL，否则 `docker exec` 可能继续在后台运行。

### 2.5 容器 ID 管理

Docker 返回的 container ID 是 64 位十六进制字符串。Roche 截取前 12 位作为 `SandboxId`（与 `docker ps` 默认显示一致）。

```rust
fn parse_container_id(output: &str) -> SandboxId {
    output.trim().chars().take(12).collect()
}
```

### 2.6 list 实现

```bash
docker ps \
  --filter label=roche.managed=true \
  --format '{{json .}}'
```

解析 JSON 输出，映射到 `SandboxInfo`：

| Docker 字段 | Roche 字段 |
|-------------|------------|
| `.ID` | `id` |
| `.State` | `status` (映射到 `SandboxStatus`) |
| `.Image` | `image` |
| — | `provider` = "docker" (硬编码) |

状态映射：

```
"running"  → SandboxStatus::Running
"exited"   → SandboxStatus::Stopped
其他       → SandboxStatus::Failed
```

### 2.7 错误处理

| 场景 | Docker 行为 | Roche 映射 |
|------|-------------|------------|
| Docker 未安装 | `docker` 命令不存在 | `ProviderError::Unavailable` |
| Docker daemon 未运行 | 连接拒绝 | `ProviderError::Unavailable` |
| 镜像不存在 | `docker create` 失败 | `ProviderError::CreateFailed` |
| 容器不存在 | `docker exec` 失败 | `ProviderError::NotFound` |
| 命令执行超时 | tokio timeout 触发 | `ProviderError::Timeout` |
| 容器已停止 | `docker exec` 失败 | `ProviderError::ExecFailed` |

检测 Docker 是否可用：

```rust
async fn check_docker_available() -> Result<(), ProviderError> {
    let output = Command::new("docker")
        .arg("info")
        .output()
        .await
        .map_err(|_| ProviderError::Unavailable(
            "Docker is not installed or not in PATH".into()
        ))?;

    if !output.status.success() {
        return Err(ProviderError::Unavailable(
            "Docker daemon is not running".into()
        ));
    }
    Ok(())
}
```

---

## 3. CLI 详细设计

### 3.1 子命令结构

```
roche
├── create    创建 sandbox
│   ├── --provider <name>      (default: "docker")
│   ├── --image <image>        (default: "python:3.12-slim")
│   ├── --memory <limit>       (e.g., "512m")
│   ├── --cpus <count>         (e.g., 1.0)
│   ├── --timeout <secs>       (default: 300)
│   ├── --network              (flag, default: off)
│   ├── --writable             (flag, default: off)
│   └── --env <K=V>            (repeatable)
│
├── exec      在 sandbox 中执行命令
│   ├── --sandbox <id>         (required)
│   ├── --timeout <secs>       (optional, override)
│   └── <command...>           (positional, trailing)
│
├── destroy   销毁 sandbox
│   └── <id>                   (positional)
│
└── list      列出活跃 sandbox
    └── --json                 (flag, JSON 输出)
```

### 3.2 输出格式

**create：** 只输出 sandbox ID（便于脚本捕获）

```
$ roche create --memory 512m
abc123def456
```

**exec：** 输出 stdout，stderr 输出到 stderr，退出码作为进程退出码

```
$ roche exec --sandbox abc123def456 python3 -c "print('hello')"
hello
$ echo $?
0
```

**destroy：** 静默成功，失败时输出错误

```
$ roche destroy abc123def456
$ echo $?
0
```

**list：** 表格格式（默认）或 JSON（`--json`）

```
$ roche list
ID              STATUS    PROVIDER  IMAGE
abc123def456    running   docker    python:3.12-slim
def789abc012    running   docker    node:20-slim

$ roche list --json
[{"id":"abc123def456","status":"running","provider":"docker","image":"python:3.12-slim"}]
```

### 3.3 Provider 路由

CLI 中通过 match 实例化 provider，职责边界清晰：

```rust
async fn get_provider(name: &str) -> Result<Box<dyn SandboxProvider>, ProviderError> {
    match name {
        "docker" => {
            let provider = DockerProvider::new();
            // 提前检查 Docker 是否可用，给出清晰错误
            provider.check_available().await?;
            Ok(Box::new(provider))
        }
        other => Err(ProviderError::Unavailable(
            format!("Unknown provider: {}. Available: docker", other)
        )),
    }
}
```

---

## 4. Python SDK 设计

### 4.1 MVP 架构：CLI Wrapper

MVP 阶段的 Python SDK 是 `roche` CLI 的 subprocess wrapper：

```python
import subprocess
import json
from dataclasses import dataclass

@dataclass
class SandboxConfig:
    provider: str = "docker"
    image: str = "python:3.12-slim"
    memory: str | None = None
    cpus: float | None = None
    timeout: int = 300
    network: bool = False
    writable: bool = False
    env: dict[str, str] | None = None

@dataclass
class ExecOutput:
    exit_code: int
    stdout: str
    stderr: str

class Roche:
    def __init__(self, binary: str = "roche"):
        self._binary = binary

    def create(self, config: SandboxConfig | None = None) -> str:
        """创建 sandbox，返回 sandbox ID。"""
        config = config or SandboxConfig()
        cmd = [self._binary, "create",
               "--provider", config.provider,
               "--image", config.image,
               "--timeout", str(config.timeout)]

        if config.memory:
            cmd.extend(["--memory", config.memory])
        if config.cpus:
            cmd.extend(["--cpus", str(config.cpus)])
        if config.network:
            cmd.append("--network")
        if config.writable:
            cmd.append("--writable")

        result = subprocess.run(cmd, capture_output=True, text=True, check=True)
        return result.stdout.strip()

    def exec(self, sandbox_id: str, command: list[str],
             timeout: int | None = None) -> ExecOutput:
        """在 sandbox 中执行命令。"""
        cmd = [self._binary, "exec", "--sandbox", sandbox_id]
        if timeout:
            cmd.extend(["--timeout", str(timeout)])
        cmd.extend(command)

        result = subprocess.run(cmd, capture_output=True, text=True)
        return ExecOutput(
            exit_code=result.returncode,
            stdout=result.stdout,
            stderr=result.stderr,
        )

    def destroy(self, sandbox_id: str) -> None:
        """销毁 sandbox。"""
        subprocess.run(
            [self._binary, "destroy", sandbox_id],
            capture_output=True, text=True, check=True,
        )

    def list(self) -> list[dict]:
        """列出活跃 sandbox。"""
        result = subprocess.run(
            [self._binary, "list", "--json"],
            capture_output=True, text=True, check=True,
        )
        return json.loads(result.stdout)
```

### 4.2 Context Manager 支持

```python
class Sandbox:
    """单个 sandbox 的上下文管理器。自动创建和销毁。"""

    def __init__(self, client: Roche, config: SandboxConfig | None = None):
        self._client = client
        self._config = config or SandboxConfig()
        self._id: str | None = None

    async def __aenter__(self) -> "Sandbox":
        self._id = self._client.create(self._config)
        return self

    async def __aexit__(self, *exc) -> None:
        if self._id:
            self._client.destroy(self._id)

    def exec(self, command: list[str], **kwargs) -> ExecOutput:
        return self._client.exec(self._id, command, **kwargs)
```

使用示例：

```python
roche = Roche()
config = SandboxConfig(memory="256m")

async with Sandbox(roche, config) as sb:
    result = sb.exec(["python3", "-c", "print(2+2)"])
    print(result.stdout)  # "4\n"
# sandbox 自动销毁
```

### 4.3 演进到 gRPC (Phase 2)

Phase 2 的 SDK 将直连 daemon：

```python
class Roche:
    def __init__(self, endpoint: str = "unix:///var/run/roched.sock"):
        self._channel = grpc.aio.insecure_channel(endpoint)
        self._stub = roche_pb2_grpc.RocheServiceStub(self._channel)
```

接口签名保持不变，用户代码无需修改。

---

## 5. 标签与追踪系统

### 5.1 标签规范

所有 Roche 管理的 Docker 容器携带以下标签：

| 标签 | 值 | 用途 |
|------|-----|------|
| `roche.managed` | `true` | 标识 Roche 管理的容器 |
| `roche.sandbox_id` | `<12-char-hex>` | Sandbox 唯一 ID |
| `roche.created_at` | `<ISO 8601>` | 创建时间戳 |
| `roche.provider` | `docker` | Provider 名称 |
| `roche.image` | `python:3.12-slim` | 使用的镜像 |
| `roche.network` | `true` / `false` | 网络是否启用 |
| `roche.writable` | `true` / `false` | 文件系统是否可写 |

### 5.2 孤儿容器清理

异常退出（进程崩溃、SIGKILL）可能留下孤儿容器。解决方案：

1. **`roche list`** 可以发现所有 `roche.managed=true` 的容器
2. **`roche destroy <id>`** 手动清理
3. **未来 (daemon 模式)：** daemon 启动时自动清理超时的孤儿容器

---

## 6. 未来 Provider 设计预览

### 6.1 Firecracker Provider (Phase 2)

```rust
pub struct FirecrackerProvider {
    socket_path: PathBuf,    // Firecracker API socket
    kernel_image: PathBuf,   // Linux kernel image
    rootfs_path: PathBuf,    // Root filesystem image
}
```

Firecracker 提供 microVM 级别隔离——独立 Linux 内核 + 独立用户空间。启动时间 ~125ms，比 Docker 更强的安全隔离。

### 6.2 WASM Provider (Phase 2)

```rust
pub struct WasmProvider {
    engine: wasmtime::Engine,
}
```

WASM 提供最轻量的隔离——无文件系统、无网络、线性内存。适合纯计算任务。启动时间 ~1ms。

### 6.3 Provider 选择指南

| 需求 | 推荐 Provider |
|------|--------------|
| 通用代码执行 | Docker |
| 高安全隔离 | Firecracker |
| 纯计算、极低延迟 | WASM |
| 已有 E2B 基础设施 | E2B (Phase 3) |
| K8s 集群可用 | Kubernetes (Phase 3) |

---

## 7. 设计决策记录

| # | 决策 | 备选方案 | 理由 |
|---|------|----------|------|
| D1 | Docker CLI 而非 Engine API | Unix socket HTTP | MVP 简单性，无额外依赖 |
| D2 | `sleep infinity` 保持容器运行 | 每次 exec 创建新容器 | 复用容器减少延迟，支持多次 exec |
| D3 | 截取前 12 位作为 ID | 使用完整 64 位 | 与 docker ps 一致，人类可读 |
| D4 | Python SDK 作为 CLI wrapper | FFI / PyO3 绑定 | MVP 最简路径，Phase 2 切 gRPC |
| D5 | 不嵌入 agent 框架 | 作为 Castor 子模块 | 独立工具，框架无关，更广泛复用 |
| D6 | 默认禁用网络和可写 FS | 默认开放，按需限制 | AI 安全场景 deny-by-default |
| D7 | `--pids-limit 256` | 无限制 | 防止 fork bomb，256 覆盖大多数合法场景 |
| D8 | 标签追踪 | 外部状态文件 | Docker 原生，容器删除标签自动清理 |
