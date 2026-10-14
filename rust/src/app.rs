//! 主窗口：原生 Win32 界面（对应 C++ 版 app.cpp / Python 版 tkinter UI）。
//!
//! 说明：C++ 版的「非递归互斥锁不能重入、持锁弹窗会自我死锁」问题在 Rust 版
//! 通过快照模式天然规避 —— 任何需要配置的地方都先 `cfg.snapshot()`（短暂加锁、
//! 拷贝、立即释放），再与界面交互。

use std::path::PathBuf;
use std::ptr::{null, null_mut};
use std::sync::Arc;

use windows_sys::core::PCWSTR;
use windows_sys::Win32::Foundation::*;
use windows_sys::Win32::Graphics::Gdi::*;
use windows_sys::Win32::System::LibraryLoader::GetModuleHandleW;
use windows_sys::Win32::System::SystemInformation::GetLocalTime;
use windows_sys::Win32::System::SystemServices::SS_RIGHT;
use windows_sys::Win32::UI::Controls::{BST_CHECKED, BST_UNCHECKED};
use windows_sys::Win32::UI::Controls::Dialogs::{
    GetOpenFileNameW, OFN_FILEMUSTEXIST, OFN_HIDEREADONLY, OPENFILENAMEW,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    EnableWindow, ReleaseCapture, SetCapture, SetFocus, VK_ESCAPE, VK_RETURN,
};
use windows_sys::Win32::UI::Shell::ShellExecuteW;
use windows_sys::Win32::UI::WindowsAndMessaging::*;

use crate::config::{load_config, save_config, AppConfig, MIN_INTERVAL};
use crate::danmu_io::load_danmu;
use crate::sender::{
    DanmuLoader, SenderWorker, SentInfo, WM_DANMU_ERROR, WM_DANMU_FGBLOCKED, WM_DANMU_PAUSED,
    WM_DANMU_SENT, WM_DANMU_STOPPED,
};
use crate::strutil::{format_g, from_wide_ptr, path_wide, to_wide, AppError, AppResult};
use crate::win_input;

// ---- 控件 ID ----
const IDC_COMBO_FILE: i32 = 1001;
const IDC_BTN_IMPORT: i32 = 1002;
const IDC_BTN_REFRESH: i32 = 1003;
const IDC_BTN_OPENFOLDER: i32 = 1004;
const IDC_STATIC_FILEINFO: i32 = 1005;
const IDC_EDIT_INTERVAL: i32 = 1006;
const IDC_EDIT_JITTER: i32 = 1007;
const IDC_CHK_SHUFFLE: i32 = 1008;
const IDC_CHK_FGCHECK: i32 = 1009;
const IDC_EDIT_FGKEYWORD: i32 = 1010;
const IDC_STATIC_FGNOTE: i32 = 1011;
const IDC_STATIC_HINT: i32 = 1012;
const IDC_STATIC_REGION: i32 = 1013;
const IDC_BTN_PICKREGION: i32 = 1014;
const IDC_BTN_CLEARREGION: i32 = 1015;
const IDC_STATIC_CURSOR: i32 = 1016;
const IDC_BTN_START: i32 = 1017;
const IDC_BTN_STOP: i32 = 1018;
const IDC_STATIC_STATUS: i32 = 1019;
const IDC_STATIC_PROGRESS: i32 = 1020;
const IDC_EDIT_CUSTOM: i32 = 1021;
const IDC_BTN_ADDCUSTOM: i32 = 1022;
const IDC_LIST_LOG: i32 = 1023;

const MAIN_CLASS: &str = "DanmuAssistantMain";
const OVERLAY_CLASS: &str = "DanmuRegionSelector";
const IDT_CURSOR: usize = 1; // 光标状态刷新定时器（150ms）

const IDI_APPICON: usize = 101; // app.rc 嵌入的手绘风格抖音音符图标

const K_GRAY: COLORREF = 136 | (136 << 8) | (136 << 16);
const K_GREEN: COLORREF = 46 | (125 << 8) | (50 << 16);
const K_ORANGE: COLORREF = 230 | (81 << 8);

fn make_font(pt: i32, bold: bool, face: &str) -> HFONT {
    unsafe {
        let hdc = GetDC(null_mut());
        let dpi = GetDeviceCaps(hdc, LOGPIXELSY as i32);
        ReleaseDC(null_mut(), hdc);
        // MulDiv(pt, dpi, 72)，pt/dpi 均为正数，+36 实现四舍五入
        let h = -((pt * dpi + 36) / 72);
        let face_w = to_wide(face);
        CreateFontW(
            h,
            0,
            0,
            0,
            if bold { FW_BOLD } else { FW_NORMAL } as i32,
            0,
            0,
            0,
            DEFAULT_CHARSET as u32,
            OUT_DEFAULT_PRECIS as u32,
            CLIP_DEFAULT_PRECIS as u32,
            CLEARTYPE_QUALITY as u32,
            (DEFAULT_PITCH | FF_DONTCARE) as u32,
            face_w.as_ptr(),
        )
    }
}

unsafe extern "system" fn set_font_cb(child: HWND, lp: LPARAM) -> BOOL {
    SendMessageW(child, WM_SETFONT, lp as WPARAM, 1);
    1
}

/// 自定义弹幕输入框：拦截回车 = 点「添加到当前文件」
unsafe extern "system" fn custom_edit_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    if m == WM_KEYDOWN && w == VK_RETURN as usize {
        let parent = GetParent(h);
        let app = GetWindowLongPtrW(parent, GWLP_USERDATA) as *mut DanmuApp;
        if !app.is_null() {
            (*app).add_custom();
        }
        return 0;
    }
    // 旧 WndProc 存在控件的 GWLP_USERDATA 里
    let old = GetWindowLongPtrW(h, GWLP_USERDATA);
    let old_proc: WNDPROC = std::mem::transmute(old);
    CallWindowProcW(old_proc, h, m, w, l)
}

