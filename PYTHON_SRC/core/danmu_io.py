# -*- coding: utf-8 -*-
"""弹幕 JSON 文件的读写。"""
import json


def load_danmu(path):
    """读取弹幕 JSON，返回去重、去除空白后的字符串列表。

    支持两种格式：
      ["弹幕1", "弹幕2", ...]
      {"items": ["弹幕1", ...]}  或  {"danmu": [...]}
    """
    with open(path, "r", encoding="utf-8") as f:
        data = json.load(f)
    if isinstance(data, dict):
        data = data.get("items") or data.get("danmu")
    if not isinstance(data, list):
        raise ValueError("JSON 格式不正确：顶层需要是字符串数组，或包含 items/danmu 数组的对象")
    items, seen = [], set()
    for x in data:
        if not isinstance(x, str):
            raise ValueError("JSON 格式不正确：数组中存在非字符串元素")
        x = x.strip()
        if x and x not in seen:
            items.append(x)
            seen.add(x)
    if not items:
        raise ValueError("弹幕文件为空")
    return items


def save_danmu(path, items):
    """保存弹幕列表：每行一条，方便手动编辑。"""
    with open(path, "w", encoding="utf-8") as f:
        f.write("[\n")
        for i, item in enumerate(items):
            comma = "," if i < len(items) - 1 else ""
            f.write("  %s%s\n" % (json.dumps(item, ensure_ascii=False), comma))
        f.write("]\n")
