//! 无头冒烟：启动 app、发送一条消息、等待回合结束（mock provider 回显）。

use std::time::Duration;

use nsh_engine::{EngineConfig, Root};
use nsh_testing::Headless;

fn start() -> Headless {
    let app = Headless::start(EngineConfig::new(Root::of::<companion::App>())).unwrap();
    app
}

#[test]
fn shows_welcome_and_sends_one_message() {
    let mut app = start();
    app.wait_for("我在呢。今天想聊点什么？").unwrap();
    app.type_text("text:和暖灯说说话…", "我最近有点累").unwrap();
    app.press("role:button[发送]").unwrap();
    // 用户气泡入流（本地立即）。
    app.wait_for("我最近有点累").unwrap();
    // mock provider 回显最后一条用户输入 → 回合结束按钮恢复可用。
    for _ in 0..100 {
        if app.screen().enabled("role:button[发送]").unwrap_or(true) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    let screen = app.screen();
    panic!(
        "agent turn did not finish within 10s\n--- screen: {:?}",
        screen.texts()
    );
}
