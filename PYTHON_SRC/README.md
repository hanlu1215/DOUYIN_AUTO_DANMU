# Python 版弹幕助手

纯 Python 标准库实现（tkinter + ctypes），**无需 pip 安装任何第三方依赖**。

## 环境要求

- Windows 10 / 11
- Python 3.8+

## 启动方式

```bash
python main.py          # 在本目录（PYTHON_SRC/）下运行，或：
python PYTHON_SRC/main.py   # 在项目根目录运行也行
```

也可以双击：

- `启动助手-静默.vbs` —— 完全无窗口（推荐）
- `启动助手.bat` —— 黑窗口一闪即关

弹幕文件和配置在项目根目录的 `danmu/`、`config.json`（程序会自动向上查找含 `danmu/` 的项目根目录），与 C++ / Rust 版共用。

## 常用命令

```bash
python main.py                          # 运行（GUI）
python tests/test_basic.py              # 自检：弹幕文件、光标、剪贴板、区域逻辑
python tools/generate_chenbo_danmu.py   # 重新生成 danmu/陈伯夸夸.json（2000 条不重复）
python tools/make_icon.py               # 重新生成应用图标（WIN_C/src/app.ico 和 rust/app.ico）
```

## 目录结构

```
main.py                    程序入口（DPI 感知 → DanmuApp，异常写入 error.log）
app.py                     tkinter 图形界面
core/win_input.py          光标 / 剪贴板 / 键盘模拟（ctypes）
core/sender.py             发送线程 + 频率控制 + 循环
core/danmu_io.py           弹幕 JSON 读写
tests/test_basic.py        基础自检（会真实读写剪贴板，仅 Windows 可运行）
tools/generate_chenbo_danmu.py   弹幕生成器
tools/make_icon.py         图标生成器
启动助手.bat / 启动助手-静默.vbs   pythonw 启动器
```

## 弹幕生成器约束

`tools/generate_chenbo_danmu.py` 生成 2000 条不重复夸夸弹幕，风格模仿男网友真实弹幕（兄弟口吻、自嘲玩梗、扣1排面）：

- 称呼只用 **伯哥 / 俊伯 / 陈伯**
- 明星类比只用 **彭于晏 / 林俊杰 / 吴彦祖**
- **不含身体部位类夸奖**（眼睛/鼻梁/喉结/身材等一律不要）
