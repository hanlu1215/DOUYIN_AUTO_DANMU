// -*- coding: utf-8 -*-
// 极简 JSON（UTF-8）解析与字符串转义。仅用于弹幕文件与 config.json，零依赖。
#pragma once
#include <map>
#include <stdexcept>
#include <string>
#include <vector>

namespace jm {

enum class Type { Null, Bool, Number, String, Array, Object };

struct Value {
    Type type = Type::Null;
    bool boolean = false;
    double number = 0.0;
    std::string str;                       // UTF-8
    std::vector<Value> arr;
    std::map<std::string, Value> obj;

    // 对象按键查找；非对象或不存在返回 nullptr
    const Value* find(const char* key) const {
        if (type != Type::Object) return nullptr;
        auto it = obj.find(key);
        return it == obj.end() ? nullptr : &it->second;
    }
};

// 解析 UTF-8 JSON 文本（自动跳过 BOM）；失败抛 std::runtime_error（中文，UTF-8）
Value parse(const std::string& text);

// 字符串内容 → JSON 转义（结果不含外层引号）
std::string escape(const std::string& s);

} // namespace jm
