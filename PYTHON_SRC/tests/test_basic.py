# -*- coding: utf-8 -*-
"""基础自检：弹幕文件、剪贴板、光标位置、区域判断。"""
import os
import sys

sys.path.insert(0, os.path.dirname(os.path.dirname(os.path.abspath(__file__))))

from core import win_input
from core.danmu_io import load_danmu
from core.sender import point_in_region


def _find_project_root(start):
    """向上最多找 4 层，定位含 danmu/ 的项目根目录（本文件位于 PYTHON_SRC/tests/ 下）。"""
    d = start
    for _ in range(4):
        if os.path.isdir(os.path.join(d, "danmu")):
            return d
        nd = os.path.dirname(d)
        if nd == d:
            break
        d = nd
    return start


def main():
    win_input.set_process_dpi_aware()

    # 1. 弹幕文件
    path = os.path.join(_find_project_root(os.path.dirname(os.path.abspath(__file__))),
                        "danmu", "陈伯夸夸.json")
    items = load_danmu(path)
    assert len(items) >= 1000, "弹幕不足 1000 条：%d" % len(items)
    assert len(items) == len(set(items)), "弹幕存在重复"
    print("OK 弹幕文件: %d 条，无重复" % len(items))

    # 2. 光标位置
    pos = win_input.get_cursor_pos()
    assert isinstance(pos, tuple) and len(pos) == 2 and all(isinstance(v, int) for v in pos)
    print("OK 光标位置: %s" % (pos,))

    # 3. 前台窗口标题
    title = win_input.get_foreground_title()
    print("OK 前台窗口标题: %s" % title)

    # 4. 剪贴板（借助 tkinter 读回校验）
    import tkinter as tk
    root = tk.Tk()
    root.withdraw()
    win_input.set_clipboard("测试弹幕123")
    got = root.clipboard_get()
    assert got == "测试弹幕123", "剪贴板内容不匹配: %r" % got
    root.destroy()
    print("OK 剪贴板读写")

    # 5. 区域判断
    assert point_in_region((50, 60), [0, 0, 100, 100])
    assert not point_in_region((150, 60), [0, 0, 100, 100])
    assert not point_in_region((0, 0), None)
    print("OK 区域判断")

    print("全部自检通过")


if __name__ == "__main__":
    main()
