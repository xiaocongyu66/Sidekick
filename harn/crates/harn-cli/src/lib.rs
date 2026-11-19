//! harn-cli：命令行装配、REPL、无头模式与子命令。

pub mod app;
pub mod cli;
pub mod commands;
pub mod server;
pub mod slash;

pub use app::run;
