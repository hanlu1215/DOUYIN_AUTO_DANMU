//! 发送工作线程：光标区域检测 + 频率控制 + 弹幕循环发送（对应 C++ 版 sender）。
//!
//! 循环流程：检测光标 → 不在输入框区域内则暂停 → 校验前台窗口 →
//! 弹幕写入剪贴板 → Ctrl+V 粘贴 → 回车发送 → 按「基础间隔 ± 随机抖动」休眠。
//! 每次循环实时读取共享的 AppConfig，界面修改设置后立即生效，无需重启线程。
//!
//! 工作线程 → 界面 通过 PostMessageW 发自定义消息（WM_DANMU_*），
//! lParam 负载由工作线程 Box::into_raw 分配、界面 WM 处理函数负责回收（与 C++ 版一致）。

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Condvar, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use windows_sys::Win32::Foundation::{HWND, LPARAM, POINT, WPARAM};
use windows_sys::Win32::System::Threading::Sleep;
use windows_sys::Win32::UI::WindowsAndMessaging::{PostMessageW, WM_APP};

use crate::config::{AppConfig, Region};
use crate::danmu_io::{load_danmu, save_danmu};
use crate::strutil::{AppError, AppResult};
use crate::win_input;

// ---- 工作线程 → 界面 的自定义消息（lParam 由界面负责释放）----
pub const WM_DANMU_SENT: u32 = WM_APP + 1; // lParam: *mut SentInfo
pub const WM_DANMU_PAUSED: u32 = WM_APP + 2; // lParam: 打包的光标坐标（低16位 x，高16位 y）
pub const WM_DANMU_FGBLOCKED: u32 = WM_APP + 3; // lParam: *mut String（前台窗口标题）
pub const WM_DANMU_ERROR: u32 = WM_APP + 4; // lParam: *mut String（错误信息）
pub const WM_DANMU_STOPPED: u32 = WM_APP + 5; // 无 payload

pub struct SentInfo {
    pub text: String,
    pub idx: i32,   // 本循环已发条数（1 起）
    pub total: i32, // 总条数
}

/// 条件不满足时的轮询间隔
const CHECK_INTERVAL: Duration = Duration::from_millis(300);
/// 两条弹幕之间的绝对下限（秒）
const MIN_DELAY: f64 = 1.0;

/// 判断坐标是否在矩形区域内
pub fn point_in_region(pos: POINT, has_region: bool, r: &Region) -> bool {
    if !has_region {
        return false;
    }
    pos.x >= r.left && pos.x <= r.right && pos.y >= r.top && pos.y <= r.bottom
}

// ---------- 小型随机数发生器（xorshift64*，抖动与打乱足够用，避免引入 rand 依赖）----------

pub struct Rng(u64);

impl Rng {
    pub fn new() -> Self {
        let t = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0x9E3779B97F4A7C15);
        // 混入栈地址增加熵
        let stack = &t as *const u64 as u64;
        let mut seed = t ^ stack.rotate_left(17) ^ 0x9E3779B97F4A7C15;
        if seed == 0 {
            seed = 0x2545F4914F6CDD1D;
        }
        Rng(seed)
    }

    fn next_u64(&mut self) -> u64 {
        let mut x = self.0;
        x ^= x >> 12;
        x ^= x << 25;
        x ^= x >> 27;
        self.0 = x;
        x.wrapping_mul(0x2545F4914F6CDD1D)
    }

    /// [0, 1) 均匀分布
    fn gen_f64(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }

    /// [low, high] 闭区间
    fn gen_range_u32(&mut self, low: u32, high: u32) -> u32 {
        if high <= low {
            return low;
        }
        low + (self.next_u64() % ((high - low + 1) as u64)) as u32
    }

    /// [low, high] 均匀浮点
    fn gen_range_f64(&mut self, low: f64, high: f64) -> f64 {
        low + (high - low) * self.gen_f64()
    }

    /// Fisher-Yates 打乱
    fn shuffle<T>(&mut self, items: &mut [T]) {
        for i in (1..items.len()).rev() {
            let j = (self.next_u64() % ((i + 1) as u64)) as usize;
            items.swap(i, j);
        }
    }
}

