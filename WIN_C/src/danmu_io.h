// -*- coding: utf-8 -*-
// 弹幕 JSON 文件的读写（UTF-8）。
#pragma once
#include <string>
#include <vector>

// 读取弹幕 JSON，返回去重、去除空白后的列表；格式错误抛 AppError。
// 支持两种格式：
//   ["弹幕1", "弹幕2", ...]
//   {"items": [...]} 或 {"danmu": [...]}
std::vector<std::wstring> load_danmu(const std::wstring& path);

// 保存弹幕列表：每行一条，方便手动编辑；失败抛 AppError
void save_danmu(const std::wstring& path, const std::vector<std::wstring>& items);
