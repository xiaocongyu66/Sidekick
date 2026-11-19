//! 引擎线程 ↔ harn agent loop 的桥接。
//!
//! harn 的异步栈跑在 nsh 引擎自带的 tokio runtime（`background`）；
//! 命令与事件经两条 mpsc 通道跨越线程边界——引擎事件任务的 waker
//! 由 nsh 的 Executor 持有，tokio 线程写入事件即可唤醒它回到引擎线程。

use harn_agent::UserInput;
use harn_cli::app::{Runtime, build_runtime};
use harn_config::{Config, ConfigOverrides};
use harn_core::event::AgentEvent;
use nsh_core::prelude::*;
use tokio::sync::mpsc;


use crate::prompt::COMPANION_SYSTEM;
use crate::store;

/// 启动桥接：App 首次渲染时在引擎线程调用。重复调用是幂等的。
pub(crate) fn start() {
    if try_global::<store::BridgeStarted>().is_some() {
        return;
    }
    global::<store::BridgeStarted>();
    let (commands_tx, commands_rx) = mpsc::unbounded_channel::<store::ChatCommand>();
    let (events_tx, mut events_rx) = mpsc::unbounded_channel::<store::BridgeEvent>();
    store::set_agent_channel(commands_tx);

    // 引擎线程：后台事件 → 信号。
    spawn(async move {
        while let Some(event) = events_rx.recv().await {
            store::apply(event);
        }
    });

    // 专用线程 + 独立 tokio runtime：harn agent loop（nsh 引擎线程无 tokio 上下文）。
    std::thread::Builder::new()
        .name("harn-agent".into())
        .spawn(move || {
            let runtime = tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
                .expect("harn tokio runtime");
            runtime.block_on(async move {
                if let Err(error) = run_agent(commands_rx, events_tx.clone()).await {
                    let _ = events_tx.send(store::BridgeEvent::Error(
                        format!("后台启动失败：{error}").into(),
                    ));
                }
            });
        })
        .expect("harn agent thread");
}

async fn run_agent(
    mut commands: mpsc::UnboundedReceiver<store::ChatCommand>,
    events: mpsc::UnboundedSender<store::BridgeEvent>,
) -> anyhow::Result<()> {
    let config = Config::resolve(ConfigOverrides {
        system_prompt: Some(COMPANION_SYSTEM.to_owned()),
        auto_approve: true,
        non_interactive: true,
        ..Default::default()
    })?;
    let model = config.model.clone();
    let Runtime {
        agent,
        abort,
        mut events_rx,
        ..
    } = build_runtime(config).await?;

    let _ = events.send(store::BridgeEvent::Started { model: model.into() });

    let mut agent = agent;
    loop {
        tokio::select! {
            maybe_command = commands.recv() => {
                let Some(store::ChatCommand::Send(text)) = maybe_command else { break };
                abort.reset();
                match agent.send(UserInput::text(String::from(text.as_str()))).await {
                    Ok(_) => {
                        let _ = events.send(store::BridgeEvent::Done);
                    }
                    Err(error) => {
                        let _ = events.send(store::BridgeEvent::Error(
                            error.to_string().into(),
                        ));
                    }
                }
            }
            maybe_event = events_rx.recv() => {
                let Some(event) = maybe_event else { break };
                if let Some(mapped) = map_event(&event) {
                    let _ = events.send(mapped);
                }
            }
        }
    }
    Ok(())
}

/// AgentEvent → UI 事件：陪伴场景只关心文本流、状态与错误。
fn map_event(event: &AgentEvent) -> Option<store::BridgeEvent> {
    match event {
        AgentEvent::TextDelta { text, .. } => {
            Some(store::BridgeEvent::Delta(text.clone().into()))
        }
        AgentEvent::Error { message, .. } => {
            Some(store::BridgeEvent::Error(message.clone().into()))
        }
        AgentEvent::Status { text, .. } => {
            Some(store::BridgeEvent::Info(text.clone().into()))
        }
        AgentEvent::PermissionDenied { name, reason, .. } => Some(store::BridgeEvent::Info(
            format!("{name}: {reason}").into(),
        )),
        _ => None,
    }
}
