// -*- coding: utf-8 -*-
#include "app.h"
#include "danmu_io.h"
#include "resource.h"
#include "strutil.h"
#include "win_input.h"
#include <windowsx.h>
#include <commdlg.h>
#include <shellapi.h>
#include <algorithm>
#include <cstdio>
#include <vector>

namespace {

// ---- 控件 ID ----
enum {
    IDC_COMBO_FILE = 1001,
    IDC_BTN_IMPORT, IDC_BTN_REFRESH, IDC_BTN_OPENFOLDER, IDC_STATIC_FILEINFO,
    IDC_EDIT_INTERVAL, IDC_EDIT_JITTER, IDC_CHK_SHUFFLE,
    IDC_CHK_FGCHECK, IDC_EDIT_FGKEYWORD, IDC_STATIC_FGNOTE, IDC_STATIC_HINT,
    IDC_STATIC_REGION, IDC_BTN_PICKREGION, IDC_BTN_CLEARREGION, IDC_STATIC_CURSOR,
    IDC_BTN_START, IDC_BTN_STOP, IDC_STATIC_STATUS, IDC_STATIC_PROGRESS,
    IDC_EDIT_CUSTOM, IDC_BTN_ADDCUSTOM,
    IDC_LIST_LOG,
};

const wchar_t* kMainClass = L"DanmuAssistantMain";
const wchar_t* kOverlayClass = L"DanmuRegionSelector";
const int IDT_CURSOR = 1;   // 光标状态刷新定时器（150ms）

const COLORREF kGray = RGB(136, 136, 136);
const COLORREF kGreen = RGB(46, 125, 50);
const COLORREF kOrange = RGB(230, 81, 0);

HFONT make_font(int pt, bool bold, const wchar_t* face) {
    HDC hdc = GetDC(NULL);
    int h = -MulDiv(pt, GetDeviceCaps(hdc, LOGPIXELSY), 72);
    ReleaseDC(NULL, hdc);
    return CreateFontW(h, 0, 0, 0, bold ? FW_BOLD : FW_NORMAL, 0, 0, 0,
                       DEFAULT_CHARSET, OUT_DEFAULT_PRECIS, CLIP_DEFAULT_PRECIS,
                       CLEARTYPE_QUALITY, DEFAULT_PITCH | FF_DONTCARE, face);
}

BOOL CALLBACK set_font_cb(HWND child, LPARAM lp) {
    SendMessageW(child, WM_SETFONT, (WPARAM)lp, TRUE);
    return TRUE;
}

// 自定义弹幕输入框：拦截回车 = 点「添加到当前文件」
WNDPROC g_oldCustomProc = nullptr;
LRESULT CALLBACK CustomEditProc(HWND h, UINT m, WPARAM w, LPARAM l) {
    if (m == WM_KEYDOWN && w == VK_RETURN) {
        HWND parent = GetParent(h);
        auto* app = (DanmuApp*)GetWindowLongPtrW(parent, GWLP_USERDATA);
        if (app) app->add_custom();
        return 0;
    }
    return CallWindowProcW(g_oldCustomProc, h, m, w, l);
}

// ---- 全屏半透明遮罩：拖拽框选抖音输入框区域 ----
class RegionSelector {
public:
    static void start(HINSTANCE hInst, HWND owner, DanmuApp* app) {
        static bool registered = false;
        if (!registered) {
            WNDCLASSW wc{};
            wc.lpfnWndProc = WndProc;
            wc.hInstance = hInst;
            wc.lpszClassName = kOverlayClass;
            wc.hCursor = LoadCursor(NULL, IDC_CROSS);
            wc.hbrBackground = (HBRUSH)GetStockObject(BLACK_BRUSH);
            RegisterClassW(&wc);
            registered = true;
        }
        RECT vs = win_input::get_virtual_screen();
        auto* self = new RegionSelector;
        self->app_ = app;
        self->originX_ = vs.left;
        self->originY_ = vs.top;
        CreateWindowExW(WS_EX_TOPMOST | WS_EX_LAYERED | WS_EX_TOOLWINDOW,
                        kOverlayClass, L"", WS_POPUP,
                        vs.left, vs.top, vs.right - vs.left, vs.bottom - vs.top,
                        owner, NULL, hInst, self);
    }

private:
    DanmuApp* app_ = nullptr;
    int originX_ = 0, originY_ = 0;
    bool dragging_ = false, hasRect_ = false;
    POINT start_{};
    RECT rect_{};
    HFONT fontText_ = NULL;

