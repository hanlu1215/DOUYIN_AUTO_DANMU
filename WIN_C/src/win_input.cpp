// -*- coding: utf-8 -*-
#include "win_input.h"
#include "strutil.h"
#include <shellscalingapi.h>

namespace win_input {

void set_process_dpi_aware() {
    // 优先 Per-Monitor V2（Win10 1607+），退回 shcore 方案，最后 V1
    if (HMODULE user32 = GetModuleHandleW(L"user32.dll")) {
        typedef BOOL(WINAPI* SetDpiCtxFn)(DPI_AWARENESS_CONTEXT);
        auto proc = GetProcAddress(user32, "SetProcessDpiAwarenessContext");
        auto fn = (SetDpiCtxFn)(void*)proc;
        if (fn)
            if (fn(DPI_AWARENESS_CONTEXT_PER_MONITOR_AWARE_V2)) return;
    }
    if (SUCCEEDED(SetProcessDpiAwareness(PROCESS_PER_MONITOR_DPI_AWARE))) return;
    SetProcessDPIAware();
}

POINT get_cursor_pos() {
    POINT pt{};
    GetCursorPos(&pt);
    return pt;
}

RECT get_virtual_screen() {
    RECT rc{};
    rc.left = GetSystemMetrics(SM_XVIRTUALSCREEN);
    rc.top = GetSystemMetrics(SM_YVIRTUALSCREEN);
    rc.right = rc.left + GetSystemMetrics(SM_CXVIRTUALSCREEN);
    rc.bottom = rc.top + GetSystemMetrics(SM_CYVIRTUALSCREEN);
    return rc;
}

std::wstring get_foreground_title() {
    HWND hwnd = GetForegroundWindow();
    if (!hwnd) return L"";
    int len = GetWindowTextLengthW(hwnd);
    if (len <= 0) return L"";
    std::wstring title(len + 1, L'\0');
    GetWindowTextW(hwnd, &title[0], len + 1);
    title.resize(len);
    return title;
}

void set_clipboard(const std::wstring& text) {
    // 剪贴板可能被其他程序短暂占用，重试几次再报错
    bool opened = false;
    for (int k = 0; k < 10 && !opened; k++) {
        if (OpenClipboard(NULL)) opened = true;
        else Sleep(50);
    }
    if (!opened) throw AppError{ L"打开剪贴板失败（可能被其他程序占用）" };

    HGLOBAL handle = NULL;
    try {
        if (!EmptyClipboard()) throw AppError{ L"清空剪贴板失败" };
        SIZE_T bytes = (text.size() + 1) * sizeof(wchar_t);
        handle = GlobalAlloc(GMEM_MOVEABLE, bytes);
        if (!handle) throw AppError{ L"分配剪贴板内存失败" };
        void* p = GlobalLock(handle);
        if (!p) throw AppError{ L"锁定剪贴板内存失败" };
        memcpy(p, text.c_str(), bytes);
        GlobalUnlock(handle);
        if (!SetClipboardData(CF_UNICODETEXT, handle))
            throw AppError{ L"写入剪贴板数据失败" };
        // 成功后内存归系统所有，不能再 GlobalFree
        handle = NULL;
    } catch (...) {
        if (handle) GlobalFree(handle);
        CloseClipboard();
        throw;
    }
    CloseClipboard();
}

static void key_down(BYTE vk) { keybd_event(vk, 0, 0, 0); Sleep(40); }
static void key_up(BYTE vk) { keybd_event(vk, 0, KEYEVENTF_KEYUP, 0); Sleep(40); }
static void press(BYTE vk) { key_down(vk); key_up(vk); }

void paste() {
    key_down(VK_CONTROL);
    press('V');
    key_up(VK_CONTROL);
}

void press_enter() {
    press(VK_RETURN);
}

} // namespace win_input
