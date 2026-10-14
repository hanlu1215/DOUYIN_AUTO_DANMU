# -*- coding: utf-8 -*-
"""抖音直播自动弹幕助手 —— tkinter 图形界面。

纯标准库实现（tkinter + ctypes），无需安装第三方依赖。
"""
import json
import os
import queue
import shutil
import time
import tkinter as tk
from tkinter import filedialog, messagebox, ttk

from core import win_input
from core.danmu_io import load_danmu
from core.sender import DanmuLoader, SenderWorker, point_in_region

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


APP_DIR = os.path.dirname(os.path.abspath(__file__))
PROJECT_ROOT = _find_project_root(APP_DIR)
DANMU_DIR = os.path.join(PROJECT_ROOT, "danmu")
CONFIG_PATH = os.path.join(PROJECT_ROOT, "config.json")

MIN_INTERVAL = 2.0   # 抖音直播发言频率的安全下限（秒），界面不允许低于此值

DEFAULT_CONFIG = {
    "region": None,        # 输入框区域 [x1, y1, x2, y2]，None 表示未设置
    "interval": 10.0,      # 发送基础间隔（秒），默认 10 秒
    "jitter": 20,          # 随机抖动（百分比，如 20 表示 ±20%）
    "shuffle": True,       # 每轮循环随机打乱顺序，默认开启
    "fg_check": True,      # 是否校验前台窗口标题
    "fg_keyword": "抖音",   # 前台窗口标题需包含的关键词
    "current_file": None,  # 当前弹幕文件名
}


class RegionSelector(tk.Toplevel):
    """全屏半透明遮罩，拖拽框选抖音输入框区域。"""

    def __init__(self, master, callback):
        super().__init__(master)
        self.callback = callback
        self.overrideredirect(True)
        self.attributes("-topmost", True)
        self.attributes("-alpha", 0.35)
        ox, oy, w, h = win_input.get_virtual_screen()
        self.origin = (ox, oy)
        self.geometry("%dx%d+%d+%d" % (w, h, ox, oy))
        self.canvas = tk.Canvas(self, bg="black", highlightthickness=0, cursor="crosshair")
        self.canvas.pack(fill="both", expand=True)
        self.canvas.create_text(
            w // 2, 90,
            text="按住鼠标左键拖拽，框选出抖音直播间的输入框\n松开鼠标完成框选，按 Esc 取消",
            fill="white", font=("Microsoft YaHei", 16), justify="center")
        self.start = None
        self.rect = None
        self.canvas.bind("<ButtonPress-1>", self._on_press)
        self.canvas.bind("<B1-Motion>", self._on_drag)
        self.canvas.bind("<ButtonRelease-1>", self._on_release)
        for wgt in (self, self.canvas):
            wgt.bind("<Escape>", lambda e: self._finish(None))
        self.focus_force()

    def _on_press(self, e):
        self.start = (e.x, e.y)

    def _on_drag(self, e):
        if self.rect:
            self.canvas.delete(self.rect)
        self.rect = self.canvas.create_rectangle(
            self.start[0], self.start[1], e.x, e.y, outline="#ff3b30", width=3)

    def _on_release(self, e):
        x1, y1 = self.start
        x2, y2 = e.x, e.y
        if abs(x2 - x1) < 10 or abs(y2 - y1) < 10:
            self._finish(None)   # 矩形太小，视为误操作
        else:
            self._finish([
                min(x1, x2) + self.origin[0], min(y1, y2) + self.origin[1],
                max(x1, x2) + self.origin[0], max(y1, y2) + self.origin[1],
            ])

    def _finish(self, region):
        self.destroy()
        self.callback(region)


