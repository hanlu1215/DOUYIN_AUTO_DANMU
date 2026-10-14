// -*- coding: utf-8 -*-
#include "danmu_io.h"
#include "json_mini.h"
#include "strutil.h"
#include <filesystem>
#include <fstream>
#include <set>

std::vector<std::wstring> load_danmu(const std::wstring& path) {
    std::ifstream f(std::filesystem::path(path), std::ios::binary);
    if (!f) throw AppError{ L"无法打开文件：" + path };
    std::string text((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());

    jm::Value root;
    try {
        root = jm::parse(text);
    } catch (const std::runtime_error& e) {
        throw AppError{ utf8_to_wide(e.what()) };
    }

    const jm::Value* list = &root;
    if (root.type == jm::Type::Object) {
        list = root.find("items");
        if (!list) list = root.find("danmu");
    }
    if (!list || list->type != jm::Type::Array)
        throw AppError{ L"JSON 格式不正确：顶层需要是字符串数组，或包含 items/danmu 数组的对象" };

    std::vector<std::wstring> items;
    std::set<std::wstring> seen;
    for (const auto& el : list->arr) {
        if (el.type != jm::Type::String)
            throw AppError{ L"JSON 格式不正确：数组中存在非字符串元素" };
        std::wstring t = trim(utf8_to_wide(el.str));
        if (!t.empty() && !seen.count(t)) {
            seen.insert(t);
            items.push_back(std::move(t));
        }
    }
    if (items.empty()) throw AppError{ L"弹幕文件为空" };
    return items;
}

void save_danmu(const std::wstring& path, const std::vector<std::wstring>& items) {
    std::ofstream f(std::filesystem::path(path), std::ios::binary | std::ios::trunc);
    if (!f) throw AppError{ L"无法写入文件：" + path };
    f << "[\n";
    for (size_t i = 0; i < items.size(); i++) {
        f << "  \"" << jm::escape(wide_to_utf8(items[i])) << "\""
          << (i + 1 < items.size() ? "," : "") << "\n";
    }
    f << "]\n";
    if (!f) throw AppError{ L"写入文件出错：" + path };
}
