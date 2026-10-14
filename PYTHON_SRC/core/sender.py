# -*- coding: utf-8 -*-
"""发送工作线程：光标区域检测 + 频率控制 + 弹幕循环发送。"""
import random
import threading
import time

from . import win_input
from .danmu_io import load_danmu, save_danmu


def point_in_region(pos, region):
    """判断坐标 pos=(x, y) 是否在矩形 region=(x1, y1, x2, y2) 内。"""
    if not region:
        return False
    x1, y1, x2, y2 = region
    return x1 <= pos[0] <= x2 and y1 <= pos[1] <= y2


class DanmuLoader:
    """管理一个弹幕 JSON 文件：加载、循环取词、追加自定义条目。"""

    def __init__(self, path=None):
        self.path = path
        self.items = []
        self.index = 0
        if path:
            self.load(path)

    def load(self, path):
        self.items = load_danmu(path)
        self.path = path
        self.index = 0

    def next(self, shuffle_each_cycle=False):
        """取下一条弹幕；一轮发完后从头循环（可选每轮打乱顺序）。"""
        if self.index >= len(self.items):
            if shuffle_each_cycle:
                random.shuffle(self.items)
            self.index = 0
        text = self.items[self.index]
        self.index += 1
        return text

    def add(self, text):
        """追加自定义条目（去重），并写回文件。"""
        text = text.strip()
        if not text:
            return False, "内容为空"
        if text in self.items:
            return False, "该条目已存在"
        self.items.append(text)
        save_danmu(self.path, self.items)
        return True, None


class SenderWorker(threading.Thread):
    """发送线程。

    循环流程：检测光标 → 不在输入框区域内则暂停 → 校验前台窗口 →
    弹幕写入剪贴板 → Ctrl+V 粘贴 → 回车发送 → 按「基础间隔 ± 随机抖动」休眠。
    所有可调参数（region / interval / jitter / shuffle / fg_check）都实时
    读取 config 字典，界面修改后立即生效，无需重启线程。
    """

    CHECK_INTERVAL = 0.3    # 条件不满足时的轮询间隔（秒）
    MIN_DELAY = 1.0         # 两条弹幕之间的绝对下限（秒）

    def __init__(self, loader, config, events):
        super().__init__(daemon=True)
        self.loader = loader
        self.config = config      # dict，实时读取
        self.events = events      # queue.Queue，向 UI 报告状态
        self._stop = threading.Event()

    def stop(self):
        self._stop.set()

    def run(self):
        try:
            while not self._stop.is_set():
                pos = win_input.get_cursor_pos()
                if not point_in_region(pos, self.config.get("region")):
                    self.events.put(("paused", pos))
                    if self._stop.wait(self.CHECK_INTERVAL):
                        break
                    continue

                # 前台窗口校验：避免把弹幕发到其他程序里
                if self.config.get("fg_check", True):
                    keyword = str(self.config.get("fg_keyword", "抖音")).strip()
                    title = win_input.get_foreground_title()
                    if keyword and keyword.lower() not in title.lower():
                        self.events.put(("fgblocked", title))
                        if self._stop.wait(self.CHECK_INTERVAL):
                            break
                        continue

                # 取词 → 剪贴板 → 粘贴 → 回车
                text = self.loader.next(self.config.get("shuffle", False))
                time.sleep(random.uniform(0.1, 0.4))   # 随机前置延迟，节奏更接近真人
                win_input.set_clipboard(text)
                win_input.paste()
                time.sleep(0.15)                       # 等输入框收到粘贴内容
                win_input.press_enter()

                self.events.put(("sent", {
                    "text": text,
                    "idx": self.loader.index,
                    "total": len(self.loader.items),
                }))

                # 频率控制：基础间隔 × (1 ± 抖动比例)，下限 1 秒
                interval = float(self.config.get("interval", 3.0))
                jitter = float(self.config.get("jitter", 0)) / 100.0
                delay = interval * random.uniform(1 - jitter, 1 + jitter)
                delay = max(delay, self.MIN_DELAY)
                if self._stop.wait(delay):
                    break
        except Exception as exc:
            self.events.put(("error", str(exc)))
        finally:
            self.events.put(("stopped", None))