// ================= 全屏半透明遮罩：拖拽框选抖音输入框区域 =================

struct OverlayState {
    app: *mut DanmuApp,
    origin_x: i32,
    origin_y: i32,
    dragging: bool,
    has_rect: bool,
    start: POINT,
    rect: RECT,
    font_text: HFONT,
}

impl OverlayState {
    unsafe fn finish(&mut self, h: HWND, ok: bool) {
        let mut r = self.rect;
        if ok {
            r.left += self.origin_x;
            r.right += self.origin_x;
            r.top += self.origin_y;
            r.bottom += self.origin_y;
        }
        (*self.app).on_region_picked(ok, &r);
        DestroyWindow(h);
    }

    unsafe fn handle(&mut self, h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match m {
            WM_CREATE => {
                SetLayeredWindowAttributes(h, 0, (255.0 * 0.35) as u8, LWA_ALPHA);
                self.font_text = make_font(16, false, "Microsoft YaHei");
                ShowWindow(h, SW_SHOW);
                SetForegroundWindow(h);
                SetFocus(h);
                0
            }
            WM_LBUTTONDOWN => {
                self.dragging = true;
                self.has_rect = false;
                self.start.x = l as i16 as i32;
                self.start.y = (l >> 16) as i16 as i32;
                SetCapture(h);
                0
            }
            WM_MOUSEMOVE => {
                if self.dragging {
                    let x = l as i16 as i32;
                    let y = (l >> 16) as i16 as i32;
                    self.rect.left = self.start.x.min(x);
                    self.rect.right = self.start.x.max(x);
                    self.rect.top = self.start.y.min(y);
                    self.rect.bottom = self.start.y.max(y);
                    self.has_rect = true;
                    InvalidateRect(h, null(), 0);
                }
                0
            }
            WM_LBUTTONUP => {
                self.dragging = false;
                ReleaseCapture();
                // 矩形太小视为误操作
                let ok = self.has_rect
                    && (self.rect.right - self.rect.left) >= 10
                    && (self.rect.bottom - self.rect.top) >= 10;
                self.finish(h, ok);
                0
            }
            WM_KEYDOWN => {
                if w == VK_ESCAPE as usize {
                    self.finish(h, false);
                }
                0
            }
            WM_CAPTURECHANGED => {
                self.dragging = false;
                0
            }
            WM_PAINT => {
                let mut ps: PAINTSTRUCT = std::mem::zeroed();
                let hdc = BeginPaint(h, &mut ps);
                let mut rc: RECT = std::mem::zeroed();
                GetClientRect(h, &mut rc);
                let w_px = rc.right - rc.left;
                let h_px = rc.bottom - rc.top;
                // 双缓冲，避免拖拽时闪烁
                let mem = CreateCompatibleDC(hdc);
                let bmp = CreateCompatibleBitmap(hdc, w_px, h_px);
                let old_bmp = SelectObject(mem, bmp);
                FillRect(mem, &rc, GetStockObject(BLACK_BRUSH) as HBRUSH);

                SetBkMode(mem, TRANSPARENT as i32);
                SetTextColor(mem, 255 | (255 << 8) | (255 << 16));
                let old_font = SelectObject(mem, self.font_text);
                let mut tr = RECT { left: 0, top: 90, right: w_px, bottom: 200 };
                let text = to_wide(
                    "按住鼠标左键拖拽，框选出抖音直播间的输入框\n松开鼠标完成框选，按 Esc 取消",
                );
                DrawTextW(mem, text.as_ptr(), -1, &mut tr, DT_CENTER | DT_WORDBREAK);

                if self.has_rect {
                    let pen = CreatePen(PS_SOLID, 3, 255 | (59 << 8) | (48 << 16));
                    let old_pen = SelectObject(mem, pen);
                    let old_brush = SelectObject(mem, GetStockObject(NULL_BRUSH));
                    Rectangle(
                        mem,
                        self.rect.left,
                        self.rect.top,
                        self.rect.right,
                        self.rect.bottom,
                    );
                    SelectObject(mem, old_pen);
                    SelectObject(mem, old_brush);
                    DeleteObject(pen);
                }
                SelectObject(mem, old_font);

                BitBlt(hdc, 0, 0, w_px, h_px, mem, 0, 0, SRCCOPY);
                SelectObject(mem, old_bmp);
                DeleteObject(bmp);
                DeleteDC(mem);
                EndPaint(h, &ps);
                0
            }
            WM_DESTROY => {
                if !self.font_text.is_null() {
                    DeleteObject(self.font_text);
                    self.font_text = null_mut();
                }
                0
            }
            _ => DefWindowProcW(h, m, w, l),
        }
    }
}

unsafe extern "system" fn overlay_wnd_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let mut this = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut OverlayState;
    if m == WM_NCCREATE {
        let cs = &*(l as *const CREATESTRUCTW);
        this = cs.lpCreateParams as *mut OverlayState;
        SetWindowLongPtrW(h, GWLP_USERDATA, this as isize);
    }
    if this.is_null() {
        return DefWindowProcW(h, m, w, l);
    }
    let r = (*this).handle(h, m, w, l);
    if m == WM_NCDESTROY {
        drop(Box::from_raw(this)); // 对应 C++ 版 WM_NCDESTROY 里的 delete this
        SetWindowLongPtrW(h, GWLP_USERDATA, 0);
    }
    r
}

