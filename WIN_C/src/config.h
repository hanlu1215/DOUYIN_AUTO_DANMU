// -*- coding: utf-8 -*-
// 运行配置：界面与工作线程共享，字段由互斥锁保护（工作线程每次循环实时读取）。
#pragma once
#include <mutex>
#include <string>
#include <windows.h>

inline const double MIN_INTERVAL = 2.0;   // 抖音直播发言频率的安全下限（秒），界面不允许低于此值

struct AppConfig {
    std::mutex mtx;                 // 保护以下所有字段
    bool hasRegion = false;         // 是否已框选输入框区域
    RECT region{};                  // 输入框区域（屏幕物理坐标）
    double interval = 10.0;         // 发送基础间隔（秒）
    int jitter = 20;                // 随机抖动（±%）
    bool shuffle = true;            // 每轮循环随机打乱顺序
    bool fgCheck = true;            // 校验前台窗口标题（防误发到其他程序）
    std::wstring fgKeyword = L"抖音";
    std::wstring currentFile;       // 当前弹幕文件名
};

// 读取 config.json（不存在或损坏则保留默认值）
void load_config(AppConfig& cfg, const std::wstring& path);

// 保存 config.json（UTF-8）
void save_config(AppConfig& cfg, const std::wstring& path);
