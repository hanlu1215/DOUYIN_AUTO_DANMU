//! 运行配置：界面与工作线程共享（对应 C++ 版 config）。
//! 字段由互斥锁保护，工作线程每次循环实时读取快照，界面修改后立即生效。

use std::path::Path;
use std::sync::Mutex;

use crate::json_mini as jm;

/// 抖音直播发言频率的安全下限（秒），界面不允许低于此值
pub const MIN_INTERVAL: f64 = 2.0;

/// 输入框区域（屏幕物理坐标）
#[derive(Clone, Copy, Default)]
pub struct Region {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

#[derive(Clone)]
pub struct ConfigData {
    pub has_region: bool,      // 是否已框选输入框区域
    pub region: Region,        // 输入框区域
    pub interval: f64,         // 发送基础间隔（秒）
    pub jitter: i32,           // 随机抖动（±%）
    pub shuffle: bool,         // 每轮循环随机打乱顺序
    pub fg_check: bool,        // 校验前台窗口标题（防误发到其他程序）
    pub fg_keyword: String,    // 前台窗口标题关键词
    pub current_file: String,  // 当前弹幕文件名
}

impl Default for ConfigData {
    fn default() -> Self {
        ConfigData {
            has_region: false,
            region: Region::default(),
            interval: 10.0,
            jitter: 20,
            shuffle: true,
            fg_check: true,
            fg_keyword: "抖音".to_string(),
            current_file: String::new(),
        }
    }
}

pub struct AppConfig {
    pub data: Mutex<ConfigData>,
}

impl AppConfig {
    pub fn new() -> Self {
        AppConfig { data: Mutex::new(ConfigData::default()) }
    }

    /// 取一份配置快照（工作线程每轮循环调用）
    pub fn snapshot(&self) -> ConfigData {
        self.data.lock().unwrap().clone()
    }
}

/// 读取 config.json（不存在或损坏则保留默认值）
pub fn load_config(cfg: &AppConfig, path: &Path) {
    let text = match std::fs::read_to_string(path) {
        Ok(t) => t,
        Err(_) => return,
    };
    let root = match jm::parse(&text) {
        Ok(v) => v,
        Err(_) => return, // 配置损坏时忽略，用默认值
    };
    if !matches!(root, jm::Value::Object(_)) {
        return;
    }

    let mut d = cfg.data.lock().unwrap();
    if let Some(jm::Value::Array(r)) = root.find("region") {
        if r.len() == 4 {
            if let (Some(a), Some(b), Some(c), Some(e)) = (
                num(&r[0]), num(&r[1]), num(&r[2]), num(&r[3]),
            ) {
                d.region = Region {
                    left: a as i32,
                    top: b as i32,
                    right: c as i32,
                    bottom: e as i32,
                };
                d.has_region = true;
            }
        }
    }
    if let Some(v) = root.find("interval").and_then(num) {
        if v > 0.0 {
            d.interval = v;
        }
    }
    if let Some(v) = root.find("jitter").and_then(num) {
        d.jitter = v as i32;
    }
    if let Some(jm::Value::Bool(b)) = root.find("shuffle") {
        d.shuffle = *b;
    }
    if let Some(jm::Value::Bool(b)) = root.find("fg_check") {
        d.fg_check = *b;
    }
    if let Some(jm::Value::Str(s)) = root.find("fg_keyword") {
        d.fg_keyword = s.clone();
    }
    if let Some(jm::Value::Str(s)) = root.find("current_file") {
        d.current_file = s.clone();
    }
}

fn num(v: &jm::Value) -> Option<f64> {
    match v {
        jm::Value::Number(n) => Some(*n),
        _ => None,
    }
}

/// 保存 config.json（UTF-8）
pub fn save_config(cfg: &AppConfig, path: &Path) {
    let out = {
        let d = cfg.data.lock().unwrap();
        let region = if d.has_region {
            format!(
                "[{}, {}, {}, {}]",
                d.region.left, d.region.top, d.region.right, d.region.bottom
            )
        } else {
            "null".to_string()
        };
        let current_file = if d.current_file.is_empty() {
            "null".to_string()
        } else {
            format!("\"{}\"", jm::escape(&d.current_file))
        };
        format!(
            "{{\n  \"region\": {},\n  \"interval\": {},\n  \"jitter\": {},\n  \"shuffle\": {},\n  \"fg_check\": {},\n  \"fg_keyword\": \"{}\",\n  \"current_file\": {}\n}}\n",
            region,
            crate::strutil::format_g(d.interval),
            d.jitter,
            d.shuffle,
            d.fg_check,
            jm::escape(&d.fg_keyword),
            current_file
        )
    };
    let _ = std::fs::write(path, out);
}