    static LRESULT CALLBACK WndProc(HWND h, UINT m, WPARAM w, LPARAM l) {
        auto* self = (RegionSelector*)GetWindowLongPtrW(h, GWLP_USERDATA);
        if (m == WM_NCCREATE) {
            self = (RegionSelector*)((CREATESTRUCTW*)l)->lpCreateParams;
            SetWindowLongPtrW(h, GWLP_USERDATA, (LONG_PTR)self);
        }
        return self ? self->handle(h, m, w, l) : DefWindowProcW(h, m, w, l);
    }

    void finish(HWND h, bool ok) {
        RECT r = rect_;
        if (ok) {
            r.left += originX_; r.right += originX_;
            r.top += originY_; r.bottom += originY_;
        }
        app_->on_region_picked(ok, r);
        DestroyWindow(h);
    }

    LRESULT handle(HWND h, UINT m, WPARAM w, LPARAM l) {
        switch (m) {
        case WM_CREATE:
            SetLayeredWindowAttributes(h, 0, (BYTE)(255 * 0.35), LWA_ALPHA);
            fontText_ = make_font(16, false, L"Microsoft YaHei");
            ShowWindow(h, SW_SHOW);
            SetForegroundWindow(h);
            SetFocus(h);
            return 0;

        case WM_LBUTTONDOWN:
            dragging_ = true;
            hasRect_ = false;
            start_.x = GET_X_LPARAM(l);
            start_.y = GET_Y_LPARAM(l);
            SetCapture(h);
            return 0;

        case WM_MOUSEMOVE:
            if (dragging_) {
                int x = GET_X_LPARAM(l), y = GET_Y_LPARAM(l);
                int sx = (int)start_.x, sy = (int)start_.y;
                rect_.left = (std::min)(sx, x);
                rect_.right = (std::max)(sx, x);
                rect_.top = (std::min)(sy, y);
                rect_.bottom = (std::max)(sy, y);
                hasRect_ = true;
                InvalidateRect(h, NULL, FALSE);
            }
            return 0;

        case WM_LBUTTONUP: {
            dragging_ = false;
            ReleaseCapture();
            bool ok = hasRect_ &&
                      (rect_.right - rect_.left) >= 10 && (rect_.bottom - rect_.top) >= 10;
            finish(h, ok);   // 矩形太小视为误操作
            return 0;
        }

        case WM_KEYDOWN:
            if (w == VK_ESCAPE) finish(h, false);
            return 0;

        case WM_CAPTURECHANGED:
            dragging_ = false;
            return 0;

        case WM_PAINT: {
            PAINTSTRUCT ps;
            HDC hdc = BeginPaint(h, &ps);
            RECT rc;
            GetClientRect(h, &rc);
            int w = rc.right - rc.left, hh = rc.bottom - rc.top;
            // 双缓冲，避免拖拽时闪烁
            HDC mem = CreateCompatibleDC(hdc);
            HBITMAP bmp = CreateCompatibleBitmap(hdc, w, hh);
            HGDIOBJ oldBmp = SelectObject(mem, bmp);
            FillRect(mem, &rc, (HBRUSH)GetStockObject(BLACK_BRUSH));

            SetBkMode(mem, TRANSPARENT);
            SetTextColor(mem, RGB(255, 255, 255));
            HGDIOBJ oldFont = SelectObject(mem, fontText_);
            RECT tr{ 0, 90, w, 200 };
            DrawTextW(mem,
                      L"按住鼠标左键拖拽，框选出抖音直播间的输入框\n松开鼠标完成框选，按 Esc 取消",
                      -1, &tr, DT_CENTER | DT_WORDBREAK);

            if (hasRect_) {
                HPEN pen = CreatePen(PS_SOLID, 3, RGB(255, 59, 48));
                HGDIOBJ oldPen = SelectObject(mem, pen);
                HGDIOBJ oldBrush = SelectObject(mem, GetStockObject(NULL_BRUSH));
                Rectangle(mem, rect_.left, rect_.top, rect_.right, rect_.bottom);
                SelectObject(mem, oldPen);
                SelectObject(mem, oldBrush);
                DeleteObject(pen);
            }
            SelectObject(mem, oldFont);

            BitBlt(hdc, 0, 0, w, hh, mem, 0, 0, SRCCOPY);
            SelectObject(mem, oldBmp);
            DeleteObject(bmp);
            DeleteDC(mem);
            EndPaint(h, &ps);
            return 0;
        }

        case WM_DESTROY:
            if (fontText_) { DeleteObject(fontText_); fontText_ = NULL; }
            return 0;

        case WM_NCDESTROY:
            delete this;
            return 0;
        }
        return DefWindowProcW(h, m, w, l);
    }
};

// 创建控件的辅助器
struct UIBuilder {
    HWND parent;
    HINSTANCE inst;

