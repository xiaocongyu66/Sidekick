# harn — Rust Coding-Agent Harness

[![CI](https://github.com/xiaocongyu66/harn/actions/workflows/ci.yml/badge.svg)](https://github.com/xiaocongyu66/harn/actions/workflows/ci.yml)

一个用 Rust 实现的 coding-agent harness：交互 REPL、无头模式、流式工具调用、
六种权限模式、多模态输入、子代理编排、Plan 模式、命令钩子与会话恢复，
以及常驻 daemon（UDS 单服务器多客户端，供前端接入）。

设计参考：

- Claude Code 反编译源码（`/root/xiaocongyu66-claude-code`）
- 《御舆：解码 Agent Harness》`lintsinghua/claude-code-book`
- 《Claude Code 实战：Harness 工程之道》的工程视角（工具、知识、观察、行动、权限）

## 快速开始

```bash
cargo build --release
./target/release/harn                 # REPL（无 API key 时自动使用离线 mock）
./target/release/harn -p "hello"      # 无头模式
./target/release/harn server start    # 常驻 daemon（UDS，供前端接入）
```

离线 mock 会回显最后一条用户输入；用脚本可以驱动完整的工具调用：

```json
{
  "turns": [
    { "text": "write it", "tool_use": [
      { "name": "Write", "input": { "file_path": "hello.txt", "content": "hi\n" } }
    ]},
    { "text": "done" }
  ]
}
```

```bash
HARN_MOCK_SCRIPT=$PWD/script.json harn -p "create hello.txt" --permission-mode acceptEdits
```

## 接入真实模型

### Anthropic Messages API

```bash
export ANTHROPIC_API_KEY=sk-ant-...
export ANTHROPIC_MODEL=claude-sonnet-4-6       # 可选
export ANTHROPIC_BASE_URL=https://api.anthropic.com  # 可选，兼容网关
harn -p "explain this repo"
```

### OpenAI Chat Completions

```bash
export OPENAI_API_KEY=sk-...
export OPENAI_BASE_URL=https://api.openai.com/v1   # 任意兼容网关
harn --provider openai --openai-api chat -p "hello"
```

### OpenAI Responses API

```bash
harn --provider openai --openai-api responses -p "hello"
```

两者共享内部消息模型，但使用各自的原生线格式：

| 内部概念 | Chat Completions | Responses API |
|---|---|---|
| system | `role=system` | `instructions` |
| tool_use | `assistant.tool_calls` | `function_call` item |
| tool_result | `role=tool` + `tool_call_id` | `function_call_output` |
| 流式文本 | `delta.content` | `response.output_text.delta` |
| 工具参数流 | `delta.tool_calls[].function.arguments` | `response.function_call_arguments.delta` |
| usage | 末尾 chunk `usage` | `response.completed.response.usage` |

也可以完全离线测试 HTTP 路径：`crates/harn-providers/tests/http.rs` 用本地 SSE 服务器
验证了两种协议的请求体、流式解析、工具调用与 usage。

## 多模态

```bash
harn -p "what is in this screenshot?" --image shot.png
```

REPL 中：

```text
/image screenshot.png
> describe it
```

支持 png/jpeg/gif/webp，单图上限 5MB。Anthropic 使用 `image` base64 块；
OpenAI Chat 使用 `image_url` data URL；Responses 使用 `input_image` + `detail: auto`。

## 命令行

```text
harn [PROMPT] [OPTIONS]
  -p, --print                 无头模式（提示词可来自 stdin）
  -m, --model <MODEL>
      --provider <NAME>       anthropic | openai | mock
      --openai-api <API>      chat | responses
      --base-url <URL>
      --permission-mode <M>   default | acceptEdits | plan | auto | bypassPermissions | dontAsk
      --dangerously-skip-permissions
  -c, --continue              恢复最近会话
      --resume <ID>           恢复指定会话（支持前缀）
      --max-turns <N>
      --max-tokens <N>
  -y, --yes                   自动批准权限询问与计划审批
      --image <PATH>          可重复
      --coordinator           主代理只编排、不直接执行
  -v, --verbose

harn sessions list|show <ID>
harn config show|path
harn doctor
harn agents
```

斜杠命令：`/help /status /model /permissions /plan /auto /bypass /default
/accept-edits /dont-ask /cost /clear /compact /image /agents /sessions /exit`。

`/permissions` 管理权限规则（对齐 Claude Code 的 PermissionUpdate）：

```text
/permissions list                      列出各层生效规则（含来源）与遮蔽警告
/permissions allow "Bash(cargo test:*)"  写入项目本地 settings（默认）
/permissions allow "Read(src/**)" --user 写入用户 settings
/permissions deny  "Bash(rm:*)" --policy 之外的层由 --user/--local/--session 选择
/permissions remove allow "Read(src/**)"
/permissions mode auto --local          设置默认模式
```

文件层更新即时重载生效；daemon 协议弹窗的「始终允许并写入本地设置」与之同一机制。

## 权限模型

优先级：`deny > ask > allow > 模式默认`；`PreToolUse` 钩子可在 deny/plan 约束之后
覆盖 ask/allow。

| 模式 | 只读 | 工作区写 | 安全构建/测试 | 普通变更 | 危险命令 | 未匹配 |
|---|---|---|---|---|---|---|
| default | allow | ask | ask | ask | deny | ask |
| acceptEdits | allow | allow | ask | ask | deny | ask |
| plan | allow | deny | deny | deny | deny | deny（Plan 工具除外） |
| auto | allow | allow | allow | ask | deny | ask |
| bypassPermissions | allow | allow | allow | allow | allow（显式 deny 除外） | allow |
| dontAsk | allow | deny | deny | deny | deny | deny |

规则示例（`~/.harn/settings.json` 或 `<project>/.harn/settings.json`）：

```json
{
  "permissions": {
    "defaultMode": "auto",
    "allow": ["Read", "Bash(git status:*)", "Edit(src/**)"],
    "ask": ["Bash(cargo publish:*)"],
    "deny": ["Bash(rm -rf:*)", "Bash(sudo:*)"]
  }
}
```

- `Bash(git status)`：精确匹配。
- `Bash(cargo test:*)`：前缀匹配。
- `Read(src/**)` / `Edit(src/**)`：路径 glob。
- 规则内容里的括号需转义：`Bash(python -c "print\(1\)")`。
- **复合命令按子命令匹配**：`git status && rm -rf build` 会拆成 `git status` 与
  `rm -rf build` 两个子命令——allow 规则必须覆盖**每一个**子命令才放行，任一子命令
  命中 deny 规则即拒绝（堵住单条前缀规则顺带放行危险命令的缺口）。
- 子代理继承父级模式与规则，只能通过工具白名单收窄，不能提权。

**规则来源分层**（与 Claude Code 的 `PermissionRuleSource` 对齐）。每条规则按来源参与
匹配与遮蔽检测，优先级从高到低：

| 层 | 文件 |
|---|---|
| `policySettings` | `/etc/harn/managed-settings.json`（或 `$HARN_POLICY_FILE` 指定），企业管控、只读 |
| `userSettings` | `~/.harn/settings.json` |
| `projectSettings` | `<project>/.harn/settings.json` |
| `localSettings` | `<project>/.harn/settings.local.json` |

- policy 层的 deny 压过所有低层 allow；会话内工具（`AllowAlways`）只在 `session` 层记忆。
- 加载期检测**规则遮蔽**（高层宽规则让低层窄规则永不生效），以
  `WARN: 权限规则 ... 被 ... 遮蔽` 打印（`harn doctor` 会显示）。
- 每次放行/拒绝都带**结构化决策理由**（命中规则[来源] / 模式默认 / 钩子干预 /
  危险命令 / plan 限制 / 用户选择），拒绝时直接展示给模型与用户，例如
  `⊘ Bash git status && rm -rf build denied — 命中 deny 规则 Bash(rm:*) [projectSettings]`。

## 工具

| 工具 | 说明 |
|---|---|
| `Bash` | `bash -c`，默认 120s 超时，进程组 SIGKILL，ANSI 清理，>30k 字符落盘 |
| `Read` | 行号读取，二进制拒绝，offset/limit |
| `Write` | 创建/覆盖，自动建父目录 |
| `Edit` | 精确替换；重复匹配需 `replace_all` |
| `Glob` | 移植的 bfs 语义文件发现（mtime 排序） |
| `Grep` | 移植的进程内 ripgrep（content/files/count、上下文、glob/type） |
| `Agent` | 子代理（general/explore/plan/verify，可后台；`isolation=worktree` 让 general 子代理在独立 git worktree 里干活，改动统计与合并命令随结果返回） |
| `AgentList` / `AgentStop` | 主代理查看/取消子代理 |
| `Team` | 并行运行多个子代理（默认并发 3，上限 8） |
| `TaskCreate` / `TaskUpdate` / `TaskList` | 会话级任务清单（○ pending / ▶ in_progress / ✓ completed，上限 64） |
| `TodoWrite` | 整体替换式清单（对齐 Claude Code TodoWrite：content/status/activeForm/blockedBy 一次写入） |
| `AskUserQuestion` | 通过弹窗或 REPL 选择器向用户提问；无交互界面时明确告知模型自行决策 |
| `WebFetch` | 抓取 http(s) 页面转纯文本（SSRF 防护：拒绝内网/环回；仅文本类内容） |
| `NotebookEdit` | 编辑 `.ipynb` cell（replace/insert/delete，按 id 或索引） |
| `lsp` | 语言服务器查询：goToDefinition / findReferences / hover / documentSymbol / workspaceSymbol / goToImplementation / prepareCallHierarchy / incomingCalls / outgoingCalls |
| `EnterWorktree` / `ExitWorktree` / `MergeWorktree` | 会话级 git 工作区：`项目/.harn/worktrees/<slug>` 隔离副本；退出给 keep/remove 与合并命令（删除有改动时 fail-closed）；MergeWorktree 把提交 cherry-pick/merge 进主工作树当前分支（dry-run、冲突回退、主树脏拒绝） |
| `EnterPlanMode` / `ExitPlanMode` | 计划模式与审批 |

工具执行使用**并发分区**：连续的并发安全工具成批并行，非安全工具严格串行，
结果顺序与请求顺序一致。Bash 超时会终止整个进程组，避免孤儿进程。

`Edit` / `Write` 成功后附带 **bat 风格 ANSI 着色 diff**（`harn-tools::diff`，移植自 Claude Code
`color-diff-napi` 的纯算法部分）：jsdiff 兼容的行/词 Myers diff + `structured_patch` hunks，
删除行红底、新增行绿底、词级高亮、行号对齐。

性能设计：Agent 在追加消息时维持「连续同角色已合并」的归一化不变量，消息序列以
`Arc<Vec<Message>>` 在各回合间零拷贝共享（不再每轮深拷贝）；provider 只做线格式映射。
工作区 clippy 启用 `unwrap_used` / `expect_used`（测试内豁免），生产路径的错误一律沿
`Result` 传播，长跑会话不会因单个假设落空而 panic。

`Grep`/`Glob` 直接复用 Claude Code 仓库中的 `packages/search-napi/native`
内核（去除 N-API 绑定），保留 grep-regex/grep-searcher/PCRE2 降级链、bfs 遍历、
隐藏文件与 gitignore 语义。

### 对话渲染（Markdown）

REPL 的对话输出是 Markdown 渲染的（移植 Claude Code `src/utils/markdown.ts` 的
终端呈现）：标题（粗体/下划线）、粗体/斜体/行内码（蓝）、列表（`•`，有序按深度
数字→字母→罗马）、引用（`▎` 前缀斜体）、代码块（syntect 语法高亮 + 语言标签）、
表格（box-drawing 边框，超宽降 key-value 竖排）、分割线。行内样式与高亮由
`harn_ui::markdown`/`highlight` 纯文本投影输出。流式期间每个 delta 后全量重解析
（未闭合 fence 自然渲染到文尾）。diff（Edit/Write）输出标记化文本（行首 `+`/`-`
+ `[n]` gutter 行号）。

### 工作区（子代理隔离）

主代理派发子代理有三种工作方式：默认共享工作树；`Agent` 加 `isolation:"worktree"`
（可配 `worktreeName`）则在**项目内** `项目/.harn/worktrees/<slug>` 建一个 git worktree
（分支 `worktree-<slug>`，默认基于当前 HEAD），子代理的全部读写只落在该副本里，
主工作树不受影响。只读子代理拒绝该参数（无写动作，建了纯浪费）。子代理结束时：
零改动 → 工作区自动删除；有改动 → 保留，并把改动计数与 git 合并命令
（`git -C <项目> cherry-pick HEAD..worktree-<slug>` 或 `merge`）随结果返回主代理——
合并与否由你/主代理执行，harn 不自动改仓库。

主代理自己也能切换：`EnterWorktree [name]` 进入、`ExitWorktree {action:"keep"|"remove"}`
退出（remove 遇未提交/未合并改动会拒绝，需显式 `discardChanges:true`）。会话结束时
汇总所有存活工作区。非 git 仓库会得到明确报错（建议 `git init`），不静默降级。

**合并回主项目**（`MergeWorktree`）：`mode=cherry-pick`（默认）把工作区的提交线性
重放进主工作树当前分支；`mode=merge` 生成一个 `--no-ff` 合并提交。`dryRun=true`
先列出将进入的提交与前置检查（不碰任何状态）。冲突时自动 `--abort` 回退，
绝不在主树留下半合并状态；主工作树有未提交改动时拒绝合并（保护在途工作）。
注意合并只带入**已提交**内容——worktree 里未提交的改动仍留在原地，先 commit
或 `ExitWorktree keep` 查看。`.harn/worktrees/` 已写入主仓 `info/exclude`，
用户的 `git add .` 不会把工作区当嵌入式仓库收进去。
settings 可选配置（默认 HEAD 基线、全量检出）：

```json
{"worktree": {"baseBranch": "main", "symlinkDirectories": ["node_modules"], "sparsePaths": ["src"]}}
```

### 语言服务器（LSP）

`lsp` 工具把外部语言服务器接进 Agent 循环（移植 Claude Code `src/services/lsp` 的
交互模型：同样的九个操作与 1-based 行列入参）。服务器从 settings 的 `lspServers` 段
配置——harn 只 spawn 用户自己安装的命令，不代装、不代管：

```json
{"lspServers": {"rust-analyzer": {
  "command": "rust-analyzer", "args": [], "filetypes": ["rs"], "rootMarker": "Cargo.toml"
}}}
```

`rootMarker` 向上寻找工作区根（`.` 表示当前目录），`filetypes` 决定路由。
相对 Claude Code 的三处更新：`lsp-types` 0.97（LSP 3.17）替代其 vscode-jsonrpc 绑定；
**位置编码协商**（服务器声明 utf-8 时按字符列，否则回退 LSP 默认的 UTF-16；
CC 缺失此项，多字节字符旁跳转会偏移）；配置经 settings 六层分层（不限于插件通道）。
工具自动补 `didOpen`（CC 需要模型先读文件），诊断经 `publishDiagnostics` 收集，
编辑后如有 error 级问题会随工具结果反馈。

## 子代理与多 Agent

- `general`：可读写；可再派生子代理（深度上限默认 2）。
- `explore`：只读探索。
- `plan`：只读规划。
- `verify`：对抗性验证。
- 子代理拥有独立上下文与 Provider 实例，结束后只向父代理返回摘要。
- 主代理通过 `AgentList` 维护子代理列表，通过 `AgentStop` 取消后台任务。
- `Team` 使用 `buffer_unordered` 并行执行，聚合结果。
- `--coordinator`：主代理只保留 `Agent / AgentList / AgentStop / Team / ExitPlanMode`，
  不直接读写，把所有执行交给 workers。

## Plan 模式

```bash
harn --permission-mode plan -p "design the pagination feature"
```

- 只读工具可用；写入与变更命令被拒绝。
- 代理调用 `ExitPlanMode` 提交计划。
- 计划写入会话目录下的 `plans/plan-<uuid>.md`。
- 批准后恢复进入 Plan 前的模式（默认 `acceptEdits`）；拒绝则继续规划。
- 子代理禁止进入 Plan 模式。

## 钩子

```json
{
  "hooks": {
    "PreToolUse": [
      { "matcher": "Bash", "hooks": [ { "type": "command", "command": "bash ./check.sh" } ] }
    ],
    "UserPromptSubmit": [
      { "hooks": [ { "type": "command", "command": "bash ./inject-context.sh" } ] }
    ]
  }
}
```

事件：`PreToolUse`、`PostToolUse`、`PostToolUseFailure`、`UserPromptSubmit`、
`SessionStart`、`SessionEnd`、`Stop`。

- stdin：JSON（event、cwd、session_id、agent_id、tool_name、tool_input、prompt）。
- 退出码 2：阻断，stderr 作为原因。
- stdout JSON：`{"decision":"block","reason":"..."}` 或
  `{"hookSpecificOutput":{"permissionDecision":"allow|deny|ask","additionalContext":"..."}}`。
- 默认超时 60s，可配置 `timeout`。

## 会话

- JSONL 转录：`~/.harn/projects/<cwd-hash>/<session-id>.jsonl`（可用 `HARN_HOME` 覆盖）。
- 记录 meta、消息、usage 与备注。
- `--continue` 恢复最近会话，`--resume <id>` 指定会话。
- `harn sessions list` / `harn sessions show <id>` 查看。
- 工具大输出保存在 `<session-dir>/tool-results/`。
- 子代理状态落盘在 `<session-dir>/agents/<id>/status.json`；重新接入会话时回填展示，
  上次会话未收尾的 `running` 记录按中断处理（标为 cancelled 并注明「上次会话中断」）。

## 架构（Cargo workspace）

```
harn-core        消息/事件/取消/Provider 契约/子代理契约
harn-config      分层 settings、Provider/模型/路径解析
harn-search      search-napi 移植内核（grep/find/bfs/sanitize/heart）
harn-permission  规则、Bash 风险分类、权限策略、交互提示
harn-providers   Anthropic / OpenAI Chat / OpenAI Responses / Mock
harn-tools       Tool trait、注册表、内置工具（含 diff 内核：jsdiff Myers + bat ANSI 着色、LSP 客户端）
harn-hooks       命令钩子
harn-session     JSONL 转录
harn-agent       主循环、并发分区、子代理管理器、系统提示
harn-ui          纯文本流式渲染
harn-daemon      常驻 Runtime、UDS 多客户端、共享协议 DTO
harn-cli         clap、REPL、子命令、server 管理、二进制
```

依赖方向单向：`harn-cli → harn-agent → {tools, permission, providers, ...}`，
共享类型下沉到 `harn-core`，不存在循环依赖。

## 安全边界

- 路径规则默认限定工作区；越界触发 ask/deny。
- 危险命令（`rm -rf /`、`mkfs`、`dd of=/dev/*`、管道执行远程脚本、force push 等）
  在非 bypass 模式下拒绝。
- `bypassPermissions` 只跳过交互确认，显式 deny 规则仍然生效。
- 子代理深度上限默认 2，只读类型在注册表中物理移除写工具。
- 钩子有超时与结构校验；非 JSON stdout 不参与决策。
- API key 只从环境变量读取，不写入转录。
## 开发与验证

本地三件套与 CI 完全一致（CI 即 GitHub Actions 两 job 并行：lint=fmt+clippy
`-D warnings`，test=`cargo test --workspace`，均带 swatinem/rust-cache 缓存）：

```bash
cargo fmt --all -- --check
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

**本机推送**：Android/Termux 沙箱对 github.com 的 git 传输通道几乎不可用
（上传速度 <1KB/s，`git push` 必然超时），但 `api.github.com` 正常。推送方式：
`gh repo create --public --source=.` 建仓后，用 GitHub Git Data API 逐对象上传
（`git ls-files` → blob → 目录 tree → commit → `PATCH git/refs/heads/main`）——
脚本要点见 `scripts/` 说明或 `PLAN.md`；增量推送同样只传改动的 blob。

测试覆盖：

- 移植搜索内核 71 项（grep/find/bfs）。
- Provider SSE 解析 + 本地 HTTP 端到端（Chat / Responses）。
- 权限矩阵、Bash 风险分类、规则匹配、路径归一化。
- CLI 集成 17 项：无头模式、工具循环、plan 拒绝/批准、子代理/Team/后台列表、
  会话恢复、钩子阻断与 allow 覆盖、Bash 超时/大输出落盘、危险命令拒绝。
- 子代理后台列表与取消单元测试。


## 许可证

本项目采用 [Blue Oak Model License 1.0.0](LICENSE)（宽松许可证：允许使用/修改/分发，
无 copyleft；要求保留许可声明；无专利/商标担保之外的明示担保）。英文原文见 `LICENSE`。
