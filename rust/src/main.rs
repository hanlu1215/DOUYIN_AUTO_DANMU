//! 抖音直播自动弹幕助手（Rust 版）—— 程序入口。
//!
//! 构建：cargo build --release
//! 产物 target/release/douyin_danmu.exe 只依赖 Windows 系统 DLL，
//! 可拷贝到其他 Win10/11 机器直接运行。

// GUI 程序，不弹控制台窗口（与 C++ 版 WIN32 子系统一致）
#![windows_subsystem = "windows"]

mod app;
mod config;
mod danmu_io;
mod json_mini;
mod sender;
mod strutil;
mod win_input;

use std::path::{Path, PathBuf};

use windows_sys::Win32::UI::Controls::{
    InitCommonControlsEx, INITCOMMONCONTROLSEX, ICC_STANDARD_CLASSES, ICC_WIN95_CLASSES,
};

use app::DanmuApp;
use strutil::{AppError, AppResult};

/// 程序根目录：默认取 exe 所在目录；若 exe 在 target/release/ 之类的构建目录下，
/// 则向上最多找 5 层，定位含 danmu/ 文件夹的项目根目录
fn get_app_dir() -> PathBuf {
    let exe = std::env::current_exe().unwrap_or_else(|_| PathBuf::from("."));
    let dir = exe
        .parent()
        .map(|p| p.to_path_buf())
        .unwrap_or_else(|| PathBuf::from("."));
    let mut cur = dir.clone();
    for _ in 0..5 {
        if cur.join("danmu").is_dir() {
            return cur;
        }
        if !cur.pop() {
            break;
        }
    }
    dir
}

/// GUI 程序没有控制台，错误写入 error.log 方便排查（与 Python / C++ 版一致）
fn write_error_log(dir: &Path, text: &str) {
    let _ = std::fs::write(dir.join("error.log"), format!("{}\n", text));
}

fn main() {
    let app_dir = get_app_dir();
    // panic 也要落到 error.log（对应 C++ 版的 catch (...)）
    let log_dir = app_dir.clone();
    std::panic::set_hook(Box::new(move |info| {
        write_error_log(&log_dir, &format!("panic: {}", info));
    }));
    match real_main(&app_dir) {
        Ok(code) => std::process::exit(code),
        Err(AppError(msg)) => {
            write_error_log(&app_dir, &msg);
            std::process::exit(1);
        }
    }
}

fn real_main(app_dir: &Path) -> AppResult<i32> {
    win_input::set_process_dpi_aware(); // 必须在创建窗口前调用
    unsafe {
        let icc = INITCOMMONCONTROLSEX {
            dwSize: std::mem::size_of::<INITCOMMONCONTROLSEX>() as u32,
            dwICC: ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES,
        };
        InitCommonControlsEx(&icc);
    }
    let mut app = DanmuApp::new(app_dir.to_path_buf())?;
    Ok(app.run())
}
