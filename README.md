# Sidekick — 手机情感陪伴 Agent

「暖灯」：nsh（Native Shell）纯 Rust 手机 app，同进程内嵌 [harn](./harn) agent
harness 作为后端。一套 Rust 代码，iOS / Android / macOS / Windows / Linux / Web。

## 仓库结构

```
companion/   nsh app：聊天 UI + harn 桥接 + 「暖灯」人设
harn/        Rust coding-agent harness（裁剪版：无 TUI、无 Web，纯后端 + daemon）
bootstrap.sh 拉取 nsh 框架源码（companion 的 path 依赖，不进本仓库）
```

## 准备

```bash
./bootstrap.sh          # 克隆 nsh-src/（上游 longbridge/nsh）
cd companion && cargo test   # 无头冒烟（无需 GUI）
```

## 开发

桌面（macOS / Windows / Linux+GTK4.14+）：

```bash
cd companion && cargo run
```

手机与 Web（需 nsh CLI：<https://nsh.dev/docs/installation>）：

```bash
cd companion
nsh run --target android        # 或 ios / web / ios-simulator
nsh package --target android    # 打包
```

## 接入真实模型

默认无 API key 时走 harn 的离线 mock。接真实模型：

```bash
export OPENAI_API_KEY=sk-...    # 或 ANTHROPIC_API_KEY
```

人设在 `companion/src/prompt.rs`，经 `ConfigOverrides.system_prompt` 注入 harn。