class DanmuApp:
    def __init__(self):
        self.root = tk.Tk()
        self.root.title("抖音直播自动弹幕助手")
        self.root.geometry("680x700")
        self.root.minsize(640, 620)

        self.config = self._load_config()
        self.events = queue.Queue()
        self.worker = None
        self.loader = None
        self.total_sent = 0

        self._build_ui()
        self._refresh_files()
        self.root.protocol("WM_DELETE_WINDOW", self._on_close)
        self.root.after(100, self._consume_events)
        self.root.after(150, self._update_cursor_status)

    # ---------- 配置读写 ----------
    def _load_config(self):
        cfg = dict(DEFAULT_CONFIG)
        try:
            with open(CONFIG_PATH, "r", encoding="utf-8") as f:
                cfg.update(json.load(f))
        except Exception:
            pass
        return cfg

    def _save_config(self):
        try:
            with open(CONFIG_PATH, "w", encoding="utf-8") as f:
                json.dump(self.config, f, ensure_ascii=False, indent=2)
        except Exception:
            pass

    # ---------- 界面搭建 ----------
    def _build_ui(self):
        root = self.root
        root.columnconfigure(0, weight=1)
        root.rowconfigure(5, weight=1)

        # ---- 弹幕文件 ----
        frm_file = ttk.LabelFrame(root, text="弹幕文件（danmu 文件夹）")
        frm_file.grid(row=0, column=0, sticky="ew", padx=10, pady=(10, 4))
        frm_file.columnconfigure(1, weight=1)
        self.file_var = tk.StringVar()
        self.file_combo = ttk.Combobox(frm_file, textvariable=self.file_var,
                                       state="readonly", width=30)
        self.file_combo.grid(row=0, column=1, sticky="ew", padx=4, pady=6)
        self.file_combo.bind("<<ComboboxSelected>>", lambda e: self._on_file_changed())
        ttk.Button(frm_file, text="导入", width=6, command=self._import_file).grid(row=0, column=2, padx=(0, 4))
        ttk.Button(frm_file, text="刷新", width=6, command=self._refresh_files).grid(row=0, column=3, padx=(0, 4))
        ttk.Button(frm_file, text="打开文件夹", width=10, command=self._open_folder).grid(row=0, column=4, padx=(0, 6))
        self.file_info_var = tk.StringVar(value="条目数: -")
        ttk.Label(frm_file, textvariable=self.file_info_var, foreground="#888888").grid(
            row=1, column=1, columnspan=4, sticky="w", padx=4, pady=(0, 6))

        # ---- 发送设置 ----
        frm_set = ttk.LabelFrame(root, text="发送设置")
        frm_set.grid(row=1, column=0, sticky="ew", padx=10, pady=4)
        self.interval_var = tk.DoubleVar(value=float(self.config["interval"]))
        self.jitter_var = tk.IntVar(value=int(self.config["jitter"]))
        self.shuffle_var = tk.BooleanVar(value=bool(self.config["shuffle"]))
        self.fg_check_var = tk.BooleanVar(value=bool(self.config.get("fg_check", True)))
        self.fg_kw_var = tk.StringVar(value=str(self.config.get("fg_keyword", "抖音")))

        ttk.Label(frm_set, text="发送间隔（秒）").grid(row=0, column=0, sticky="w", padx=(4, 2), pady=6)
        ttk.Spinbox(frm_set, from_=MIN_INTERVAL, to=60.0, increment=0.5,
                    textvariable=self.interval_var, width=6).grid(row=0, column=1, padx=(0, 10))
        ttk.Label(frm_set, text="随机抖动 ±").grid(row=0, column=2, sticky="w")
        ttk.Spinbox(frm_set, from_=0, to=50, increment=5,
                    textvariable=self.jitter_var, width=5).grid(row=0, column=3, padx=(2, 2))
        ttk.Label(frm_set, text="%").grid(row=0, column=4, sticky="w", padx=(0, 10))
        ttk.Checkbutton(frm_set, text="每轮随机打乱", variable=self.shuffle_var).grid(row=0, column=5, padx=6)
        ttk.Checkbutton(frm_set, text="仅当前台窗口标题包含", variable=self.fg_check_var).grid(
            row=1, column=0, columnspan=2, sticky="w", padx=(4, 0), pady=(0, 6))
        ttk.Entry(frm_set, textvariable=self.fg_kw_var, width=10).grid(row=1, column=2, columnspan=2, sticky="w")
        ttk.Label(frm_set, text="时才发送（防误发到其他程序）", foreground="#888888").grid(
            row=1, column=4, columnspan=2, sticky="w")
        ttk.Label(frm_set, text="提示：抖音直播发言太快会被限制（提示「发言太快」），建议间隔 ≥ 2~3 秒",
                  foreground="#888888").grid(row=2, column=0, columnspan=6, sticky="w", padx=4, pady=(0, 6))

        for var in (self.interval_var, self.jitter_var, self.shuffle_var,
                    self.fg_check_var, self.fg_kw_var):
            var.trace_add("write", lambda *a: self._on_setting_changed())

        # ---- 输入框区域 ----
        frm_region = ttk.LabelFrame(root, text="输入框区域（光标检测）")
        frm_region.grid(row=2, column=0, sticky="ew", padx=10, pady=4)
        frm_region.columnconfigure(0, weight=1)
        region = self.config.get("region")
        self.region_var = tk.StringVar(
            value="(%d, %d) → (%d, %d)" % tuple(region) if region else "未设置")
        ttk.Label(frm_region, textvariable=self.region_var).grid(row=0, column=0, sticky="w", padx=4, pady=6)
        ttk.Button(frm_region, text="框选输入框区域", command=self._pick_region).grid(row=0, column=1, padx=4)
        ttk.Button(frm_region, text="清除", width=6, command=self._clear_region).grid(row=0, column=2, padx=(0, 6))
        self.cursor_var = tk.StringVar(value="光标: (0, 0)")
        self.cursor_label = ttk.Label(frm_region, textvariable=self.cursor_var, foreground="#888888")
        self.cursor_label.grid(row=1, column=0, columnspan=3, sticky="w", padx=4, pady=(0, 6))

        # ---- 控制 ----
        frm_ctrl = ttk.Frame(root)
        frm_ctrl.grid(row=3, column=0, sticky="ew", padx=10, pady=4)
        frm_ctrl.columnconfigure(4, weight=1)
        self.btn_start = tk.Button(frm_ctrl, text="▶ 开始发送", bg="#34c759", fg="white",
                                   font=("Microsoft YaHei", 11, "bold"), width=14,
                                   command=self._on_start)
        self.btn_start.grid(row=0, column=0, padx=(0, 6), pady=4)
        self.btn_stop = tk.Button(frm_ctrl, text="■ 停止", bg="#ff3b30", fg="white",
                                  font=("Microsoft YaHei", 11, "bold"), width=14,
                                  state="disabled", command=self._on_stop)
        self.btn_stop.grid(row=0, column=1, padx=(0, 10))
        self.status_var = tk.StringVar(value="● 空闲")
        self.status_label = ttk.Label(frm_ctrl, textvariable=self.status_var, foreground="#888888")
        self.status_label.grid(row=0, column=2, sticky="w")
        self.progress_var = tk.StringVar(value="本循环 -/- · 累计 0 条")
        ttk.Label(frm_ctrl, textvariable=self.progress_var, foreground="#888888").grid(
            row=0, column=4, sticky="e", padx=4)

        # ---- 自定义弹幕 ----
        frm_custom = ttk.LabelFrame(root, text="自定义弹幕（追加到当前文件，立即生效，自动去重）")
        frm_custom.grid(row=4, column=0, sticky="ew", padx=10, pady=4)
        frm_custom.columnconfigure(0, weight=1)
        self.custom_var = tk.StringVar()
        entry_custom = ttk.Entry(frm_custom, textvariable=self.custom_var)
        entry_custom.grid(row=0, column=0, sticky="ew", padx=4, pady=6)
        entry_custom.bind("<Return>", lambda e: self._add_custom())
        ttk.Button(frm_custom, text="添加到当前文件", command=self._add_custom).grid(
            row=0, column=1, padx=(0, 6))

        # ---- 日志 ----
        frm_log = ttk.LabelFrame(root, text="日志")
        frm_log.grid(row=5, column=0, sticky="nsew", padx=10, pady=(4, 10))
        frm_log.rowconfigure(0, weight=1)
        frm_log.columnconfigure(0, weight=1)
        self.log_text = tk.Text(frm_log, height=10, state="disabled",
                                font=("Consolas", 9), wrap="word")
        self.log_text.grid(row=0, column=0, sticky="nsew", padx=(4, 0), pady=(0, 4))
        sb = ttk.Scrollbar(frm_log, command=self.log_text.yview)
        sb.grid(row=0, column=1, sticky="ns", padx=(0, 4), pady=(0, 4))
        self.log_text.config(yscrollcommand=sb.set)
        self.log_text.tag_config("info", foreground="#888888")
        self.log_text.tag_config("send", foreground="#111111")
        self.log_text.tag_config("ok", foreground="#2e7d32")
        self.log_text.tag_config("warn", foreground="#e65100")
        self.log_text.tag_config("err", foreground="#c62828")

    # ---------- 弹幕文件管理 ----------
    def _refresh_files(self):
        os.makedirs(DANMU_DIR, exist_ok=True)
        files = sorted(f for f in os.listdir(DANMU_DIR) if f.lower().endswith(".json"))
        self.file_combo["values"] = files
        cur = self.config.get("current_file")
        if cur in files:
            self.file_combo.set(cur)
        elif files:
            self.file_combo.set(files[0])
        else:
            self.file_combo.set("")
        self._on_file_changed()

    def _on_file_changed(self):
        name = self.file_var.get()
        if not name:
            self.loader = None
            self.file_info_var.set("条目数: -")
            return
        try:
            self.loader = DanmuLoader(os.path.join(DANMU_DIR, name))
        except Exception as exc:
            self.loader = None
            self.file_info_var.set("条目数: 加载失败")
            messagebox.showerror("弹幕文件加载失败", str(exc))
            return
        self.config["current_file"] = name
        self._save_config()
        self.file_info_var.set("条目数: %d" % len(self.loader.items))
        self._log("已加载弹幕文件 %s（%d 条）" % (name, len(self.loader.items)), "info")

    def _import_file(self):
        path = filedialog.askopenfilename(
            title="选择弹幕 JSON 文件",
            filetypes=[("JSON 文件", "*.json"), ("所有文件", "*.*")])
        if not path:
            return
        try:
            items = load_danmu(path)   # 仅校验格式
        except Exception as exc:
            messagebox.showerror("导入失败", str(exc))
            return
        target = os.path.join(DANMU_DIR, os.path.basename(path))
        if os.path.exists(target) and os.path.abspath(path) != os.path.abspath(target):
            if not messagebox.askyesno("覆盖确认", "弹幕文件夹中已存在同名文件，是否覆盖？"):
                return
        try:
            shutil.copyfile(path, target)
        except Exception as exc:
            messagebox.showerror("导入失败", "复制文件出错：%s" % exc)
            return
        self.config["current_file"] = os.path.basename(target)
        self._save_config()
        self._refresh_files()
        self._log("已导入弹幕文件 %s（%d 条）" % (os.path.basename(target), len(items)), "ok")

    def _open_folder(self):
        os.makedirs(DANMU_DIR, exist_ok=True)
        os.startfile(DANMU_DIR)

    # ---------- 输入框区域 ----------
    def _pick_region(self):
        if self.worker and self.worker.is_alive():
            messagebox.showinfo("提示", "请先停止发送，再重新框选区域")
            return
        RegionSelector(self.root, self._on_region_picked)

    def _on_region_picked(self, region):
        if region:
            self.config["region"] = region
            self.region_var.set("(%d, %d) → (%d, %d)" % tuple(region))
            self._save_config()
            self._log("输入框区域已设置：%s" % self.region_var.get(), "ok")
        else:
            self._log("已取消框选", "info")

    def _clear_region(self):
        self.config["region"] = None
        self.region_var.set("未设置")
        self._save_config()
        self._log("已清除输入框区域", "info")

    # ---------- 启停 ----------
    def _on_start(self):
        if self.worker and self.worker.is_alive():
            return
        if not self.loader:
            messagebox.showwarning("无法开始", "请先选择弹幕文件")
            return
        if not self.config.get("region"):
            messagebox.showwarning("无法开始", "请先点击「框选输入框区域」，框出抖音直播间的输入框")
            return
        try:
            interval = float(self.interval_var.get())
        except Exception:
            interval = MIN_INTERVAL
        if interval < MIN_INTERVAL:
            interval = MIN_INTERVAL
            self.interval_var.set(interval)
        self.config["interval"] = interval
        self.total_sent = 0
        self.progress_var.set("本循环 -/- · 累计 0 条")
        self._set_running(True)
        self._log("开始发送：间隔 %.1f 秒 ±%d%% 抖动，把鼠标点进抖音输入框即自动发送"
                  % (interval, int(self.jitter_var.get())), "ok")
        self.worker = SenderWorker(self.loader, self.config, self.events)
        self.worker.start()

    def _on_stop(self):
        if self.worker and self.worker.is_alive():
            self.worker.stop()
            self.status_var.set("⏹ 正在停止…")
            self._log("正在停止…", "warn")

    def _set_running(self, running):
        if running:
            self.btn_start.config(state="disabled")
            self.btn_stop.config(state="normal")
            self.file_combo.config(state="disabled")
            self.status_var.set("▶ 发送中：等待光标进入输入框…")
            self.status_label.config(foreground="#2e7d32")
        else:
            self.btn_start.config(state="normal")
            self.btn_stop.config(state="disabled")
            self.file_combo.config(state="readonly")
            self.status_var.set("● 空闲")
            self.status_label.config(foreground="#888888")

    # ---------- 自定义条目 ----------
    def _add_custom(self):
        if not self.loader:
            messagebox.showwarning("无法添加", "请先选择弹幕文件")
            return
        text = self.custom_var.get().strip()
        if not text:
            return
        ok, err = self.loader.add(text)
        if not ok:
            messagebox.showinfo("提示", err)
            return
        self.custom_var.set("")
        self.file_info_var.set("条目数: %d" % len(self.loader.items))
        self._log("已添加自定义弹幕：%s" % text, "ok")

    # ---------- 设置实时生效 ----------
    def _on_setting_changed(self):
        try:
            self.config["interval"] = float(self.interval_var.get())
            self.config["jitter"] = int(self.jitter_var.get())
            self.config["shuffle"] = bool(self.shuffle_var.get())
            self.config["fg_check"] = bool(self.fg_check_var.get())
            self.config["fg_keyword"] = str(self.fg_kw_var.get()).strip() or "抖音"
            self._save_config()
        except Exception:
            pass

    # ---------- 状态刷新 ----------
    def _consume_events(self):
        try:
            while True:
                kind, payload = self.events.get_nowait()
                if kind == "sent":
                    self.total_sent += 1
                    self.status_var.set("▶ 发送中（第 %d/%d 条）" % (payload["idx"], payload["total"]))
                    self.progress_var.set("本循环 %d/%d · 累计 %d 条"
                                          % (payload["idx"], payload["total"], self.total_sent))
                    self._log(payload["text"], "send")
                elif kind == "paused":
                    self.status_var.set("⏸ 已暂停：光标不在输入框区域内 (%d, %d)" % payload)
                elif kind == "fgblocked":
                    self.status_var.set("⏸ 已暂停：前台窗口不是抖音（当前：%s）" % payload)
                elif kind == "error":
                    self._log("发送出错：%s" % payload, "err")
                    messagebox.showerror("发送出错", payload)
                elif kind == "stopped":
                    self.worker = None
                    self._set_running(False)
                    self._log("已停止（本轮累计发送 %d 条）" % self.total_sent, "info")
        except queue.Empty:
            pass
        self.root.after(100, self._consume_events)

    def _update_cursor_status(self):
        pos = win_input.get_cursor_pos()
        if point_in_region(pos, self.config.get("region")):
            self.cursor_var.set("光标: (%d, %d)  ● 在输入框内" % pos)
            self.cursor_label.config(foreground="#2e7d32")
        else:
            self.cursor_var.set("光标: (%d, %d)  ○ 不在输入框内" % pos)
            self.cursor_label.config(foreground="#e65100")
        self.root.after(150, self._update_cursor_status)

    # ---------- 日志 ----------
    def _log(self, text, tag=None):
        self.log_text.config(state="normal")
        self.log_text.insert("end", "%s  %s\n" % (time.strftime("%H:%M:%S"), text), tag or "info")
        # 控制日志行数，避免无限增长
        if int(self.log_text.index("end-1c").split(".")[0]) > 500:
            self.log_text.delete("1.0", "100.0")
        self.log_text.see("end")
        self.log_text.config(state="disabled")

    # ---------- 退出 ----------
    def _on_close(self):
        if self.worker and self.worker.is_alive():
            self.worker.stop()
            self.worker.join(timeout=2)
        self._save_config()
        self.root.destroy()

    def run(self):
        self.root.mainloop()