    HWND mk(const wchar_t* cls, const wchar_t* text, DWORD style,
            int x, int y, int w, int h, int id, DWORD ex = 0) {
        return CreateWindowExW(ex, cls, text, style | WS_CHILD | WS_VISIBLE,
                               x, y, w, h, parent, (HMENU)(INT_PTR)id, inst, NULL);
    }
    HWND group(const wchar_t* text, int x, int y, int w, int h) {
        return mk(L"BUTTON", text, BS_GROUPBOX, x, y, w, h, 0);
    }
    HWND label(const wchar_t* text, int x, int y, int w, int h, int id, DWORD extra = 0) {
        return mk(L"STATIC", text, extra, x, y, w, h, id);
    }
    HWND button(const wchar_t* text, int x, int y, int w, int h, int id) {
        return mk(L"BUTTON", text, BS_PUSHBUTTON | WS_TABSTOP, x, y, w, h, id);
    }
    HWND edit(int x, int y, int w, int h, int id) {
        return mk(L"EDIT", L"", ES_AUTOHSCROLL | WS_TABSTOP, x, y, w, h, id, WS_EX_CLIENTEDGE);
    }
    HWND check(const wchar_t* text, int x, int y, int w, int h, int id) {
        return mk(L"BUTTON", text, BS_AUTOCHECKBOX | WS_TABSTOP, x, y, w, h, id);
    }
};

} // namespace

// ================= DanmuApp =================

DanmuApp::DanmuApp(const std::wstring& appDir)
    : hInst_(GetModuleHandleW(NULL)), appDir_(appDir) {
    danmuDir_ = appDir_ + L"\\danmu";
    configPath_ = appDir_ + L"\\config.json";
    load_config(cfg_, configPath_);

    WNDCLASSW wc{};
    wc.lpfnWndProc = WndProc;
    wc.hInstance = hInst_;
    wc.lpszClassName = kMainClass;
    wc.hCursor = LoadCursor(NULL, IDC_ARROW);
    wc.hbrBackground = (HBRUSH)(COLOR_BTNFACE + 1);
    // 手绘风格抖音音符图标（app.rc 嵌入）
    wc.hIcon = LoadIconW(hInst_, MAKEINTRESOURCEW(IDI_APPICON));
    if (!RegisterClassW(&wc)) throw AppError{ L"注册窗口类失败" };

    DWORD style = WS_OVERLAPPED | WS_CAPTION | WS_SYSMENU | WS_MINIMIZEBOX;
    RECT rc{ 0, 0, 700, 770 };
    AdjustWindowRect(&rc, style, FALSE);
    hwnd_ = CreateWindowExW(0, kMainClass, L"抖音直播自动弹幕助手", style,
                            CW_USEDEFAULT, CW_USEDEFAULT,
                            rc.right - rc.left, rc.bottom - rc.top,
                            NULL, NULL, hInst_, this);
    if (!hwnd_) throw AppError{ L"创建主窗口失败" };
    // WNDCLASSW 没有 hIconSm，标题栏小图标用 WM_SETICON 单独设置
    SendMessageW(hwnd_, WM_SETICON, ICON_SMALL,
                 (LPARAM)LoadIconW(hInst_, MAKEINTRESOURCEW(IDI_APPICON)));

    font_ = make_font(9, false, L"Microsoft YaHei");
    fontBold_ = make_font(11, true, L"Microsoft YaHei");
    // 日志不能用 Consolas：无中文字形，中文会显示成方块（乱码）
    fontLog_ = make_font(9, false, L"Microsoft YaHei");

    build_ui();
    apply_config_to_ui();
    refresh_files();
    SetTimer(hwnd_, IDT_CURSOR, 150, NULL);
}

DanmuApp::~DanmuApp() {
    if (worker_) { worker_->stop(); worker_->join(); worker_.reset(); }
    if (font_) DeleteObject(font_);
    if (fontBold_) DeleteObject(fontBold_);
    if (fontLog_) DeleteObject(fontLog_);
}

int DanmuApp::run() {
    ShowWindow(hwnd_, SW_SHOW);
    UpdateWindow(hwnd_);
    MSG msg;
    while (GetMessageW(&msg, NULL, 0, 0) > 0) {
        TranslateMessage(&msg);
        DispatchMessageW(&msg);
    }
    return (int)msg.wParam;
}

