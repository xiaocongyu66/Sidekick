//! harn-ui：UI 无关的转录模型 + plain 渲染器。
//!
//! 分层：
//! - [`transcript`]：Agent 事件 → 渲染条目（工具生命周期、子代理、用量）。
//! - [`plain`]：流式纯文本渲染。
//! - [`render`]：转录 → 渲染行的纯数据投影（摘要行 + 详情 + 分组折叠），
//!   plain 与外部渲染器共用。
//! - [`markdown`] / [`highlight`]：Markdown 解析与语法高亮（渲染复用）。

pub mod highlight;
pub mod markdown;
pub mod plain;
pub mod render;
pub mod transcript;

pub use plain::PlainUi;
pub use render::{
    InlineKind, InlineSpan, RenderLine, RenderOptions, RenderedTranscript, render, tag_of,
    tool_header,
};
pub use transcript::{Entry, ToolCallState, Transcript};
