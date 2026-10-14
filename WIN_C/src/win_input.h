// -*- coding: utf-8 -*-
// Windows 输入层：光标位置 / 剪贴板 / 键盘模拟，直接调 Win32 API，零依赖。
#pragma once
#include <string>
#include <windows.h>

namespace win_input {

// 让进程使用物理像素坐标，保证 GetCursorPos 与界面坐标一致（必须在创建窗口前调用）
void set_process_dpi_aware();

// 鼠标光标在屏幕上的物理坐标
POINT get_cursor_pos();

// 虚拟桌面（多显示器合并区域）
RECT get_virtual_screen();

// 当前前台窗口标题，用于校验弹幕是否发往抖音窗口
std::wstring get_foreground_title();

// 把文本写入系统剪贴板（CF_UNICODETEXT）；失败抛 AppError
void set_clipboard(const std::wstring& text);

// 模拟 Ctrl+V，粘贴剪贴板内容
void paste();

// 模拟按回车（直播间输入框发送）
void press_enter();

} // namespace win_input
