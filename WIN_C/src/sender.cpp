// -*- coding: utf-8 -*-
#include "sender.h"
#include "danmu_io.h"
#include "strutil.h"
#include "win_input.h"
#include <algorithm>
#include <cwctype>
#include <ctime>

bool point_in_region(POINT pos, bool hasRegion, const RECT& r) {
    if (!hasRegion) return false;
    return pos.x >= r.left && pos.x <= r.right && pos.y >= r.top && pos.y <= r.bottom;
}

// ---------- DanmuLoader ----------

void DanmuLoader::load(const std::wstring& path) {
    std::vector<std::wstring> items = load_danmu(path);   // 可能抛 AppError
    std::lock_guard<std::mutex> lk(mtx_);
    items_ = std::move(items);
    path_ = path;
    index_ = 0;
}

void DanmuLoader::clear() {
    std::lock_guard<std::mutex> lk(mtx_);
    path_.clear();
    items_.clear();
    index_ = 0;
}

std::wstring DanmuLoader::path() const {
    std::lock_guard<std::mutex> lk(mtx_);
    return path_;
}

int DanmuLoader::size() {
    std::lock_guard<std::mutex> lk(mtx_);
    return (int)items_.size();
}

bool DanmuLoader::next(bool shuffle, std::wstring& text, int& idx, int& total) {
    std::lock_guard<std::mutex> lk(mtx_);
    if (items_.empty()) return false;
    if (index_ >= items_.size()) {
        if (shuffle) std::shuffle(items_.begin(), items_.end(), rng_);
        index_ = 0;
    }
    text = items_[index_];
    idx = (int)index_ + 1;
    total = (int)items_.size();
    index_++;
    return true;
}

bool DanmuLoader::add(const std::wstring& text, std::wstring& err) {
    std::wstring t = trim(text);
    if (t.empty()) { err = L"内容为空"; return false; }
    std::lock_guard<std::mutex> lk(mtx_);
    if (path_.empty()) { err = L"未加载弹幕文件"; return false; }
    for (const auto& s : items_)
        if (s == t) { err = L"该条目已存在"; return false; }
    items_.push_back(t);
    try {
        save_danmu(path_, items_);
    } catch (const AppError& e) {
        items_.pop_back();
        err = e.msg;
        return false;
    }
    return true;
}

// ---------- SenderWorker ----------

SenderWorker::SenderWorker(DanmuLoader* loader, AppConfig* cfg, HWND notify)
    : loader_(loader), cfg_(cfg), notify_(notify) {
    stopEvent_ = CreateEventW(NULL, TRUE, FALSE, NULL);
}

SenderWorker::~SenderWorker() {
    stop();
    join();
    if (stopEvent_) CloseHandle(stopEvent_);
}

void SenderWorker::start() {
    th_ = std::thread(&SenderWorker::run, this);
}

void SenderWorker::stop() {
    if (stopEvent_) SetEvent(stopEvent_);
}

void SenderWorker::join() {
    if (th_.joinable()) th_.join();
}

static std::wstring to_lower(std::wstring s) {
    for (auto& c : s) c = (wchar_t)towlower(c);
    return s;
}

void SenderWorker::run() {
    running_ = true;
    std::mt19937 rng(std::random_device{}() ^ (unsigned)time(nullptr));
    try {
        while (WaitForSingleObject(stopEvent_, 0) != WAIT_OBJECT_0) {
            // 每次循环实时读取配置快照，界面修改后立即生效
            bool hasRegion, shuffle, fgCheck;
            RECT region{};
            double interval;
            int jitter;
            std::wstring fgKeyword;
            {
                std::lock_guard<std::mutex> lk(cfg_->mtx);
                hasRegion = cfg_->hasRegion;
                region = cfg_->region;
                interval = cfg_->interval;
                jitter = cfg_->jitter;
                shuffle = cfg_->shuffle;
                fgCheck = cfg_->fgCheck;
                fgKeyword = trim(cfg_->fgKeyword);
            }

            // 1. 光标必须位于输入框区域内
            POINT pos = win_input::get_cursor_pos();
            if (!point_in_region(pos, hasRegion, region)) {
                LPARAM lp = MAKELPARAM((WORD)pos.x, (WORD)pos.y);
                PostMessageW(notify_, WM_DANMU_PAUSED, 0, lp);
                if (WaitForSingleObject(stopEvent_, CHECK_INTERVAL_MS) == WAIT_OBJECT_0) break;
                continue;
            }

            // 2. 前台窗口校验：避免把弹幕发到其他程序里
            if (fgCheck) {
                std::wstring kw = to_lower(fgKeyword.empty() ? L"抖音" : fgKeyword);
                std::wstring title = win_input::get_foreground_title();
                if (to_lower(title).find(kw) == std::wstring::npos) {
                    PostMessageW(notify_, WM_DANMU_FGBLOCKED, 0, (LPARAM) new std::wstring(title));
                    if (WaitForSingleObject(stopEvent_, CHECK_INTERVAL_MS) == WAIT_OBJECT_0) break;
                    continue;
                }
            }

            // 3. 取词 → 随机前置延迟 → 剪贴板 → 粘贴 → 回车
            std::wstring text;
            int idx = 0, total = 0;
            if (!loader_->next(shuffle, text, idx, total))
                throw AppError{ L"弹幕文件为空" };

            std::uniform_int_distribution<DWORD> preDelay(100, 400);   // 节奏更接近真人
            if (WaitForSingleObject(stopEvent_, preDelay(rng)) == WAIT_OBJECT_0) break;

            win_input::set_clipboard(text);
            win_input::paste();
            Sleep(150);   // 等输入框收到粘贴内容
            win_input::press_enter();

            PostMessageW(notify_, WM_DANMU_SENT, 0,
                         (LPARAM) new SentInfo{ text, idx, total });

            // 4. 频率控制：基础间隔 × (1 ± 抖动比例)，下限 1 秒
            double j = jitter / 100.0;
            std::uniform_real_distribution<double> mul(1.0 - j, 1.0 + j);
            double delay = interval * mul(rng);
            if (delay < MIN_DELAY) delay = MIN_DELAY;
            if (WaitForSingleObject(stopEvent_, (DWORD)(delay * 1000)) == WAIT_OBJECT_0) break;
        }
    } catch (const AppError& e) {
        PostMessageW(notify_, WM_DANMU_ERROR, 0, (LPARAM) new std::wstring(e.msg));
    } catch (const std::exception& e) {
        PostMessageW(notify_, WM_DANMU_ERROR, 0, (LPARAM) new std::wstring(utf8_to_wide(e.what())));
    }
    running_ = false;
    PostMessageW(notify_, WM_DANMU_STOPPED, 0, 0);
}