// ---------- 界面搭建 ----------
void DanmuApp::build_ui() {
    UIBuilder ui{ hwnd_, hInst_ };

    // ---- 弹幕文件 ----
    ui.group(L"弹幕文件（danmu 文件夹）", 10, 8, 680, 78);
    hFileCombo_ = ui.mk(L"COMBOBOX", L"", CBS_DROPDOWNLIST | WS_VSCROLL | WS_TABSTOP,
                        22, 30, 300, 200, IDC_COMBO_FILE);
    ui.button(L"导入", 330, 28, 56, 24, IDC_BTN_IMPORT);
    ui.button(L"刷新", 392, 28, 56, 24, IDC_BTN_REFRESH);
    ui.button(L"打开文件夹", 454, 28, 90, 24, IDC_BTN_OPENFOLDER);
    hFileInfo_ = ui.label(L"条目数: -", 22, 58, 400, 18, IDC_STATIC_FILEINFO);

    // ---- 发送设置 ----
    ui.group(L"发送设置", 10, 92, 680, 96);
    ui.label(L"发送间隔（秒）", 22, 116, 92, 18, 0);
    hInterval_ = ui.edit(116, 113, 52, 22, IDC_EDIT_INTERVAL);
    ui.label(L"随机抖动 ±", 178, 116, 72, 18, 0);
    hJitter_ = ui.edit(252, 113, 42, 22, IDC_EDIT_JITTER);
    ui.label(L"%", 298, 116, 14, 18, 0);
    hShuffle_ = ui.check(L"每轮随机打乱", 322, 113, 130, 22, IDC_CHK_SHUFFLE);
    hFgCheck_ = ui.check(L"仅当前台窗口标题包含", 22, 142, 160, 22, IDC_CHK_FGCHECK);
    hFgKeyword_ = ui.edit(186, 140, 80, 22, IDC_EDIT_FGKEYWORD);
    ui.label(L"时才发送（防误发到其他程序）", 272, 142, 190, 18, IDC_STATIC_FGNOTE);
    ui.label(L"提示：抖音直播发言太快会被限制（提示「发言太快」），建议间隔 ≥ 2~3 秒",
             22, 166, 650, 16, IDC_STATIC_HINT);

    // ---- 输入框区域 ----
    ui.group(L"输入框区域（光标检测）", 10, 194, 680, 72);
    hRegion_ = ui.label(L"未设置", 22, 216, 360, 18, IDC_STATIC_REGION);
    ui.button(L"框选输入框区域", 440, 212, 130, 26, IDC_BTN_PICKREGION);
    ui.button(L"清除", 578, 212, 60, 26, IDC_BTN_CLEARREGION);
    hCursor_ = ui.label(L"光标: (0, 0)", 22, 240, 400, 18, IDC_STATIC_CURSOR);

    // ---- 控制 ----
    hBtnStart_ = ui.button(L"▶ 开始发送", 10, 274, 140, 36, IDC_BTN_START);
    hBtnStop_ = ui.button(L"■ 停止", 158, 274, 140, 36, IDC_BTN_STOP);
    EnableWindow(hBtnStop_, FALSE);
    hStatus_ = ui.label(L"● 空闲", 308, 283, 240, 18, IDC_STATIC_STATUS);
    hProgress_ = ui.label(L"本循环 -/- · 累计 0 条", 470, 283, 220, 18,
                          IDC_STATIC_PROGRESS, SS_RIGHT);

    // ---- 自定义弹幕 ----
    ui.group(L"自定义弹幕（追加到当前文件，立即生效，自动去重）", 10, 318, 680, 60);
    hCustom_ = ui.edit(22, 340, 510, 24, IDC_EDIT_CUSTOM);
    ui.button(L"添加到当前文件", 540, 338, 140, 26, IDC_BTN_ADDCUSTOM);
    g_oldCustomProc = (WNDPROC)SetWindowLongPtrW(hCustom_, GWLP_WNDPROC, (LONG_PTR)CustomEditProc);

    // ---- 日志 ----
    ui.group(L"日志", 10, 384, 680, 376);
    hLog_ = ui.mk(L"LISTBOX", L"", LBS_NOINTEGRALHEIGHT | WS_VSCROLL | WS_HSCROLL | WS_BORDER,
                  20, 404, 660, 344, IDC_LIST_LOG);
    ListBox_SetHorizontalExtent(hLog_, 1500);

    // 统一字体：普通控件微软雅黑，启停按钮加粗，日志用等宽
    EnumChildWindows(hwnd_, set_font_cb, (LPARAM)font_);
    SendMessageW(hBtnStart_, WM_SETFONT, (WPARAM)fontBold_, TRUE);
    SendMessageW(hBtnStop_, WM_SETFONT, (WPARAM)fontBold_, TRUE);
    SendMessageW(hLog_, WM_SETFONT, (WPARAM)fontLog_, TRUE);
}

