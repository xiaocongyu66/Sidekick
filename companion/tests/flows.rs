//! 无头冒烟：启动 app、发送一条消息、等待回合结束（mock provider 回显）。


use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

fn start() -> Headless {
    Headless::start(EngineConfig::new(Root::of::<companion::App>())).unwrap()
}

#[test]
fn shows_welcome_and_sends_one_message() {
    let mut app = start();
    app.wait_for("我在呢。今天想聊点什么？").unwrap();
    app.type_text("text:和暖灯说说话…", "我最近有点累").unwrap();
    app.press("role:button[发送]").unwrap();
    // 用户气泡入流（本地立即）。
    app.wait_for("我最近有点累").unwrap();
    // mock provider 回显最后一条用户输入。
    app.wait_for("mock: 我最近有点累").unwrap();
}
