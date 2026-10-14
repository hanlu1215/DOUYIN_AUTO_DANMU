//! 原生 Win32 输入层（对应 C++ 版 win_input）：
//! 光标位置、虚拟屏幕范围、前台窗口标题、剪贴板写入（UTF-16）、keybd_event 按键模拟。

use windows_sys::Win32::Foundation::{GlobalFree, BOOL, HWND, POINT, RECT};
use windows_sys::Win32::System::DataExchange::{
    CloseClipboard, EmptyClipboard, OpenClipboard, SetClipboardData,
};
use windows_sys::Win32::System::LibraryLoader::{GetModuleHandleW, GetProcAddress};
use windows_sys::Win32::System::Memory::{GlobalAlloc, GlobalLock, GlobalUnlock, GMEM_MOVEABLE};
use windows_sys::Win32::System::Threading::Sleep;
use windows_sys::Win32::UI::HiDpi::{
    SetProcessDpiAwareness, DPI_AWARENESS_CONTEXT,
    DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2, PROCESS_PER_MONITOR_DPI_AWARE,
};
use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
    keybd_event, KEYEVENTF_KEYUP, VK_CONTROL, VK_RETURN,
};
use windows_sys::Win32::UI::WindowsAndMessaging::{
    GetCursorPos, GetForegroundWindow, GetSystemMetrics, GetWindowTextLengthW, GetWindowTextW,
    SetProcessDPIAware, SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN, SM_XVIRTUALSCREEN,
    SM_YVIRTUALSCREEN,
};

use crate::strutil::{to_wide, AppError, AppResult};

const CF_UNICODETEXT: u32 = 13;

/// 设置 DPI 感知：优先 Per-Monitor V2（Win10 1607+），退回 shcore 方案，最后 V1。
/// 必须在创建任何窗口前调用，否则 GetCursorPos 物理坐标与界面坐标对不上。
pub fn set_process_dpi_aware() {
    unsafe {
        type SetDpiCtxFn = unsafe extern "system" fn(DPI_AWARENESS_CONTEXT) -> BOOL;
        let user32 = GetModuleHandleW(to_wide("user32.dll").as_ptr());
        if !user32.is_null() {
            let proc = GetProcAddress(user32, b"SetProcessDpiAwarenessContext\0".as_ptr());
            if let Some(p) = proc {
                let f: SetDpiCtxFn = std::mem::transmute(p);
                if f(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2) != 0 {
                    return;
                }
            }
        }
        if SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE) >= 0 {
            return;
        }
        SetProcessDPIAware();
    }
}

pub fn get_cursor_pos() -> POINT {
    let mut pt = POINT { x: 0, y: 0 };
    unsafe { GetCursorPos(&mut pt) };
    pt
}

/// 虚拟屏幕（多显示器整体范围）
pub fn get_virtual_screen() -> RECT {
    unsafe {
        let left = GetSystemMetrics(SM_XVIRTUALSCREEN);
        let top = GetSystemMetrics(SM_YVIRTUALSCREEN);
        RECT {
            left,
            top,
            right: left + GetSystemMetrics(SM_CXVIRTUALSCREEN),
            bottom: top + GetSystemMetrics(SM_CYVIRTUALSCREEN),
        }
    }
}

pub fn get_foreground_title() -> String {
    unsafe {
        let hwnd: HWND = GetForegroundWindow();
        if hwnd.is_null() {
            return String::new();
        }
        let len = GetWindowTextLengthW(hwnd);
        if len <= 0 {
            return String::new();
        }
        let mut buf = vec![0u16; (len + 1) as usize];
        let n = GetWindowTextW(hwnd, buf.as_mut_ptr(), len + 1);
        String::from_utf16_lossy(&buf[..n as usize])
    }
}

/// 把文本写入系统剪贴板（CF_UNICODETEXT）。
/// 发送采用「剪贴板 + Ctrl+V」而非模拟逐字输入 —— 这是中文弹幕能正常发送的原因，
/// 也是 README 警告「运行期间剪贴板被占用」的原因。
pub fn set_clipboard(text: &str) -> AppResult<()> {
    unsafe {
        // 剪贴板可能被其他程序短暂占用，重试几次再报错
        let mut opened = false;
        for _ in 0..10 {
            if OpenClipboard(std::ptr::null_mut()) != 0 {
                opened = true;
                break;
            }
            Sleep(50);
        }
        if !opened {
            return Err(AppError::new("打开剪贴板失败（可能被其他程序占用）"));
        }

        let result = set_clipboard_inner(text);
        CloseClipboard();
        result
    }
}

unsafe fn set_clipboard_inner(text: &str) -> AppResult<()> {
    if EmptyClipboard() == 0 {
        return Err(AppError::new("清空剪贴板失败"));
    }
    let wide = to_wide(text);
    let bytes = wide.len() * std::mem::size_of::<u16>();
    let handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
    if handle.is_null() {
        return Err(AppError::new("分配剪贴板内存失败"));
    }
    let p = GlobalLock(handle);
    if p.is_null() {
        GlobalFree(handle);
        return Err(AppError::new("锁定剪贴板内存失败"));
    }
    std::ptr::copy_nonoverlapping(wide.as_ptr(), p as *mut u16, wide.len());
    GlobalUnlock(handle);
    if SetClipboardData(CF_UNICODETEXT, handle).is_null() {
        GlobalFree(handle);
        return Err(AppError::new("写入剪贴板数据失败"));
    }
    // 成功后内存归系统所有，不能再 GlobalFree
    Ok(())
}

unsafe fn key_down(vk: u16) {
    keybd_event(vk as u8, 0, 0, 0);
    Sleep(40);
}
unsafe fn key_up(vk: u16) {
    keybd_event(vk as u8, 0, KEYEVENTF_KEYUP, 0);
    Sleep(40);
}
unsafe fn press(vk: u16) {
    key_down(vk);
    key_up(vk);
}

/// Ctrl+V 粘贴
pub fn paste() {
    unsafe {
        key_down(VK_CONTROL);
        press(b'V' as u16);
        key_up(VK_CONTROL);
    }
}

/// 回车发送
pub fn press_enter() {
    unsafe { press(VK_RETURN) }
}
