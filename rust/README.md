# Rust 版弹幕助手

C++ 版（[../WIN_C/](../WIN_C/)）的 Rust 移植，功能完全一致：原生 Win32 GUI、无界面框架。

## 构建

```bash
cargo build --release
```

产物 `target/release/douyin_danmu.exe`（约 600KB），只依赖 Windows 系统 DLL，
可拷贝到其他 Win10/11 机器直接运行。

exe 会自动向上查找含 `danmu/` 的项目根目录（兼容 `target/release/` 布局），
弹幕文件与配置和 Python / C++ 版共用；把 exe 拷贝到别处运行时，会在自身目录新建 `danmu/`。

## 依赖说明

- [`windows-sys`](https://crates.io/crates/windows-sys)：微软官方 Win32 FFI 绑定（纯函数声明，无运行时库），是唯一的外部依赖
- `embed-resource`（仅构建期）：把 `app.rc`（图标 + 应用清单）编译进 exe；找不到资源编译器时自动跳过，功能不受影响（控件外观退回经典样式，DPI 感知由运行时 API 兜底）
- JSON 解析、随机数发生器均为手写实现（对应 C++ 版 json_mini / mt19937），不引入 serde / rand

## 目录结构

```
Cargo.toml               包定义 + windows-sys 特性清单
build.rs                 嵌入 app.rc（图标 + 清单）
app.rc / app.manifest    资源脚本 / 应用清单（Comctl32 v6 / PerMonitorV2）
app.ico                  图标（由 PYTHON_SRC/tools/make_icon.py 生成）
src/main.rs              入口（DPI 感知 → DanmuApp，panic/错误写入 error.log）
src/app.rs               原生 Win32 界面 + 全屏框选遮罩（对应 app.cpp）
src/sender.rs            发送线程 + 弹幕加载器 + 随机数（对应 sender.cpp）
src/win_input.rs         光标 / 剪贴板 / 键盘模拟（对应 win_input.cpp）
src/danmu_io.rs          弹幕 JSON 读写（对应 danmu_io.cpp）
src/json_mini.rs         手写 UTF-8 JSON 解析（对应 json_mini.cpp）
src/config.rs            config.json 读写（对应 config.cpp）
src/strutil.rs           UTF-8/UTF-16 互转、统一错误类型（对应 strutil.h）
```

## 与 C++ 版的差异（刻意为之）

- **配置共享用快照模式**：`cfg.snapshot()` 短暂加锁拷贝后立即释放，天然规避了 C++ 版「非递归 mutex 重入 / 持锁弹 MessageBox 自我死锁」的坑
- **线程停止用 `Mutex + Condvar`**（对应 C++ 的 event + `WaitForSingleObject`），所有休眠都可被「停止」立即中断
- **工作线程 → 界面仍用 `PostMessageW`**：lParam 负载由工作线程 `Box::into_raw` 分配、界面 WM 处理函数 `Box::from_raw` 回收，所有权语义与 C++ 的 new/delete 一一对应
- 文件枚举 / 复制走 Rust 标准库 `std::fs`（内部就是 Win32 宽字符 API），替代 `FindFirstFileW` / `CopyFileW`