// ---------- DanmuLoader：管理一个弹幕 JSON 文件（线程安全）----------

struct LoaderInner {
    path: Option<PathBuf>,
    items: Vec<String>,
    index: usize,
    rng: Rng,
}

pub struct DanmuLoader {
    inner: Mutex<LoaderInner>,
}

impl DanmuLoader {
    pub fn new() -> Self {
        DanmuLoader {
            inner: Mutex::new(LoaderInner {
                path: None,
                items: Vec::new(),
                index: 0,
                rng: Rng::new(),
            }),
        }
    }

    pub fn load(&self, path: PathBuf) -> AppResult<()> {
        let items = load_danmu(&path)?; // 可能失败
        let mut g = self.inner.lock().unwrap();
        g.items = items;
        g.path = Some(path);
        g.index = 0;
        Ok(())
    }

    pub fn clear(&self) {
        let mut g = self.inner.lock().unwrap();
        g.path = None;
        g.items.clear();
        g.index = 0;
    }

    pub fn path(&self) -> Option<PathBuf> {
        self.inner.lock().unwrap().path.clone()
    }

    pub fn size(&self) -> usize {
        self.inner.lock().unwrap().items.len()
    }

    /// 取下一条弹幕；一轮发完后从头循环（可选每轮打乱顺序）。None 表示无条目
    pub fn next(&self, shuffle: bool) -> Option<(String, i32, i32)> {
        let mut g = self.inner.lock().unwrap();
        if g.items.is_empty() {
            return None;
        }
        if g.index >= g.items.len() {
            if shuffle {
                let mut rng = Rng(g.rng.0);
                rng.shuffle(&mut g.items);
                g.rng = rng;
            }
            g.index = 0;
        }
        let text = g.items[g.index].clone();
        let idx = g.index as i32 + 1;
        let total = g.items.len() as i32;
        g.index += 1;
        Some((text, idx, total))
    }

    /// 追加自定义条目（去重）并写回文件；失败返回原因
    pub fn add(&self, text: &str) -> Result<(), String> {
        let t = text.trim();
        if t.is_empty() {
            return Err("内容为空".to_string());
        }
        let mut g = self.inner.lock().unwrap();
        let path = match &g.path {
            Some(p) => p.clone(),
            None => return Err("未加载弹幕文件".to_string()),
        };
        if g.items.iter().any(|s| s == t) {
            return Err("该条目已存在".to_string());
        }
        g.items.push(t.to_string());
        if let Err(e) = save_danmu(&path, &g.items) {
            g.items.pop();
            return Err(e.0);
        }
        Ok(())
    }
}

// ---------- 可中断的停止信号（对应 C++ 版的 stopEvent + WaitForSingleObject）----------

pub struct StopSignal {
    m: Mutex<bool>,
    cv: Condvar,
}

impl StopSignal {
    fn new() -> Self {
        StopSignal { m: Mutex::new(false), cv: Condvar::new() }
    }
    fn stop(&self) {
        *self.m.lock().unwrap() = true;
        self.cv.notify_all();
    }
    /// 等待指定时长；返回 true 表示收到了停止请求
    fn wait(&self, dur: Duration) -> bool {
        let g = self.m.lock().unwrap();
        if *g {
            return true;
        }
        let (g, _) = self.cv.wait_timeout(g, dur).unwrap();
        *g
    }
}

// ---------- SenderWorker ----------

pub struct SenderWorker {
    stop: Arc<StopSignal>,
    running: Arc<AtomicBool>,
    handle: Option<JoinHandle<()>>,
}

impl SenderWorker {
    pub fn start(loader: Arc<DanmuLoader>, cfg: Arc<AppConfig>, notify: HWND) -> Self {
        let stop = Arc::new(StopSignal::new());
        let running = Arc::new(AtomicBool::new(true));
        // HWND 是原始指针、不能跨线程传递，转成 usize 再传（仅作消息目标地址使用）
        let notify = notify as usize;
        let handle = {
            let stop = Arc::clone(&stop);
            let running = Arc::clone(&running);
            std::thread::spawn(move || run_loop(&loader, &cfg, notify, &stop, &running))
        };
        SenderWorker { stop, running, handle: Some(handle) }
    }

