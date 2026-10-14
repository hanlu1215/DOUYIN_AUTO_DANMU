# -*- coding: utf-8 -*-
"""抖音直播自动弹幕助手 —— 程序入口。

运行：python main.py（或双击 启动助手.bat / 启动助手-静默.vbs）
"""
import os
import sys
import traceback

from core import win_input
from app import DanmuApp

APP_DIR = os.path.dirname(os.path.abspath(__file__))


def _find_project_root(start):
    """向上最多找 4 层，定位含 danmu/ 的项目根目录（本文件位于 PYTHON_SRC/ 下）。"""
    d = start
    for _ in range(4):
        if os.path.isdir(os.path.join(d, "danmu")):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return start


ERROR_LOG = os.path.join(_find_project_root(APP_DIR), "error.log")


def main():
    try:
        win_input.set_process_dpi_aware()   # 必须在创建 Tk 窗口前调用
        DanmuApp().run()
    except Exception:
        # 用 pythonw 启动时没有控制台，异常写入 error.log 方便排查
        try:
            with open(ERROR_LOG, "w", encoding="utf-8") as f:
                traceback.print_exc(file=f)
        except Exception:
            pass
        raise


if __name__ == "__main__":
    main()
