// -*- coding: utf-8 -*-
// 主窗口：原生 Win32 界面（对应 Python 版的 tkinter UI）。
#pragma once
#include <memory>
#include <string>
#include <windows.h>
#include "config.h"
#include "sender.h"

class DanmuApp {
public:
    explicit DanmuApp(const std::wstring& appDir);   // 失败抛 AppError
    ~DanmuApp();
    int run();                                       // 进入消息循环

    // 供全屏框选遮罩 / 子类化控件回调
    void on_region_picked(bool ok, const RECT& region);
    void add_custom();

private:
    static LRESULT CALLBACK WndProc(HWND, UINT, WPARAM, LPARAM);
    LRESULT on_message(HWND hwnd, UINT msg, WPARAM w, LPARAM l);
    void on_command(int id, int code);

    void build_ui();
    void apply_config_to_ui();
    void refresh_files();
    void on_file_changed();
    void import_file();
    void open_folder();
    void pick_region();
    void clear_region();
    void on_start();
    void on_stop();
    void set_running(bool running);
    void on_setting_changed();
    void update_cursor_status();
    void update_region_label();
    void set_status(const std::wstring& text, COLORREF color);
    void log(const std::wstring& text);
    void save_config();

    HWND hwnd_ = NULL;
    HINSTANCE hInst_;
    std::wstring appDir_, danmuDir_, configPath_;
    AppConfig cfg_;
    DanmuLoader loader_;
    std::unique_ptr<SenderWorker> worker_;
    int totalSent_ = 0;
    bool updatingUI_ = false;
    bool regionPicking_ = false;

    // 控件句柄
    HWND hFileCombo_ = NULL, hFileInfo_ = NULL;
    HWND hInterval_ = NULL, hJitter_ = NULL, hShuffle_ = NULL;
    HWND hFgCheck_ = NULL, hFgKeyword_ = NULL;
    HWND hRegion_ = NULL, hCursor_ = NULL;
    HWND hBtnStart_ = NULL, hBtnStop_ = NULL, hStatus_ = NULL, hProgress_ = NULL;
    HWND hCustom_ = NULL, hLog_ = NULL;

    HFONT font_ = NULL, fontBold_ = NULL, fontLog_ = NULL;
    COLORREF statusColor_ = RGB(136, 136, 136);
    COLORREF cursorColor_ = RGB(136, 136, 136);
};
