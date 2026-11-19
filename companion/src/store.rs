//! 聊天状态：消息流、草稿、运行态，与后台 agent 的命令通道。
//!
//! 全局状态为每个 surface 独立一份（`global::<ChatState>()`）。
//! UI 绑定经内部信号订阅（`messages()`/`draft()`/`busy()`）；
//! `to_agent` 是跨线程通道，把命令送到 harn 的 tokio 任务，
//! 事件经 `apply` 回到引擎线程写信号。

use nsh_core::element::SharedString;
use nsh_core::prelude::*;

/// 一条消息的角色。
#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum Role {
    User,
    Assistant,
    Info,
}

/// 一条聊天气泡。
#[derive(Clone, PartialEq)]
pub(crate) struct Message {
    pub(crate) role: Role,
    pub(crate) text: SharedString,
}

/// UI → 后台 agent 的命令。
pub(crate) enum ChatCommand {
    Send(SharedString),
}

/// 后台 → UI 的事件。
#[derive(Clone, PartialEq)]
pub(crate) enum BridgeEvent {
    Started {
        model: SharedString,
    },
    Delta(SharedString),
    Info(SharedString),
    Error(SharedString),
    Done,
}

#[derive(Clone)]
struct ChatState {
    messages: Signal<Vec<Message>>,
    draft: Signal<SharedString>,
    busy: Signal<bool>,
    to_agent: Option<tokio::sync::mpsc::UnboundedSender<ChatCommand>>,
}

impl Default for ChatState {
    fn default() -> Self {
        Self {
            messages: signal(vec![Message {
                role: Role::Assistant,
                text: "我在呢。今天想聊点什么？".into(),
            }]),
            draft: signal(SharedString::default()),
            busy: signal(false),
            to_agent: None,
        }
    }
}

fn state() -> Signal<ChatState> {
    global::<ChatState>()
}

fn messages() -> Signal<Vec<Message>> {
    state().with_untracked(|state| state.messages)
}

fn draft() -> Signal<SharedString> {
    state().with_untracked(|state| state.draft)
}

fn busy() -> Signal<bool> {
    state().with_untracked(|state| state.busy)
}

/// 注入后台通道（bridge 在引擎线程建好后调用）。
pub(crate) fn set_agent_channel(sender: tokio::sync::mpsc::UnboundedSender<ChatCommand>) {
    state().update(|state| {
        state.to_agent = Some(sender);
    });
}

/// 绑定：消息条数（virtual_list 计数）。
pub(crate) fn message_count() -> usize {
    messages().with(|messages| messages.len())
}

/// 绑定：消息稳定键。
pub(crate) fn message_ids() -> Vec<usize> {
    messages().with(|messages| (0..messages.len()).collect())
}

/// 绑定：一条消息。
pub(crate) fn message_at(index: usize) -> Option<Message> {
    messages().with(|messages| messages.get(index).cloned())
}

/// 绑定：草稿。
pub(crate) fn draft_text() -> SharedString {
    draft().get()
}

/// 设置草稿（输入框回调）。
pub(crate) fn set_draft(text: SharedString) {
    draft().set(text);
}

/// 绑定：是否正在等待回复。
pub(crate) fn is_busy() -> bool {
    busy().get()
}

/// 提交草稿：本地先入流，再送后台。
pub(crate) fn submit() {
    let text = draft().get_untracked();
    if text.trim().is_empty() {
        return;
    }
    draft().set(SharedString::default());
    messages().update(|messages| messages.push(Message {
        role: Role::User,
        text: text.clone(),
    }));
    busy().set(true);
    state().with_untracked(|state| {
        if let Some(sender) = state.to_agent.clone().as_ref() {
            let _ = sender.send(ChatCommand::Send(text));
        }
    });
}

/// 应用一条后台事件（在引擎线程调用，写信号安全）。
pub(crate) fn apply(event: BridgeEvent) {
    match event {
        BridgeEvent::Started { model } => {
            messages().update(|messages| messages.push(Message {
                role: Role::Info,
                text: format!("后台就绪 · 模型 {model}").into(),
            }));
        }
        BridgeEvent::Delta(delta) => {
            messages().update(|messages| {
                let starts_new = messages
                    .last()
                    .is_none_or(|last| !matches!(last.role, Role::Assistant));
                if starts_new {
                    messages.push(Message { role: Role::Assistant, text: delta });
                } else if let Some(last) = messages.last_mut() {
                    let mut combined = String::from(last.text.as_str());
                    combined.push_str(&delta);
                    last.text = combined.into();
                }
            });
        }
        BridgeEvent::Info(text) => {
            messages().update(|messages| messages.push(Message {
                role: Role::Info,
                text,
            }));
        }
        BridgeEvent::Error(text) => {
            messages().update(|messages| messages.push(Message {
                role: Role::Info,
                text: format!("⚠ {text}").into(),
            }));
        }
        BridgeEvent::Done => {
            busy().set(false);
        }
    }
}
