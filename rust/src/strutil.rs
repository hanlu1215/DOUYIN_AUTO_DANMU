//! 字符串工具：Rust String（UTF-8）↔ Win32 宽字符串（UTF-16）互转、统一错误类型。
//! （对应 C++ 版 strutil.h）

use std::os::windows::ffi::OsStrExt;
use std::path::Path;

/// 统一错误类型：消息直接用于弹窗 / 写日志（界面全部为中文）
#[derive(Debug)]
pub struct AppError(pub String);

pub type AppResult<T> = Result<T, AppError>;

impl AppError {
    pub fn new(msg: impl Into<String>) -> Self {
        AppError(msg.into())
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for AppError {}

/// Rust 字符串 → 以 0 结尾的 UTF-16 缓冲（供 *W 系 API 使用）
pub fn to_wide(s: &str) -> Vec<u16> {
    s.encode_utf16().chain(std::iter::once(0)).collect()
}

/// 以 0 结尾的 UTF-16 指针 → Rust 字符串
///
/// # Safety
/// 调用者保证指针指向有效的、以 0 结尾的 UTF-16 缓冲区（或为空指针）。
pub unsafe fn from_wide_ptr(p: *const u16) -> String {
    if p.is_null() {
        return String::new();
    }
    let mut len = 0;
    while *p.add(len) != 0 {
        len += 1;
    }
    String::from_utf16_lossy(std::slice::from_raw_parts(p, len))
}

/// 路径 → 宽字符（Windows 路径可能不是合法 UTF-8，不能直接走 &str）
pub fn path_wide(p: &Path) -> Vec<u16> {
    p.as_os_str().encode_wide().chain(std::iter::once(0)).collect()
}

/// C 风格 %g 的近似：整数不带小数点（Rust 的 f64 Display 本身就是这个行为）
pub fn format_g(v: f64) -> String {
    format!("{}", v)
}
