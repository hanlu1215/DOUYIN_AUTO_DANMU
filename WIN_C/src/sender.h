// -*- coding: utf-8 -*-
// 发送工作线程：光标区域检测 + 频率控制 + 弹幕循环发送。
#pragma once
#include <atomic>
#include <mutex>
#include <random>
#include <string>
#include <thread>
#include <vector>
#include <windows.h>
#include "config.h"

// ---- 工作线程 → 界面 的自定义消息（lParam 由界面负责释放）----
#define WM_DANMU_SENT      (WM_APP + 1)   // lParam: SentInfo*
#define WM_DANMU_PAUSED    (WM_APP + 2)   // lParam: 打包的光标坐标（用 GET_X_LPARAM 解开）
#define WM_DANMU_FGBLOCKED (WM_APP + 3)   // lParam: std::wstring*（前台窗口标题）
#define WM_DANMU_ERROR     (WM_APP + 4)   // lParam: std::wstring*（错误信息）
#define WM_DANMU_STOPPED   (WM_APP + 5)   // 无 payload

struct SentInfo {
    std::wstring text;
    int idx;     // 本循环已发条数（1 起）
    int total;   // 总条数
};

// 判断坐标是否在矩形区域内
bool point_in_region(POINT pos, bool hasRegion, const RECT& region);

// 管理一个弹幕 JSON 文件：加载、循环取词、追加自定义条目（线程安全）
class DanmuLoader {
public:
    void load(const std::wstring& path);   // 失败抛 AppError
    void clear();
    std::wstring path() const;
    int size();

    // 取下一条弹幕；一轮发完后从头循环（可选每轮打乱顺序）。返回 false 表示无条目
    bool next(bool shuffle, std::wstring& text, int& idx, int& total);

    // 追加自定义条目（去重）并写回文件；失败返回 false 并给出原因
    bool add(const std::wstring& text, std::wstring& err);

private:
    mutable std::mutex mtx_;
    std::wstring path_;
    std::vector<std::wstring> items_;
    size_t index_ = 0;
    std::mt19937 rng_{ std::random_device{}() };
};

// 发送线程。循环流程：检测光标 → 不在输入框区域内则暂停 → 校验前台窗口 →
// 弹幕写入剪贴板 → Ctrl+V 粘贴 → 回车发送 → 按「基础间隔 ± 随机抖动」休眠。
// 每次循环实时读取共享的 AppConfig，界面修改设置后立即生效，无需重启线程。
class SenderWorker {
public:
    SenderWorker(DanmuLoader* loader, AppConfig* cfg, HWND notify);
    ~SenderWorker();

    void start();
    void stop();                 // 可中断的等待，stop 后线程很快退出
    void join();
    bool running() const { return running_; }

private:
    void run();

    DanmuLoader* loader_;
    AppConfig* cfg_;
    HWND notify_;
    HANDLE stopEvent_;
    std::thread th_;
    std::atomic<bool> running_{ false };

    static const DWORD CHECK_INTERVAL_MS = 300;   // 条件不满足时的轮询间隔
    inline static const double MIN_DELAY = 1.0;   // 两条弹幕之间的绝对下限（秒）
};
