// -*- coding: utf-8 -*-
#include "config.h"
#include "json_mini.h"
#include "strutil.h"
#include <cstdio>
#include <filesystem>
#include <fstream>

void load_config(AppConfig& cfg, const std::wstring& path) {
    std::ifstream f(std::filesystem::path(path), std::ios::binary);
    if (!f) return;
    std::string text((std::istreambuf_iterator<char>(f)), std::istreambuf_iterator<char>());

    jm::Value root;
    try {
        root = jm::parse(text);
    } catch (...) {
        return;   // 配置损坏时忽略，用默认值
    }
    if (root.type != jm::Type::Object) return;

    std::lock_guard<std::mutex> lk(cfg.mtx);
    if (const jm::Value* r = root.find("region")) {
        if (r->type == jm::Type::Array && r->arr.size() == 4) {
            cfg.region.left = (long)r->arr[0].number;
            cfg.region.top = (long)r->arr[1].number;
            cfg.region.right = (long)r->arr[2].number;
            cfg.region.bottom = (long)r->arr[3].number;
            cfg.hasRegion = true;
        }
    }
    if (const jm::Value* v = root.find("interval"))
        if (v->type == jm::Type::Number && v->number > 0) cfg.interval = v->number;
    if (const jm::Value* v = root.find("jitter"))
        if (v->type == jm::Type::Number) cfg.jitter = (int)v->number;
    if (const jm::Value* v = root.find("shuffle"))
        if (v->type == jm::Type::Bool) cfg.shuffle = v->boolean;
    if (const jm::Value* v = root.find("fg_check"))
        if (v->type == jm::Type::Bool) cfg.fgCheck = v->boolean;
    if (const jm::Value* v = root.find("fg_keyword"))
        if (v->type == jm::Type::String) cfg.fgKeyword = utf8_to_wide(v->str);
    if (const jm::Value* v = root.find("current_file"))
        if (v->type == jm::Type::String) cfg.currentFile = utf8_to_wide(v->str);
}

void save_config(AppConfig& cfg, const std::wstring& path) {
    std::string out;
    {
        std::lock_guard<std::mutex> lk(cfg.mtx);
        char b[256];
        out = "{\n  \"region\": ";
        if (cfg.hasRegion) {
            sprintf(b, "[%ld, %ld, %ld, %ld]",
                    cfg.region.left, cfg.region.top, cfg.region.right, cfg.region.bottom);
            out += b;
        } else {
            out += "null";
        }
        sprintf(b, ",\n  \"interval\": %g,\n  \"jitter\": %d,\n", cfg.interval, cfg.jitter);
        out += b;
        out += std::string("  \"shuffle\": ") + (cfg.shuffle ? "true" : "false") + ",\n";
        out += std::string("  \"fg_check\": ") + (cfg.fgCheck ? "true" : "false") + ",\n";
        out += "  \"fg_keyword\": \"" + jm::escape(wide_to_utf8(cfg.fgKeyword)) + "\",\n";
        out += "  \"current_file\": ";
        out += cfg.currentFile.empty() ? "null"
                                       : "\"" + jm::escape(wide_to_utf8(cfg.currentFile)) + "\"";
        out += "\n}\n";
    }
    std::ofstream f(std::filesystem::path(path), std::ios::binary | std::ios::trunc);
    if (f) f << out;
}