unsafe fn start_overlay(hinst: HINSTANCE, owner: HWND, app: *mut DanmuApp) {
    // 注册窗口类（重复注册会失败，属正常，忽略返回值）
    let class = to_wide(OVERLAY_CLASS);
    let wc = WNDCLASSW {
        style: 0,
        lpfnWndProc: Some(overlay_wnd_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: hinst,
        hIcon: null_mut(),
        hCursor: LoadCursorW(null_mut(), IDC_CROSS),
        hbrBackground: GetStockObject(BLACK_BRUSH) as HBRUSH,
        lpszMenuName: null(),
        lpszClassName: class.as_ptr(),
    };
    RegisterClassW(&wc);

    let vs = win_input::get_virtual_screen();
    let state = Box::new(OverlayState {
        app,
        origin_x: vs.left,
        origin_y: vs.top,
        dragging: false,
        has_rect: false,
        start: POINT { x: 0, y: 0 },
        rect: RECT { left: 0, top: 0, right: 0, bottom: 0 },
        font_text: null_mut(),
    });
    CreateWindowExW(
        WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
        class.as_ptr(),
        to_wide("").as_ptr(),
        WS_POPUP,
        vs.left,
        vs.top,
        vs.right - vs.left,
        vs.bottom - vs.top,
        owner,
        null_mut(),
        hinst,
        Box::into_raw(state) as *const _,
    );
}

// ================= 创建控件的辅助器 =================

struct UIBuilder {
    parent: HWND,
    inst: HINSTANCE,
}

impl UIBuilder {
    unsafe fn mk(
        &self,
        cls: &str,
        text: &str,
        style: u32,
        x: i32,
        y: i32,
        w: i32,
        h: i32,
        id: i32,
        ex: u32,
    ) -> HWND {
        CreateWindowExW(
            ex,
            to_wide(cls).as_ptr(),
            to_wide(text).as_ptr(),
            style | WS_CHILD | WS_VISIBLE,
            x,
            y,
            w,
            h,
            self.parent,
            id as isize as HMENU,
            self.inst,
            null(),
        )
    }
    unsafe fn group(&self, text: &str, x: i32, y: i32, w: i32, h: i32) -> HWND {
        self.mk("BUTTON", text, BS_GROUPBOX as u32, x, y, w, h, 0, 0)
    }
    unsafe fn label(&self, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32, extra: u32) -> HWND {
        self.mk("STATIC", text, extra, x, y, w, h, id, 0)
    }
    unsafe fn button(&self, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
        self.mk("BUTTON", text, BS_PUSHBUTTON as u32 | WS_TABSTOP, x, y, w, h, id, 0)
    }
    unsafe fn edit(&self, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
        self.mk("EDIT", "", ES_AUTOHSCROLL as u32 | WS_TABSTOP, x, y, w, h, id, WS_EX_CLIENTEDGE)
    }
    unsafe fn check(&self, text: &str, x: i32, y: i32, w: i32, h: i32, id: i32) -> HWND {
        self.mk("BUTTON", text, BS_AUTOCHECKBOX as u32 | WS_TABSTOP, x, y, w, h, id, 0)
    }
}

// ================= DanmuApp =================

pub struct DanmuApp {
    hwnd: HWND,
    hinst: HINSTANCE,
    #[allow(dead_code)]
    app_dir: PathBuf,
    danmu_dir: PathBuf,
    config_path: PathBuf,
    cfg: Arc<AppConfig>,
    loader: Arc<DanmuLoader>,
    worker: Option<SenderWorker>,
    total_sent: i32,
    updating_ui: bool,
    region_picking: bool,

    // 控件句柄
    h_file_combo: HWND,
    h_file_info: HWND,
    h_interval: HWND,
    h_jitter: HWND,
    h_shuffle: HWND,
    h_fg_check: HWND,
    h_fg_keyword: HWND,
    h_region: HWND,
    h_cursor: HWND,
    h_btn_start: HWND,
    h_btn_stop: HWND,
    h_status: HWND,
    h_progress: HWND,
    h_custom: HWND,
    h_log: HWND,

    font: HFONT,
    font_bold: HFONT,
    font_log: HFONT,
    status_color: COLORREF,
    cursor_color: COLORREF,
}

impl DanmuApp {
    pub fn new(app_dir: PathBuf) -> AppResult<Box<DanmuApp>> {
        unsafe {
            let hinst = GetModuleHandleW(null());
            let cfg = Arc::new(AppConfig::new());
            let config_path = app_dir.join("config.json");
            load_config(&cfg, &config_path);

            let mut app = Box::new(DanmuApp {
                hwnd: null_mut(),
                hinst,
                danmu_dir: app_dir.join("danmu"),
                config_path,
                cfg,
                loader: Arc::new(DanmuLoader::new()),
                worker: None,
                total_sent: 0,
                updating_ui: false,
                region_picking: false,
                h_file_combo: null_mut(),
                h_file_info: null_mut(),
                h_interval: null_mut(),
                h_jitter: null_mut(),
                h_shuffle: null_mut(),
                h_fg_check: null_mut(),
                h_fg_keyword: null_mut(),
                h_region: null_mut(),
                h_cursor: null_mut(),
                h_btn_start: null_mut(),
                h_btn_stop: null_mut(),
                h_status: null_mut(),
                h_progress: null_mut(),
                h_custom: null_mut(),
                h_log: null_mut(),
                font: null_mut(),
                font_bold: null_mut(),
                font_log: null_mut(),
                status_color: K_GRAY,
                cursor_color: K_GRAY,
                app_dir,
            });

            // 注册窗口类（图标可能因清单未嵌入而为空，属正常）
            let class = to_wide(MAIN_CLASS);
            let icon = LoadIconW(hinst, IDI_APPICON as PCWSTR);
            let wc = WNDCLASSW {
                style: 0,
                lpfnWndProc: Some(wnd_proc),
                cbClsExtra: 0,
                cbWndExtra: 0,
                hInstance: hinst,
                hIcon: icon,
                hCursor: LoadCursorW(null_mut(), IDC_ARROW),
                hbrBackground: (COLOR_BTNFACE as usize + 1) as HBRUSH,
                lpszMenuName: null(),
                lpszClassName: class.as_ptr(),
            };
            if RegisterClassW(&wc) == 0 {
                return Err(AppError::new("注册窗口类失败"));
            }

            let style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
            let mut rc = RECT { left: 0, top: 0, right: 700, bottom: 770 };
            AdjustWindowRect(&mut rc, style, 0);
            let title = to_wide("抖音直播自动弹幕助手");
            let hwnd = CreateWindowExW(
                0,
                class.as_ptr(),
                title.as_ptr(),
                style,
                CW_USEDEFAULT as i32,
                CW_USEDEFAULT as i32,
                rc.right - rc.left,
                rc.bottom - rc.top,
                null_mut(),
                null_mut(),
                hinst,
                &mut *app as *mut DanmuApp as *const _,
            );
            if hwnd.is_null() {
                return Err(AppError::new("创建主窗口失败"));
            }
            app.hwnd = hwnd;
            // WNDCLASSW 没有 hIconSm，标题栏小图标用 WM_SETICON 单独设置
            if !icon.is_null() {
                SendMessageW(hwnd, WM_SETICON, ICON_SMALL as WPARAM, icon as LPARAM);
            }

            app.font = make_font(9, false, "Microsoft YaHei");
            app.font_bold = make_font(11, true, "Microsoft YaHei");
            // 日志不能用 Consolas：无中文字形，中文会显示成方块（乱码）
            app.font_log = make_font(9, false, "Microsoft YaHei");

            app.build_ui();
            app.apply_config_to_ui();
            app.refresh_files();
            SetTimer(hwnd, IDT_CURSOR, 150, None);
            Ok(app)
        }
    }

    pub fn run(&mut self) -> i32 {
        unsafe {
            ShowWindow(self.hwnd, SW_SHOW);
            UpdateWindow(self.hwnd);
            let mut msg: MSG = std::mem::zeroed();
            while GetMessageW(&mut msg, null_mut(), 0, 0) > 0 {
                TranslateMessage(&msg);
                DispatchMessageW(&msg);
            }
            msg.wParam as i32
        }
    }

    // ---------- 界面搭建 ----------
    unsafe fn build_ui(&mut self) {
        let ui = UIBuilder { parent: self.hwnd, inst: self.hinst };

        // ---- 弹幕文件 ----
        ui.group("弹幕文件（danmu 文件夹）", 10, 8, 680, 78);
        self.h_file_combo = ui.mk(
            "COMBOBOX",
            "",
            CBS_DROPDOWNLIST as u32 | WS_VSCROLL | WS_TABSTOP,
            22,
            30,
            300,
            200,
            IDC_COMBO_FILE,
            0,
        );
        ui.button("导入", 330, 28, 56, 24, IDC_BTN_IMPORT);
        ui.button("刷新", 392, 28, 56, 24, IDC_BTN_REFRESH);
        ui.button("打开文件夹", 454, 28, 90, 24, IDC_BTN_OPENFOLDER);
        self.h_file_info = ui.label("条目数: -", 22, 58, 400, 18, IDC_STATIC_FILEINFO, 0);

        // ---- 发送设置 ----
        ui.group("发送设置", 10, 92, 680, 96);
        ui.label("发送间隔（秒）", 22, 116, 92, 18, 0, 0);
        self.h_interval = ui.edit(116, 113, 52, 22, IDC_EDIT_INTERVAL);
        ui.label("随机抖动 ±", 178, 116, 72, 18, 0, 0);
        self.h_jitter = ui.edit(252, 113, 42, 22, IDC_EDIT_JITTER);
        ui.label("%", 298, 116, 14, 18, 0, 0);
        self.h_shuffle = ui.check("每轮随机打乱", 322, 113, 130, 22, IDC_CHK_SHUFFLE);
        self.h_fg_check = ui.check("仅当前台窗口标题包含", 22, 142, 160, 22, IDC_CHK_FGCHECK);
        self.h_fg_keyword = ui.edit(186, 140, 80, 22, IDC_EDIT_FGKEYWORD);
        ui.label("时才发送（防误发到其他程序）", 272, 142, 190, 18, IDC_STATIC_FGNOTE, 0);
        ui.label(
            "提示：抖音直播发言太快会被限制（提示「发言太快」），建议间隔 ≥ 2~3 秒",
            22,
            166,
            650,
            16,
            IDC_STATIC_HINT,
            0,
        );

        // ---- 输入框区域 ----
        ui.group("输入框区域（光标检测）", 10, 194, 680, 72);
        self.h_region = ui.label("未设置", 22, 216, 360, 18, IDC_STATIC_REGION, 0);
        ui.button("框选输入框区域", 440, 212, 130, 26, IDC_BTN_PICKREGION);
        ui.button("清除", 578, 212, 60, 26, IDC_BTN_CLEARREGION);
        self.h_cursor = ui.label("光标: (0, 0)", 22, 240, 400, 18, IDC_STATIC_CURSOR, 0);

        // ---- 控制 ----
        self.h_btn_start = ui.button("▶ 开始发送", 10, 274, 140, 36, IDC_BTN_START);
        self.h_btn_stop = ui.button("■ 停止", 158, 274, 140, 36, IDC_BTN_STOP);
        EnableWindow(self.h_btn_stop, 0);
        self.h_status = ui.label("● 空闲", 308, 283, 240, 18, IDC_STATIC_STATUS, 0);
        self.h_progress =
            ui.label("本循环 -/- · 累计 0 条", 470, 283, 220, 18, IDC_STATIC_PROGRESS, SS_RIGHT);

        // ---- 自定义弹幕 ----
        ui.group(
            "自定义弹幕（追加到当前文件，立即生效，自动去重）",
            10,
            318,
            680,
            60,
        );
        self.h_custom = ui.edit(22, 340, 510, 24, IDC_EDIT_CUSTOM);
        ui.button("添加到当前文件", 540, 338, 140, 26, IDC_BTN_ADDCUSTOM);
        // 子类化：回车 = 添加。旧 WndProc 存进控件的 GWLP_USERDATA
        let old =
            SetWindowLongPtrW(self.h_custom, GWLP_WNDPROC, custom_edit_proc as *const () as isize);
        SetWindowLongPtrW(self.h_custom, GWLP_USERDATA, old);

        // ---- 日志 ----
        ui.group("日志", 10, 384, 680, 376);
        self.h_log = ui.mk(
            "LISTBOX",
            "",
            LBS_NOINTEGRALHEIGHT as u32 | WS_VSCROLL | WS_HSCROLL | WS_BORDER,
            20,
            404,
            660,
            344,
            IDC_LIST_LOG,
            0,
        );
        SendMessageW(self.h_log, LB_SETHORIZONTALEXTENT, 1500, 0);

        // 统一字体：普通控件微软雅黑，启停按钮加粗，日志用雅黑（等宽字体无中文字形）
        EnumChildWindows(self.hwnd, Some(set_font_cb), self.font as LPARAM);
        SendMessageW(self.h_btn_start, WM_SETFONT, self.font_bold as WPARAM, 1);
        SendMessageW(self.h_btn_stop, WM_SETFONT, self.font_bold as WPARAM, 1);
        SendMessageW(self.h_log, WM_SETFONT, self.font_log as WPARAM, 1);
    }

    unsafe fn apply_config_to_ui(&mut self) {
        self.updating_ui = true;
        let snap = self.cfg.snapshot();
        set_text(self.h_interval, &format_g(snap.interval));
        set_text(self.h_jitter, &snap.jitter.to_string());
        SendMessageW(
            self.h_shuffle,
            BM_SETCHECK,
            if snap.shuffle { BST_CHECKED } else { BST_UNCHECKED } as WPARAM,
            0,
        );
        SendMessageW(
            self.h_fg_check,
            BM_SETCHECK,
            if snap.fg_check { BST_CHECKED } else { BST_UNCHECKED } as WPARAM,
            0,
        );
        set_text(self.h_fg_keyword, &snap.fg_keyword);
        self.updating_ui = false;
        self.update_region_label();
    }

    unsafe fn update_region_label(&mut self) {
        let snap = self.cfg.snapshot();
        if snap.has_region {
            set_text(
                self.h_region,
                &format!(
                    "({}, {}) → ({}, {})",
                    snap.region.left, snap.region.top, snap.region.right, snap.region.bottom
                ),
            );
        } else {
            set_text(self.h_region, "未设置");
        }
    }

    // ---------- 弹幕文件管理 ----------
    unsafe fn refresh_files(&mut self) {
        let _ = std::fs::create_dir_all(&self.danmu_dir);
        let mut files: Vec<String> = Vec::new();
        if let Ok(rd) = std::fs::read_dir(&self.danmu_dir) {
            for e in rd.flatten() {
                let p = e.path();
                if p.is_file()
                    && p.extension()
                        .map(|x| x.to_string_lossy().eq_ignore_ascii_case("json"))
                        .unwrap_or(false)
                {
                    if let Some(name) = p.file_name() {
                        files.push(name.to_string_lossy().into_owned());
                    }
                }
            }
        }
        files.sort();

        SendMessageW(self.h_file_combo, CB_RESETCONTENT, 0, 0);
        for f in &files {
            let w = to_wide(f);
            SendMessageW(self.h_file_combo, CB_ADDSTRING, 0, w.as_ptr() as LPARAM);
        }

        let cur = self.cfg.snapshot().current_file;
        let mut sel: LRESULT = -1;
        if !cur.is_empty() {
            let w = to_wide(&cur);
            sel = SendMessageW(self.h_file_combo, CB_FINDSTRINGEXACT, (-1isize) as WPARAM, w.as_ptr() as LPARAM);
        }
        if sel < 0 && !files.is_empty() {
            sel = 0;
        }
        SendMessageW(self.h_file_combo, CB_SETCURSEL, sel as WPARAM, 0);
        self.on_file_changed();
    }

    unsafe fn on_file_changed(&mut self) {
        let sel = SendMessageW(self.h_file_combo, CB_GETCURSEL, 0, 0);
        if sel < 0 {
            self.loader.clear();
            set_text(self.h_file_info, "条目数: -");
            return;
        }
        let mut buf = [0u16; 260];
        SendMessageW(self.h_file_combo, CB_GETLBTEXT, sel as WPARAM, buf.as_mut_ptr() as LPARAM);
        let name = from_wide_ptr(buf.as_ptr());
        let path = self.danmu_dir.join(&name);
        if let Err(e) = self.loader.load(path) {
            self.loader.clear();
            set_text(self.h_file_info, "条目数: 加载失败");
            MessageBoxW(
                self.hwnd,
                to_wide(&e.0).as_ptr(),
                to_wide("弹幕文件加载失败").as_ptr(),
                MB_ICONERROR,
            );
            return;
        }
        self.cfg.data.lock().unwrap().current_file = name.clone();
        self.save_config();
        let size = self.loader.size();
        set_text(self.h_file_info, &format!("条目数: {}", size));
        self.log(&format!("已加载弹幕文件 {}（{} 条）", name, size));
    }

    unsafe fn import_file(&mut self) {
        let mut file_buf = [0u16; 260];
        let filter = to_wide("JSON 文件 (*.json)\0*.json\0所有文件 (*.*)\0*.*\0");
        let title = to_wide("选择弹幕 JSON 文件");
        let mut ofn: OPENFILENAMEW = std::mem::zeroed();
        ofn.lStructSize = std::mem::size_of::<OPENFILENAMEW>() as u32;
        ofn.hwndOwner = self.hwnd;
        ofn.lpstrFilter = filter.as_ptr();
        ofn.lpstrFile = file_buf.as_mut_ptr();
        ofn.nMaxFile = file_buf.len() as u32;
        ofn.lpstrTitle = title.as_ptr();
        ofn.Flags = OFN_FILEMUSTEXIST | OFN_HIDEREADONLY;
        if GetOpenFileNameW(&mut ofn) == 0 {
            return;
        }

        let src_str = from_wide_ptr(file_buf.as_ptr());
        let src = PathBuf::from(&src_str);
        let count = match load_danmu(&src) {
            // 仅校验格式
            Ok(items) => items.len(),
            Err(e) => {
                MessageBoxW(
                    self.hwnd,
                    to_wide(&e.0).as_ptr(),
                    to_wide("导入失败").as_ptr(),
                    MB_ICONERROR,
                );
                return;
            }
        };

        let base = match src.file_name() {
            Some(n) => n.to_string_lossy().into_owned(),
            None => return,
        };
        let target = self.danmu_dir.join(&base);
        if target.exists()
            && !src_str
                .replace('/', "\\")
                .eq_ignore_ascii_case(&target.to_string_lossy().replace('/', "\\"))
        {
            if MessageBoxW(
                self.hwnd,
                to_wide("弹幕文件夹中已存在同名文件，是否覆盖？").as_ptr(),
                to_wide("覆盖确认").as_ptr(),
                MB_YESNO | MB_ICONQUESTION,
            ) != IDYES
            {
                return;
            }
        }
        let _ = std::fs::create_dir_all(&self.danmu_dir);
        if std::fs::copy(&src, &target).is_err() {
            MessageBoxW(
                self.hwnd,
                to_wide("复制文件出错").as_ptr(),
                to_wide("导入失败").as_ptr(),
                MB_ICONERROR,
            );
            return;
        }
        self.cfg.data.lock().unwrap().current_file = base.clone();
        self.save_config();
        self.refresh_files();
        self.log(&format!("已导入弹幕文件 {}（{} 条）", base, count));
    }

    unsafe fn open_folder(&mut self) {
        let _ = std::fs::create_dir_all(&self.danmu_dir);
        ShellExecuteW(
            self.hwnd,
            to_wide("explore").as_ptr(),
            path_wide(&self.danmu_dir).as_ptr(),
            null(),
            null(),
            SW_SHOW,
        );
    }

    // ---------- 输入框区域 ----------
    unsafe fn pick_region(&mut self) {
        if self.worker.as_ref().map(|w| w.running()).unwrap_or(false) {
            MessageBoxW(
                self.hwnd,
                to_wide("请先停止发送，再重新框选区域").as_ptr(),
                to_wide("提示").as_ptr(),
                MB_ICONINFORMATION,
            );
            return;
        }
        if self.region_picking {
            return;
        }
        self.region_picking = true;
        start_overlay(self.hinst, self.hwnd, self);
    }

    unsafe fn on_region_picked(&mut self, ok: bool, r: &RECT) {
        self.region_picking = false;
        if ok {
            {
                let mut d = self.cfg.data.lock().unwrap();
                d.region.left = r.left;
                d.region.top = r.top;
                d.region.right = r.right;
                d.region.bottom = r.bottom;
                d.has_region = true;
            }
            self.update_region_label();
            self.save_config();
            self.log(&format!(
                "输入框区域已设置：({}, {}) → ({}, {})",
                r.left, r.top, r.right, r.bottom
            ));
        } else {
            self.log("已取消框选");
        }
    }

    unsafe fn clear_region(&mut self) {
        self.cfg.data.lock().unwrap().has_region = false;
        self.update_region_label();
        self.save_config();
        self.log("已清除输入框区域");
    }

    // ---------- 启停 ----------
    unsafe fn on_start(&mut self) {
        if self.worker.as_ref().map(|w| w.running()).unwrap_or(false) {
            return;
        }
        if self.loader.path().is_none() {
            MessageBoxW(
                self.hwnd,
                to_wide("请先选择弹幕文件").as_ptr(),
                to_wide("无法开始").as_ptr(),
                MB_ICONWARNING,
            );
            return;
        }
        // 先取快照再弹窗（快照模式天然避免了 C++ 版「持锁弹窗自我死锁」的问题）
        let snap = self.cfg.snapshot();
        if !snap.has_region {
            MessageBoxW(
                self.hwnd,
                to_wide("请先点击「框选输入框区域」，框出抖音直播间的输入框").as_ptr(),
                to_wide("无法开始").as_ptr(),
                MB_ICONWARNING,
            );
            return;
        }
        let mut interval = get_text(self.h_interval).parse::<f64>().unwrap_or(0.0);
        if interval < MIN_INTERVAL {
            interval = MIN_INTERVAL;
            self.updating_ui = true;
            set_text(self.h_interval, &format_g(interval));
            self.updating_ui = false;
        }
        self.cfg.data.lock().unwrap().interval = interval;

        self.total_sent = 0;
        set_text(self.h_progress, "本循环 -/- · 累计 0 条");
        self.set_running(true);
        self.log(&format!(
            "开始发送：间隔 {:.1} 秒 ±{}% 抖动，把鼠标点进抖音输入框即自动发送",
            interval, snap.jitter
        ));
        self.worker = Some(SenderWorker::start(
            Arc::clone(&self.loader),
            Arc::clone(&self.cfg),
            self.hwnd,
        ));
    }

    unsafe fn on_stop(&mut self) {
        if let Some(w) = &self.worker {
            if w.running() {
                w.stop();
                // 不用 ⏹/⏸ 等符号：微软雅黑缺少这些字形会显示成方块
                self.set_status("正在停止…", K_ORANGE);
                self.log("正在停止…");
            }
        }
    }

    unsafe fn set_running(&mut self, running: bool) {
        EnableWindow(self.h_btn_start, (!running) as BOOL);
        EnableWindow(self.h_btn_stop, running as BOOL);
        EnableWindow(self.h_file_combo, (!running) as BOOL);
        if running {
            self.set_status("▶ 发送中：等待光标进入输入框…", K_GREEN);
        } else {
            self.set_status("● 空闲", K_GRAY);
        }
    }

    // ---------- 自定义条目 ----------
    pub unsafe fn add_custom(&mut self) {
        if self.loader.path().is_none() {
            MessageBoxW(
                self.hwnd,
                to_wide("请先选择弹幕文件").as_ptr(),
                to_wide("无法添加").as_ptr(),
                MB_ICONWARNING,
            );
            return;
        }
        let text = get_text(self.h_custom);
        let text = text.trim().to_string();
        if text.is_empty() {
            return;
        }
        if let Err(err) = self.loader.add(&text) {
            MessageBoxW(
                self.hwnd,
                to_wide(&err).as_ptr(),
                to_wide("提示").as_ptr(),
                MB_ICONINFORMATION,
            );
            return;
        }
        set_text(self.h_custom, "");
        set_text(self.h_file_info, &format!("条目数: {}", self.loader.size()));
        self.log(&format!("已添加自定义弹幕：{}", text));
    }

    // ---------- 设置实时生效 ----------
    unsafe fn on_setting_changed(&mut self) {
        if self.updating_ui {
            return;
        }
        {
            let mut d = self.cfg.data.lock().unwrap();
            if let Ok(iv) = get_text(self.h_interval).parse::<f64>() {
                if iv > 0.0 {
                    d.interval = iv;
                }
            }
            if let Ok(j) = get_text(self.h_jitter).parse::<i32>() {
                if (0..=50).contains(&j) {
                    d.jitter = j;
                }
            }
            d.shuffle = SendMessageW(self.h_shuffle, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
            d.fg_check = SendMessageW(self.h_fg_check, BM_GETCHECK, 0, 0) == BST_CHECKED as LRESULT;
            let kw = get_text(self.h_fg_keyword).trim().to_string();
            d.fg_keyword = if kw.is_empty() { "抖音".to_string() } else { kw };
        }
        self.save_config();
    }

    // ---------- 状态 / 日志 ----------
    unsafe fn set_status(&mut self, text: &str, color: COLORREF) {
        self.status_color = color;
        set_text(self.h_status, text);
        InvalidateRect(self.h_status, null(), 0);
    }

    unsafe fn update_cursor_status(&mut self) {
        let pos = win_input::get_cursor_pos();
        let snap = self.cfg.snapshot();
        let in_region = crate::sender::point_in_region(pos, snap.has_region, &snap.region);
        let text = if in_region {
            format!("光标: ({}, {})  ● 在输入框内", pos.x, pos.y)
        } else {
            format!("光标: ({}, {})  ○ 不在输入框内", pos.x, pos.y)
        };
        self.cursor_color = if in_region { K_GREEN } else { K_ORANGE };
        set_text(self.h_cursor, &text);
        InvalidateRect(self.h_cursor, null(), 0);
    }

    unsafe fn log(&mut self, text: &str) {
        let mut st: SYSTEMTIME = std::mem::zeroed();
        GetLocalTime(&mut st);
        let line = format!(
            "{:02}:{:02}:{:02}  {}",
            st.wHour, st.wMinute, st.wSecond, text
        );
        // 控制日志行数，避免无限增长
        if SendMessageW(self.h_log, LB_GETCOUNT, 0, 0) > 500 {
            for _ in 0..100 {
                SendMessageW(self.h_log, LB_DELETESTRING, 0, 0);
            }
        }
        let w = to_wide(&line);
        let idx = SendMessageW(self.h_log, LB_ADDSTRING, 0, w.as_ptr() as LPARAM);
        SendMessageW(self.h_log, LB_SETCURSEL, idx as WPARAM, 0); // 滚动到底部
    }

    unsafe fn save_config(&self) {
        save_config(&self.cfg, &self.config_path);
    }

    // ---------- 消息处理 ----------
    unsafe fn on_command(&mut self, id: i32, code: u32) {
        match id {
            IDC_COMBO_FILE => {
                if code == CBN_SELCHANGE {
                    self.on_file_changed();
                }
            }
            IDC_BTN_IMPORT => self.import_file(),
            IDC_BTN_REFRESH => self.refresh_files(),
            IDC_BTN_OPENFOLDER => self.open_folder(),
            IDC_BTN_PICKREGION => self.pick_region(),
            IDC_BTN_CLEARREGION => self.clear_region(),
            IDC_BTN_START => self.on_start(),
            IDC_BTN_STOP => self.on_stop(),
            IDC_BTN_ADDCUSTOM => self.add_custom(),
            IDC_EDIT_INTERVAL | IDC_EDIT_JITTER | IDC_EDIT_FGKEYWORD => {
                if code == EN_CHANGE {
                    self.on_setting_changed();
                }
            }
            IDC_CHK_SHUFFLE | IDC_CHK_FGCHECK => {
                if code == BN_CLICKED as u32 {
                    self.on_setting_changed();
                }
            }
            _ => {}
        }
    }

    unsafe fn on_message(&mut self, hwnd: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
        match m {
            WM_COMMAND => {
                self.on_command((w & 0xFFFF) as i32, ((w >> 16) & 0xFFFF) as u32);
                0
            }
            WM_TIMER => {
                if w == IDT_CURSOR {
                    self.update_cursor_status();
                }
                0
            }
            WM_CTLCOLORSTATIC => {
                let hdc = w as HDC;
                let id = GetDlgCtrlID(l as HWND);
                if id == IDC_STATIC_CURSOR {
                    SetTextColor(hdc, self.cursor_color);
                } else if id == IDC_STATIC_STATUS {
                    SetTextColor(hdc, self.status_color);
                } else if id == IDC_STATIC_FILEINFO
                    || id == IDC_STATIC_HINT
                    || id == IDC_STATIC_FGNOTE
                    || id == IDC_STATIC_PROGRESS
                {
                    SetTextColor(hdc, K_GRAY);
                } else {
                    return DefWindowProcW(hwnd, m, w, l);
                }
                SetBkMode(hdc, TRANSPARENT as i32);
                GetSysColorBrush(COLOR_BTNFACE as i32) as LRESULT
            }
            WM_DANMU_SENT => {
                let info = Box::from_raw(l as *mut SentInfo); // 回收工作线程分配的负载
                self.total_sent += 1;
                self.status_color = K_GREEN;
                set_text(
                    self.h_status,
                    &format!("▶ 发送中（第 {}/{} 条）", info.idx, info.total),
                );
                set_text(
                    self.h_progress,
                    &format!("本循环 {}/{} · 累计 {} 条", info.idx, info.total, self.total_sent),
                );
                self.log(&info.text);
                0
            }
            WM_DANMU_PAUSED => {
                let x = l as i16 as i32;
                let y = (l >> 16) as i16 as i32;
                self.status_color = K_ORANGE;
                set_text(
                    self.h_status,
                    &format!("已暂停：光标不在输入框区域内 ({}, {})", x, y),
                );
                0
            }
            WM_DANMU_FGBLOCKED => {
                let title = Box::from_raw(l as *mut String);
                self.set_status(
                    &format!("已暂停：前台窗口不是抖音（当前：{}）", *title),
                    K_ORANGE,
                );
                0
            }
            WM_DANMU_ERROR => {
                let err = Box::from_raw(l as *mut String);
                self.log(&format!("发送出错：{}", *err));
                MessageBoxW(
                    hwnd,
                    to_wide(&err).as_ptr(),
                    to_wide("发送出错").as_ptr(),
                    MB_ICONERROR,
                );
                0
            }
            WM_DANMU_STOPPED => {
                if let Some(mut w) = self.worker.take() {
                    w.join();
                }
                self.set_running(false);
                self.log(&format!("已停止（本轮累计发送 {} 条）", self.total_sent));
                0
            }
            WM_CLOSE => {
                if let Some(mut w) = self.worker.take() {
                    w.stop();
                    w.join();
                }
                self.save_config();
                DestroyWindow(hwnd);
                0
            }
            WM_DESTROY => {
                KillTimer(hwnd, IDT_CURSOR);
                PostQuitMessage(0);
                0
            }
            _ => DefWindowProcW(hwnd, m, w, l),
        }
    }
}

impl Drop for DanmuApp {
    fn drop(&mut self) {
        unsafe {
            if let Some(mut w) = self.worker.take() {
                w.stop();
                w.join();
            }
            for f in [self.font, self.font_bold, self.font_log] {
                if !f.is_null() {
                    DeleteObject(f);
                }
            }
        }
    }
}

/// 静态 WndProc：从 GWLP_USERDATA 取回 DanmuApp 指针再分发
unsafe extern "system" fn wnd_proc(h: HWND, m: u32, w: WPARAM, l: LPARAM) -> LRESULT {
    let mut this = GetWindowLongPtrW(h, GWLP_USERDATA) as *mut DanmuApp;
    if m == WM_NCCREATE {
        let cs = &*(l as *const CREATESTRUCTW);
        this = cs.lpCreateParams as *mut DanmuApp;
        SetWindowLongPtrW(h, GWLP_USERDATA, this as isize);
    }
    // 注意把 h 传下去：WM_NCCREATE 期间 app.hwnd 尚未赋值，
    // 若用成员里的 hwnd 调 DefWindowProcW 会返回 0，导致窗口创建被中止
    if this.is_null() {
        return DefWindowProcW(h, m, w, l);
    }
    (*this).on_message(h, m, w, l)
}

// ---------- 小工具 ----------

unsafe fn set_text(h: HWND, text: &str) {
    SetWindowTextW(h, to_wide(text).as_ptr());
}

unsafe fn get_text(h: HWND) -> String {
    let mut buf = [0u16; 512];
    let n = GetWindowTextW(h, buf.as_mut_ptr(), buf.len() as i32);
    String::from_utf16_lossy(&buf[..n.max(0) as usize])
}
