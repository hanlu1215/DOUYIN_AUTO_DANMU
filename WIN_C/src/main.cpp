// -*- coding: utf-8 -*-
// 抖音直播自动弹幕助手（C++ 版）—— 程序入口。
//
// 构建：cmake -B build && cmake --build build
// 产物 build/douyin_danmu.exe 静态链接运行时，可拷贝到其他 Win10/11 机器直接运行。
#include <windows.h>
#include <commctrl.h>
#include <exception>
#include <filesystem>
#include <fstream>
#include <string>
#include "app.h"
#include "strutil.h"
#include "win_input.h"

// 视觉样式 / DPI 感知清单由 src/app.rc 嵌入（MinGW 与 MSVC 通用），不要再用 pragma 注入

// 程序根目录：默认取 exe 所在目录；若 exe 在 build/ 或 build/<Config>/ 下，
// 则向上最多找 3 层，定位含 danmu/ 文件夹的项目根目录
static std::wstring get_app_dir() {
    wchar_t buf[MAX_PATH];
    GetModuleFileNameW(NULL, buf, MAX_PATH);
    std::wstring dir(buf);
    size_t p = dir.find_last_of(L"\\/");
    if (p != std::wstring::npos) dir.resize(p);

    std::wstring cur = dir;
    for (int i = 0; i < 3; i++) {
        DWORD attr = GetFileAttributesW((cur + L"\\danmu").c_str());
        if (attr != INVALID_FILE_ATTRIBUTES && (attr & FILE_ATTRIBUTE_DIRECTORY))
            return cur;
        size_t q = cur.find_last_of(L"\\/");
        if (q == std::wstring::npos || q == 0) break;
        cur.resize(q);
    }
    return dir;
}

// GUI 程序没有控制台，异常写入 error.log 方便排查（与 Python 版一致）
static void write_error_log(const std::wstring& dir, const std::string& text) {
    std::ofstream f(std::filesystem::path(dir) / L"error.log",
                    std::ios::binary | std::ios::trunc);
    if (f) f << text;
}

int WINAPI wWinMain(HINSTANCE, HINSTANCE, PWSTR, int) {
    const std::wstring appDir = get_app_dir();
    try {
        win_input::set_process_dpi_aware();   // 必须在创建窗口前调用
        INITCOMMONCONTROLSEX icc{ sizeof(icc), ICC_STANDARD_CLASSES | ICC_WIN95_CLASSES };
        InitCommonControlsEx(&icc);
        DanmuApp app(appDir);
        return app.run();
    } catch (const AppError& e) {
        write_error_log(appDir, wide_to_utf8(e.msg) + "\n");
        return 1;
    } catch (const std::exception& e) {
        write_error_log(appDir, std::string(e.what()) + "\n");
        return 1;
    } catch (...) {
        write_error_log(appDir, "unknown error\n");
        return 1;
    }
}