void DanmuApp::apply_config_to_ui() {
    updatingUI_ = true;
    {
        std::lock_guard<std::mutex> lk(cfg_.mtx);
        wchar_t b[64];
        swprintf(b, 64, L"%g", cfg_.interval);
        SetWindowTextW(hInterval_, b);
        swprintf(b, 64, L"%d", cfg_.jitter);
        SetWindowTextW(hJitter_, b);
        Button_SetCheck(hShuffle_, cfg_.shuffle ? BST_CHECKED : BST_UNCHECKED);
        Button_SetCheck(hFgCheck_, cfg_.fgCheck ? BST_CHECKED : BST_UNCHECKED);
        SetWindowTextW(hFgKeyword_, cfg_.fgKeyword.c_str());
    }   // 必须先释放锁：update_region_label 内部会再次加锁，std::mutex 不可重入
    updatingUI_ = false;
    update_region_label();
}

void DanmuApp::update_region_label() {
    std::lock_guard<std::mutex> lk(cfg_.mtx);
    if (cfg_.hasRegion) {
        wchar_t b[128];
        swprintf(b, 128, L"(%d, %d) → (%d, %d)",
                 (int)cfg_.region.left, (int)cfg_.region.top,
                 (int)cfg_.region.right, (int)cfg_.region.bottom);
        SetWindowTextW(hRegion_, b);
    } else {
        SetWindowTextW(hRegion_, L"未设置");
    }
}

// ---------- 弹幕文件管理 ----------
void DanmuApp::refresh_files() {
    CreateDirectoryW(danmuDir_.c_str(), NULL);
    std::vector<std::wstring> files;
    WIN32_FIND_DATAW fd;
    HANDLE fh = FindFirstFileW((danmuDir_ + L"\\*.json").c_str(), &fd);
    if (fh != INVALID_HANDLE_VALUE) {
        do {
            if (!(fd.dwFileAttributes & FILE_ATTRIBUTE_DIRECTORY))
                files.push_back(fd.cFileName);
        } while (FindNextFileW(fh, &fd));
        FindClose(fh);
    }
    std::sort(files.begin(), files.end());

    ComboBox_ResetContent(hFileCombo_);
    for (const auto& f : files) ComboBox_AddString(hFileCombo_, f.c_str());

    std::wstring cur;
    { std::lock_guard<std::mutex> lk(cfg_.mtx); cur = cfg_.currentFile; }
    int sel = -1;
    if (!cur.empty())
        sel = (int)ComboBox_FindStringExact(hFileCombo_, -1, cur.c_str());
    if (sel < 0 && !files.empty()) sel = 0;
    ComboBox_SetCurSel(hFileCombo_, sel);
    on_file_changed();
}

void DanmuApp::on_file_changed() {
    int sel = ComboBox_GetCurSel(hFileCombo_);
    if (sel < 0) {
        loader_.clear();
        SetWindowTextW(hFileInfo_, L"条目数: -");
        return;
    }
    wchar_t name[MAX_PATH];
    ComboBox_GetLBText(hFileCombo_, sel, name);
    try {
        loader_.load(danmuDir_ + L"\\" + name);
    } catch (const AppError& e) {
        loader_.clear();
        SetWindowTextW(hFileInfo_, L"条目数: 加载失败");
        MessageBoxW(hwnd_, e.msg.c_str(), L"弹幕文件加载失败", MB_ICONERROR);
        return;
    }
    { std::lock_guard<std::mutex> lk(cfg_.mtx); cfg_.currentFile = name; }
    save_config();
    wchar_t b[64];
    swprintf(b, 64, L"条目数: %d", loader_.size());
    SetWindowTextW(hFileInfo_, b);
    log(std::wstring(L"已加载弹幕文件 ") + name + L"（" +
        std::to_wstring(loader_.size()) + L" 条）");
}