    pub fn stop(&self) {
        self.stop.stop();
    }

    pub fn join(&mut self) {
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
    }

    pub fn running(&self) -> bool {
        self.running.load(Ordering::SeqCst)
    }
}

impl Drop for SenderWorker {
    fn drop(&mut self) {
        self.stop();
        self.join();
    }
}

fn post_string(notify: usize, msg: u32, s: String) {
    unsafe {
        PostMessageW(notify as HWND, msg, 0 as WPARAM, Box::into_raw(Box::new(s)) as LPARAM);
    }
}

fn run_loop(
    loader: &DanmuLoader,
    cfg: &AppConfig,
    notify: usize,
    stop: &StopSignal,
    running: &AtomicBool,
) {
    if let Err(e) = run_inner(loader, cfg, notify, stop) {
        post_string(notify, WM_DANMU_ERROR, e.0);
    }
    running.store(false, Ordering::SeqCst);
    unsafe {
        PostMessageW(notify as HWND, WM_DANMU_STOPPED, 0 as WPARAM, 0 as LPARAM);
    }
}

fn run_inner(
    loader: &DanmuLoader,
    cfg: &AppConfig,
    notify: usize,
    stop: &StopSignal,
) -> AppResult<()> {
    let mut rng = Rng::new();
    while !stop.wait(Duration::ZERO) {
        // 每次循环实时读取配置快照，界面修改后立即生效
        let snap = cfg.snapshot();

        // 1. 光标必须位于输入框区域内
        let pos = win_input::get_cursor_pos();
        if !point_in_region(pos, snap.has_region, &snap.region) {
            let lp = ((pos.y as u16 as u32) << 16 | (pos.x as u16 as u32)) as LPARAM;
            unsafe {
                PostMessageW(notify as HWND, WM_DANMU_PAUSED, 0 as WPARAM, lp);
            }
            if stop.wait(CHECK_INTERVAL) {
                break;
            }
            continue;
        }

        // 2. 前台窗口校验：避免把弹幕发到其他程序里
        if snap.fg_check {
            let kw = if snap.fg_keyword.trim().is_empty() {
                "抖音".to_string()
            } else {
                snap.fg_keyword.trim().to_string()
            };
            let kw = kw.to_lowercase();
            let title = win_input::get_foreground_title();
            if !title.to_lowercase().contains(&kw) {
                post_string(notify, WM_DANMU_FGBLOCKED, title);
                if stop.wait(CHECK_INTERVAL) {
                    break;
                }
                continue;
            }
        }

        // 3. 取词 → 随机前置延迟 → 剪贴板 → 粘贴 → 回车
        let (text, idx, total) = match loader.next(snap.shuffle) {
            Some(v) => v,
            None => return Err(AppError::new("弹幕文件为空")),
        };

        // 随机前置延迟，节奏更接近真人
        let pre = rng.gen_range_u32(100, 400);
        if stop.wait(Duration::from_millis(pre as u64)) {
            break;
        }

        win_input::set_clipboard(&text)?;
        win_input::paste();
        unsafe { Sleep(150) } // 等输入框收到粘贴内容
        win_input::press_enter();

        unsafe {
            PostMessageW(
                notify as HWND,
                WM_DANMU_SENT,
                0 as WPARAM,
                Box::into_raw(Box::new(SentInfo { text, idx, total })) as LPARAM,
            );
        }

        // 4. 频率控制：基础间隔 × (1 ± 抖动比例)，下限 1 秒
        let j = snap.jitter as f64 / 100.0;
        let delay = (snap.interval * rng.gen_range_f64(1.0 - j, 1.0 + j)).max(MIN_DELAY);
        if stop.wait(Duration::from_secs_f64(delay)) {
            break;
        }
    }
    Ok(())
}
