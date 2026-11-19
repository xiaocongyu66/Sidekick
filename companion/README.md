# 暖灯（companion）— 情感陪伴手机 Agent

nsh（Native Shell）纯 Rust app，后端为**同进程内嵌的 [harn](../harn) agent
harness**：无 JS VM、无独立服务进程，iOS/Android/macOS/Windows/Linux/Web 同一套代码。

## 结构

- `src/lib.rs` — 聊天 UI（消息流 + 输入框），nsh 组件
- `src/store.rs` — 聊天状态：消息流/草稿/运行态信号 + 与后台的通道
- `src/bridge.rs` — 引擎线程 ↔ harn agent loop 桥接：harn 跑在 nsh 引擎自带的
  tokio runtime（`background`），命令/事件经 mpsc 通道跨线程
- `src/prompt.rs` — 「暖灯」情感陪伴人设（通过 `ConfigOverrides.system_prompt` 注入）
- `tests/flows.rs` — 无头冒烟测试（nsh-engine 跑真实 App，无平台窗口）

## 数据流

```
input 回调 ──store::submit──▶ ChatCommand::Send ──mpsc──▶ harn tokio loop
                                                              │ agent.send()
                                                              ▼
store::apply ◀──mpsc◀── AgentEvent{TextDelta,…}──┘
  └─▶ Signal<Vec<Message>> ──▶ virtual_list 重渲染
```

## 开发

桌面（GTK 4.14+ / macOS / Windows）：

```bash
cargo run                       # Linux 需 GTK 4.14+
```

无头测试（不需要任何平台窗口）：

```bash
cargo test
```

iOS / Android / Web（需 `nsh` CLI：<https://nsh.dev/docs/installation>）：

```bash
nsh run --target ios            # 或 android / web / ios-simulator
nsh package --target android    # 打包 apk
```

## 接入真实模型

默认无 API key 时走 harn 的离线 mock（回显）。接真实模型：

```bash
export OPENAI_API_KEY=sk-...          # 或 ANTHROPIC_API_KEY
export OPENAI_BASE_URL=...            # 可选，兼容网关
```

模型/provider 由 harn 的 `Config::resolve` 读取（env 与 settings 分层，
见 harn README）。