void DanmuApp::import_file() {
    wchar_t fileBuf[MAX_PATH]{};
    OPENFILENAMEW ofn{};
    ofn.lStructSize = sizeof(ofn);
    ofn.hwndOwner = hwnd_;
    ofn.lpstrFilter = L"JSON 文件 (*.json)\0*.json\0所有文件 (*.*)\0*.*\0";
    ofn.lpstrFile = fileBuf;
    ofn.nMaxFile = MAX_PATH;
    ofn.lpstrTitle = L"选择弹幕 JSON 文件";
    ofn.Flags = OFN_FILEMUSTEXIST | OFN_HIDEREADONLY;
    if (!GetOpenFileNameW(&ofn)) return;

    int count = 0;
    try {
        count = (int)load_danmu(fileBuf).size();   // 仅校验格式
    } catch (const AppError& e) {
        MessageBoxW(hwnd_, e.msg.c_str(), L"导入失败", MB_ICONERROR);
        return;
    }

    std::wstring src = fileBuf;
    std::wstring base = src.substr(src.find_last_of(L"\\/") + 1);
    std::wstring target = danmuDir_ + L"\\" + base;
    if (GetFileAttributesW(target.c_str()) != INVALID_FILE_ATTRIBUTES &&
        _wcsicmp(src.c_str(), target.c_str()) != 0) {
        if (MessageBoxW(hwnd_, L"弹幕文件夹中已存在同名文件，是否覆盖？",
                        L"覆盖确认", MB_YESNO | MB_ICONQUESTION) != IDYES)
            return;
    }
    CreateDirectoryW(danmuDir_.c_str(), NULL);
    if (!CopyFileW(src.c_str(), target.c_str(), FALSE)) {
        MessageBoxW(hwnd_, L"复制文件出错", L"导入失败", MB_ICONERROR);
        return;
    }
    { std::lock_guard<std::mutex> lk(cfg_.mtx); cfg_.currentFile = base; }
    save_config();
    refresh_files();
    log(L"已导入弹幕文件 " + base + L"（" + std::to_wstring(count) + L" 条）");
}

void DanmuApp::open_folder() {
    CreateDirectoryW(danmuDir_.c_str(), NULL);
    ShellExecuteW(hwnd_, L"explore", danmuDir_.c_str(), NULL, NULL, SW_SHOW);
}

// ---------- 输入框区域 ----------
void DanmuApp::pick_region() {
    if (worker_ && worker_->running()) {
        MessageBoxW(hwnd_, L"请先停止发送，再重新框选区域", L"提示", MB_ICONINFORMATION);
        return;
    }
    if (regionPicking_) return;
    regionPicking_ = true;
    RegionSelector::start(hInst_, hwnd_, this);
}

void DanmuApp::on_region_picked(bool ok, const RECT& r) {
    regionPicking_ = false;
    if (ok) {
        { std::lock_guard<std::mutex> lk(cfg_.mtx); cfg_.region = r; cfg_.hasRegion = true; }
        update_region_label();
        save_config();
        wchar_t b[128];
        swprintf(b, 128, L"输入框区域已设置：(%d, %d) → (%d, %d)",
                 (int)r.left, (int)r.top, (int)r.right, (int)r.bottom);
        log(b);
    } else {
        log(L"已取消框选");
    }
}

void DanmuApp::clear_region() {
    { std::lock_guard<std::mutex> lk(cfg_.mtx); cfg_.hasRegion = false; }
    update_region_label();
    save_config();
    log(L"已清除输入框区域");
}

// ---------- 启停 ----------
void DanmuApp::on_start() {
    if (worker_ && worker_->running()) return;
    if (loader_.path().empty()) {
        MessageBoxW(hwnd_, L"请先选择弹幕文件", L"无法开始", MB_ICONWARNING);
        return;
    }
    // 先取快照再弹窗：MessageBox 会泵消息（定时器 → update_cursor_status 也要加锁），
    // 持锁弹窗会自我死锁
    bool hasRegion;
    int jitter;
    {
        std::lock_guard<std::mutex> lk(cfg_.mtx);
        hasRegion = cfg_.hasRegion;
        jitter = cfg_.jitter;
    }
    if (!hasRegion) {
        MessageBoxW(hwnd_, L"请先点击「框选输入框区域」，框出抖音直播间的输入框",
                    L"无法开始", MB_ICONWARNING);
        return;
    }
    wchar_t b[64];
    GetWindowTextW(hInterval_, b, 64);
    double interval = wcstod(b, NULL);
    if (interval < MIN_INTERVAL) {
        interval = MIN_INTERVAL;
        updatingUI_ = true;
        swprintf(b, 64, L"%g", interval);
        SetWindowTextW(hInterval_, b);
        updatingUI_ = false;
    }
    { std::lock_guard<std::mutex> lk(cfg_.mtx); cfg_.interval = interval; }

    totalSent_ = 0;
    SetWindowTextW(hProgress_, L"本循环 -/- · 累计 0 条");
    set_running(true);
    wchar_t msg[160];
    swprintf(msg, 160,
             L"开始发送：间隔 %.1f 秒 ±%d%% 抖动，把鼠标点进抖音输入框即自动发送",
             interval, jitter);
    log(msg);
    worker_.reset(new SenderWorker(&loader_, &cfg_, hwnd_));
    worker_->start();
}

void DanmuApp::on_stop() {
    if (worker_ && worker_->running()) {
        worker_->stop();
        // 不用 ⏹/⏸ 等符号：微软雅黑缺少这些字形会显示成方块
        set_status(L"正在停止…", kOrange);
        log(L"正在停止…");
    }
}

