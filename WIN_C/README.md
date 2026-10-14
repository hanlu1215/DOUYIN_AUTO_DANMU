# C++ 版弹幕助手

原生 Win32 GUI 的 C++17 实现，CMake 构建，**无第三方依赖**（JSON 解析为手写实现）。
静态链接 C/C++ 运行时，产物 exe 只依赖 Windows 系统 DLL，可拷贝到其他 Win10/11 机器直接运行。

## 构建

```bash
# MinGW / MSYS2 UCRT64
cmake -B build -G "MinGW Makefiles" -DCMAKE_BUILD_TYPE=Release
cmake --build build

# 或 Visual Studio
cmake -B build -G "Visual Studio 17 2022"
cmake --build build --config Release
```

产物 `build/douyin_danmu.exe`（VS 为 `build/<Config>/douyin_danmu.exe`）。

exe 会自动向上查找含 `danmu/` 的项目根目录（兼容 `build/`、`build/Release/` 布局），
弹幕文件与配置和 Python / Rust 版共用；把 exe 拷贝到别处运行时，会在自身目录新建 `danmu/`。

## 目录结构

```
CMakeLists.txt           构建脚本（静态链接、/utf-8、WINVER=0x0A00）
src/main.cpp             入口（DPI 感知 → DanmuApp，异常写入 error.log）
src/app.cpp / app.h      原生 Win32 界面 + 全屏框选遮罩
src/sender.cpp / sender.h  发送线程（PostMessage 通知界面）
src/win_input.cpp/.h     光标 / 剪贴板 / 键盘模拟
src/danmu_io.cpp/.h      弹幕 JSON 读写
src/json_mini.cpp/.h     手写 UTF-8 JSON（无第三方依赖）
src/config.cpp/.h        config.json 读写
src/strutil.h            UTF-8/UTF-16 互转、trim、统一错误类型
src/app.rc               图标 + 应用清单（Comctl32 v6 视觉样式 / PerMonitorV2）
src/app.ico              图标（由 PYTHON_SRC/tools/make_icon.py 生成）
```

## 移植要点（踩过的坑）

- **DefWindowProc 必须收到真实 hwnd**：`WM_NCCREATE` 期间成员 `hwnd_` 还是 NULL，传它会中止窗口创建
- **`cfg_.mtx` 是非递归 `std::mutex`**：持锁时不能再调会加锁的函数，也不能泵消息（MessageBox），否则与 150ms 光标定时器自我死锁 —— 先取快照、释放锁、再操作 UI
- **MinGW 的 `swprintf` 不要用 `%s` 传宽字符串**（严格 ANSI 下按 C99 语义，`%s` 期望 `char*`，会把中文搅成乱码）：数字用 `swprintf`，文本用 `std::wstring` 拼接
- **日志/状态字体必须含中文字形**（用微软雅黑，不要用 Consolas）；界面文字避免 ⏸/⏹ 类符号（雅黑缺字形）
- **不要在 main.cpp 里用 `#pragma` 注入 MSVC 清单**，会与 app.rc 嵌入的清单冲突
