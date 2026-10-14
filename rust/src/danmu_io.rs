//! 弹幕 JSON 读写（对应 C++ 版 danmu_io）。
//! 接受 ["...", ...] 或 {"items"/"danmu": [...]}；加载时去空白、去重；
//! 保存时每行一条。

use std::collections::HashSet;
use std::path::Path;

use crate::json_mini as jm;
use crate::strutil::{AppError, AppResult};

pub fn load_danmu(path: &Path) -> AppResult<Vec<String>> {
    let text = std::fs::read_to_string(path)
        .map_err(|_| AppError(format!("无法打开文件：{}", path.display())))?;

    let root = jm::parse(&text).map_err(AppError)?;

    let list = match &root {
        jm::Value::Object(_) => root.find("items").or_else(|| root.find("danmu")),
        _ => Some(&root),
    };
    let arr = match list {
        Some(jm::Value::Array(a)) => a,
        _ => {
            return Err(AppError::new(
                "JSON 格式不正确：顶层需要是字符串数组，或包含 items/danmu 数组的对象",
            ))
        }
    };

    let mut items = Vec::new();
    let mut seen = HashSet::new();
    for el in arr {
        match el {
            jm::Value::Str(s) => {
                let t = s.trim();
                if !t.is_empty() && !seen.contains(t) {
                    seen.insert(t.to_string());
                    items.push(t.to_string());
                }
            }
            _ => return Err(AppError::new("JSON 格式不正确：数组中存在非字符串元素")),
        }
    }
    if items.is_empty() {
        return Err(AppError::new("弹幕文件为空"));
    }
    Ok(items)
}

pub fn save_danmu(path: &Path, items: &[String]) -> AppResult<()> {
    let mut out = String::from("[\n");
    for (i, item) in items.iter().enumerate() {
        out.push_str("  \"");
        out.push_str(&jm::escape(item));
        out.push('"');
        if i + 1 < items.len() {
            out.push(',');
        }
        out.push('\n');
    }
    out.push_str("]\n");
    std::fs::write(path, out)
        .map_err(|_| AppError(format!("无法写入文件：{}", path.display())))
}