void DanmuApp::set_running(bool running) {
    EnableWindow(hBtnStart_, !running);
    EnableWindow(hBtnStop_, running);
    EnableWindow(hFileCombo_, !running);
    if (running) set_status(L"▶ 发送中：等待光标进入输入框…", kGreen);
    else set_status(L"● 空闲", kGray);
}

// ---------- 自定义条目 ----------
void DanmuApp::add_custom() {
    if (loader_.path().empty()) {
        MessageBoxW(hwnd_, L"请先选择弹幕文件", L"无法添加", MB_ICONWARNING);
        return;
    }
    wchar_t b[512];
    GetWindowTextW(hCustom_, b, 512);
    std::wstring text = trim(b);
    if (text.empty()) return;
    std::wstring err;
    if (!loader_.add(text, err)) {
        MessageBoxW(hwnd_, err.c_str(), L"提示", MB_ICONINFORMATION);
        return;
    }
    SetWindowTextW(hCustom_, L"");
    wchar_t info[64];
    swprintf(info, 64, L"条目数: %d", loader_.size());
    SetWindowTextW(hFileInfo_, info);
    log(L"已添加自定义弹幕：" + text);
}

// ---------- 设置实时生效 ----------
void DanmuApp::on_setting_changed() {
    if (updatingUI_) return;
    wchar_t b[256];
    {
        std::lock_guard<std::mutex> lk(cfg_.mtx);
        GetWindowTextW(hInterval_, b, 64);
        double iv = wcstod(b, NULL);
        if (iv > 0) cfg_.interval = iv;
        GetWindowTextW(hJitter_, b, 64);
        int j = _wtoi(b);
        if (j >= 0 && j <= 50) cfg_.jitter = j;
        cfg_.shuffle = Button_GetCheck(hShuffle_) == BST_CHECKED;
        cfg_.fgCheck = Button_GetCheck(hFgCheck_) == BST_CHECKED;
        GetWindowTextW(hFgKeyword_, b, 256);
        std::wstring kw = trim(b);
        cfg_.fgKeyword = kw.empty() ? L"抖音" : kw;
    }
    save_config();
}

// ---------- 状态 / 日志 ----------
void DanmuApp::set_status(const std::wstring& text, COLORREF color) {
    statusColor_ = color;
    SetWindowTextW(hStatus_, text.c_str());
    InvalidateRect(hStatus_, NULL, FALSE);
}

void DanmuApp::update_cursor_status() {
    POINT pos = win_input::get_cursor_pos();
    bool in;
    { std::lock_guard<std::mutex> lk(cfg_.mtx); in = point_in_region(pos, cfg_.hasRegion, cfg_.region); }
    wchar_t b[128];
    swprintf(b, 128, in ? L"光标: (%d, %d)  ● 在输入框内" : L"光标: (%d, %d)  ○ 不在输入框内",
             (int)pos.x, (int)pos.y);
    cursorColor_ = in ? kGreen : kOrange;
    SetWindowTextW(hCursor_, b);
    InvalidateRect(hCursor_, NULL, FALSE);
}

void DanmuApp::log(const std::wstring& text) {
    SYSTEMTIME st;
    GetLocalTime(&st);
    wchar_t ts[16];
    // 注意：MinGW 严格 ANSI 模式下 swprintf 遵循 C99 —— 宽格式串里的 %s 期望
    // char* 而非 wchar_t*，会把 UTF-16 中文当字节流读出乱码。故 %s 只用于
    // 数字，文本一律用 wstring 拼接。
    swprintf(ts, 16, L"%02d:%02d:%02d", st.wHour, st.wMinute, st.wSecond);
    std::wstring line = std::wstring(ts) + L"  " + text;
    // 控制日志行数，避免无限增长
    if (ListBox_GetCount(hLog_) > 500)
        for (int i = 0; i < 100; i++) ListBox_DeleteString(hLog_, 0);
    int idx = ListBox_AddString(hLog_, line.c_str());
    ListBox_SetCurSel(hLog_, idx);   // 滚动到底部
}

void DanmuApp::save_config() {
    ::save_config(cfg_, configPath_);   // 全局函数（config.cpp），与成员函数同名
}

// ---------- 消息处理 ----------
LRESULT CALLBACK DanmuApp::WndProc(HWND h, UINT m, WPARAM w, LPARAM l) {
    auto* self = (DanmuApp*)GetWindowLongPtrW(h, GWLP_USERDATA);
    if (m == WM_NCCREATE) {
        self = (DanmuApp*)((CREATESTRUCTW*)l)->lpCreateParams;
        SetWindowLongPtrW(h, GWLP_USERDATA, (LONG_PTR)self);
    }
    // 注意把 h 传下去：WM_NCCREATE 期间成员 hwnd_ 尚未赋值，
    // 若用 hwnd_ 调 DefWindowProcW 会返回 0，导致窗口创建被中止
    return self ? self->on_message(h, m, w, l) : DefWindowProcW(h, m, w, l);
}

