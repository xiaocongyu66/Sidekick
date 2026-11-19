//! 暖灯：情感陪伴聊天 app（nsh 纯 Rust，后端为内嵌的 harn agent）。

mod bridge;
mod prompt;
mod store;

use nsh_base::{button, input, theme};
use nsh_core::element::{Position, SharedString};
use nsh_core::prelude::*;

use store::Role;

/// UI 组件集（`nsh.json` 的 `"rust"`）。
#[native_module("companion/native")]
fn native(registry: &mut Registry) {
    registry.register::<App>();
}

/// App 根：主题底色上的一屏聊天。
#[component]
pub fn App() -> impl IntoElement {
    bridge::start();
    view()
        .flex_1()
        .bg(move || theme().background)
        .child(chat())
}

fn chat() -> impl IntoElement {
    view()
        .flex_1()
        .p_4()
        .gap_3()
        .child(
            virtual_list(store::message_count, message_row)
                .keys(store::message_ids)
                .estimated_item_size(px(56.0))
                .flex_1(),
        )
        .child(composer())
}

fn composer() -> impl IntoElement {
    view()
        .flex_row()
        .items_center()
        .gap_2()
        .child(
            input()
                .value(move || store::draft_text().as_str().to_owned())
                .on_change(move |value: &str| store::set_draft(SharedString::from(value.to_owned())))
                .on_submit(move |_: &str| store::submit())
                .placeholder("和暖灯说说话…")
                .flex_1()
                .px_3()
                .py_2()
                .border_1()
                .border_color(move || theme().input)
                .rounded(move || theme().radius)
                .text_color(move || theme().foreground),
        )
        .child(
            button()
                .disabled(move || store::is_busy() || store::draft_text().trim().is_empty())
                .on_press(move || store::submit())
                .px_4()
                .py_2()
                .rounded(move || theme().radius)
                .bg(move || theme().primary)
                .active(|s| s.opacity(0.6))
                .disabled_style(|s| s.opacity(0.4))
                .child(text("发送").text_color(move || theme().primary_foreground)),
        )
}

/// 一条消息气泡：配色随角色（v1 左对齐，后续再做左右分边）。
fn message_row(at: Position) -> impl IntoElement {
    let message = computed(move || store::message_at(at.index()));
    view().flex_row().child(
        view()
            .px_3()
            .py_2()
            .rounded(px(16.0))
            .bg(move || {
                let user = message.get().is_some_and(|m| m.role == Role::User);
                let tokens = theme();
                if user { tokens.primary } else { tokens.muted }
            })
            .child(
                text(move || {
                    message
                        .get()
                        .map(|message| message.text)
                        .unwrap_or_default()
                })
                .text_color(move || {
                    let user = message.get().is_some_and(|m| m.role == Role::User);
                    let tokens = theme();
                    if user { tokens.primary_foreground } else { tokens.foreground }
                }),
            ),
    )
}
