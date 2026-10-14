# -*- coding: utf-8 -*-
"""Windows 输入层：光标位置 / 剪贴板 / 键盘模拟，全部基于 ctypes 实现，无第三方依赖。"""
import ctypes
import time
from ctypes import wintypes

user32 = ctypes.windll.user32
kernel32 = ctypes.windll.kernel32

# ---- 常量 ----
CF_UNICODETEXT = 13
GMEM_MOVEABLE = 0x0042

VK_CONTROL = 0x11
VK_V = 0x56
VK_RETURN = 0x0D
KEYEVENTF_KEYUP = 0x0002

# ---- 函数签名（64 位下必须显式声明，否则句柄会被截断为 32 位）----
kernel32.GlobalAlloc.restype = ctypes.c_void_p
kernel32.GlobalAlloc.argtypes = [ctypes.c_uint, ctypes.c_size_t]
kernel32.GlobalLock.restype = ctypes.c_void_p
kernel32.GlobalLock.argtypes = [ctypes.c_void_p]
kernel32.GlobalUnlock.restype = ctypes.c_int
kernel32.GlobalUnlock.argtypes = [ctypes.c_void_p]

user32.OpenClipboard.argtypes = [ctypes.c_void_p]
user32.OpenClipboard.restype = ctypes.c_int
user32.EmptyClipboard.restype = ctypes.c_int
user32.SetClipboardData.argtypes = [ctypes.c_uint, ctypes.c_void_p]
user32.SetClipboardData.restype = ctypes.c_void_p
user32.CloseClipboard.restype = ctypes.c_int
user32.keybd_event.argtypes = [ctypes.c_ubyte, ctypes.c_ubyte, ctypes.c_uint, ctypes.c_void_p]


class POINT(ctypes.Structure):
    _fields_ = [("x", wintypes.LONG), ("y", wintypes.LONG)]


def set_process_dpi_aware():
    """让进程使用物理像素坐标，保证 GetCursorPos 与 tkinter 坐标一致。"""
    try:
        ctypes.windll.shcore.SetProcessDpiAwareness(2)
    except Exception:
        try:
            user32.SetProcessDPIAware()
        except Exception:
            pass


def get_cursor_pos():
    """返回鼠标光标在屏幕上的物理坐标 (x, y)。"""
    pt = POINT()
    user32.GetCursorPos(ctypes.byref(pt))
    return pt.x, pt.y


def get_virtual_screen():
    """返回虚拟桌面（多显示器合并区域）的 (x, y, width, height)。"""
    SM_XVIRTUALSCREEN, SM_YVIRTUALSCREEN = 76, 77
    SM_CXVIRTUALSCREEN, SM_CYVIRTUALSCREEN = 78, 79
    return (
        user32.GetSystemMetrics(SM_XVIRTUALSCREEN),
        user32.GetSystemMetrics(SM_YVIRTUALSCREEN),
        user32.GetSystemMetrics(SM_CXVIRTUALSCREEN),
        user32.GetSystemMetrics(SM_CYVIRTUALSCREEN),
    )


def get_foreground_title():
    """返回当前前台窗口的标题，用于校验弹幕是否发往抖音窗口。"""
    hwnd = user32.GetForegroundWindow()
    if not hwnd:
        return ""
    length = user32.GetWindowTextLengthW(hwnd)
    if length <= 0:
        return ""
    buf = ctypes.create_unicode_buffer(length + 1)
    user32.GetWindowTextW(hwnd, buf, length + 1)
    return buf.value


def set_clipboard(text):
    """把文本写入系统剪贴板（UTF-16）。"""
    data = text.encode("utf-16-le") + b"\x00\x00"
    if not user32.OpenClipboard(None):
        raise RuntimeError("打开剪贴板失败（可能被其他程序占用）")
    try:
        if not user32.EmptyClipboard():
            raise RuntimeError("清空剪贴板失败")
        handle = kernel32.GlobalAlloc(GMEM_MOVEABLE, len(data))
        if not handle:
            raise RuntimeError("分配剪贴板内存失败")
        p = kernel32.GlobalLock(handle)
        if not p:
            raise RuntimeError("锁定剪贴板内存失败")
        ctypes.memmove(p, data, len(data))
        kernel32.GlobalUnlock(handle)
        # 成功后内存归系统所有，不能再 GlobalFree
        if not user32.SetClipboardData(CF_UNICODETEXT, handle):
            raise RuntimeError("写入剪贴板数据失败")
    finally:
        user32.CloseClipboard()


def _key_down(vk):
    user32.keybd_event(vk, 0, 0, None)
    time.sleep(0.04)


def _key_up(vk):
    user32.keybd_event(vk, 0, KEYEVENTF_KEYUP, None)
    time.sleep(0.04)


def press_key(vk):
    _key_down(vk)
    _key_up(vk)


def paste():
    """模拟 Ctrl+V，粘贴剪贴板内容。"""
    _key_down(VK_CONTROL)
    press_key(VK_V)
    _key_up(VK_CONTROL)


def press_enter():
    """模拟按回车（直播间输入框发送）。"""
    press_key(VK_RETURN)