void DanmuApp::on_command(int id, int code) {
    switch (id) {
    case IDC_COMBO_FILE:
        if (code == CBN_SELCHANGE) on_file_changed();
        break;
    case IDC_BTN_IMPORT: import_file(); break;
    case IDC_BTN_REFRESH: refresh_files(); break;
    case IDC_BTN_OPENFOLDER: open_folder(); break;
    case IDC_BTN_PICKREGION: pick_region(); break;
    case IDC_BTN_CLEARREGION: clear_region(); break;
    case IDC_BTN_START: on_start(); break;
    case IDC_BTN_STOP: on_stop(); break;
    case IDC_BTN_ADDCUSTOM: add_custom(); break;
    case IDC_EDIT_INTERVAL:
    case IDC_EDIT_JITTER:
    case IDC_EDIT_FGKEYWORD:
        if (code == EN_CHANGE) on_setting_changed();
        break;
    case IDC_CHK_SHUFFLE:
    case IDC_CHK_FGCHECK:
        if (code == BN_CLICKED) on_setting_changed();
        break;
    }
}

LRESULT DanmuApp::on_message(HWND hwnd, UINT m, WPARAM w, LPARAM l) {
    switch (m) {
    case WM_COMMAND:
        on_command(LOWORD(w), HIWORD(w));
        return 0;

    case WM_TIMER:
        if (w == IDT_CURSOR) update_cursor_status();
        return 0;

    case WM_CTLCOLORSTATIC: {
        HDC hdc = (HDC)w;
        int id = GetDlgCtrlID((HWND)l);
        if (id == IDC_STATIC_CURSOR) SetTextColor(hdc, cursorColor_);
        else if (id == IDC_STATIC_STATUS) SetTextColor(hdc, statusColor_);
        else if (id == IDC_STATIC_FILEINFO || id == IDC_STATIC_HINT ||
                 id == IDC_STATIC_FGNOTE || id == IDC_STATIC_PROGRESS)
            SetTextColor(hdc, kGray);
        else
            return DefWindowProcW(hwnd, m, w, l);
        SetBkMode(hdc, TRANSPARENT);
        static HBRUSH bg = GetSysColorBrush(COLOR_BTNFACE);
        return (LRESULT)bg;
    }

    case WM_DANMU_SENT: {
        auto* info = (SentInfo*)l;
        totalSent_++;
        wchar_t b[160];
        swprintf(b, 160, L"▶ 发送中（第 %d/%d 条）", info->idx, info->total);
        statusColor_ = kGreen;
        SetWindowTextW(hStatus_, b);
        swprintf(b, 160, L"本循环 %d/%d · 累计 %d 条", info->idx, info->total, totalSent_);
        SetWindowTextW(hProgress_, b);
        log(info->text);
        delete info;
        return 0;
    }

    case WM_DANMU_PAUSED: {
        wchar_t b[128];
        swprintf(b, 128, L"已暂停：光标不在输入框区域内 (%d, %d)",
                 (int)GET_X_LPARAM(l), (int)GET_Y_LPARAM(l));
        statusColor_ = kOrange;
        SetWindowTextW(hStatus_, b);
        return 0;
    }

    case WM_DANMU_FGBLOCKED: {
        auto* title = (std::wstring*)l;
        set_status(L"已暂停：前台窗口不是抖音（当前：" + *title + L"）", kOrange);
        delete title;
        return 0;
    }

    case WM_DANMU_ERROR: {
        auto* err = (std::wstring*)l;
        log(L"发送出错：" + *err);
        MessageBoxW(hwnd_, err->c_str(), L"发送出错", MB_ICONERROR);
        delete err;
        return 0;
    }

    case WM_DANMU_STOPPED: {
        if (worker_) { worker_->join(); worker_.reset(); }
        set_running(false);
        log(L"已停止（本轮累计发送 " + std::to_wstring(totalSent_) + L" 条）");
        return 0;
    }

    case WM_CLOSE:
        if (worker_) { worker_->stop(); worker_->join(); worker_.reset(); }
        save_config();
        DestroyWindow(hwnd_);
        return 0;

    case WM_DESTROY:
        KillTimer(hwnd_, IDT_CURSOR);
        PostQuitMessage(0);
        return 0;
    }
    return DefWindowProcW(hwnd, m, w, l);
}
