// -*- coding: utf-8 -*-
#include "json_mini.h"
#include <cstdio>
#include <cstdlib>

namespace jm {
namespace {

struct Parser {
    const char* s;
    size_t n;
    size_t i = 0;

    [[noreturn]] void fail(const char* m) { throw std::runtime_error(m); }
    void ws() { while (i < n && (s[i] == ' ' || s[i] == '\t' || s[i] == '\r' || s[i] == '\n')) i++; }
    char peek() const { return i < n ? s[i] : '\0'; }
    bool eat(char c) { if (peek() == c) { i++; return true; } return false; }
    void expect(char c, const char* m) { if (!eat(c)) fail(m); }

    Value value() {
        ws();
        char c = peek();
        if (c == '"') { Value v; v.type = Type::String; v.str = string(); return v; }
        if (c == '[') return array();
        if (c == '{') return object();
        if (c == 't') { literal("true"); Value v; v.type = Type::Bool; v.boolean = true; return v; }
        if (c == 'f') { literal("false"); Value v; v.type = Type::Bool; return v; }
        if (c == 'n') { literal("null"); return Value{}; }
        if (c == '-' || (c >= '0' && c <= '9')) return number();
        fail("JSON 格式不正确：无法解析的值");
    }

    void literal(const char* w) {
        size_t k = i;
        for (const char* p = w; *p; ++p, ++k)
            if (k >= n || s[k] != *p) fail("JSON 格式不正确：字面量错误");
        i = k;
    }

    Value number() {
        size_t st = i;
        if (peek() == '-') i++;
        while (peek() >= '0' && peek() <= '9') i++;
        if (peek() == '.') { i++; while (peek() >= '0' && peek() <= '9') i++; }
        if (peek() == 'e' || peek() == 'E') {
            i++;
            if (peek() == '+' || peek() == '-') i++;
            while (peek() >= '0' && peek() <= '9') i++;
        }
        if (i == st || (i == st + 1 && s[st] == '-')) fail("JSON 格式不正确：数字格式错误");
        Value v; v.type = Type::Number;
        v.number = atof(std::string(s + st, i - st).c_str());
        return v;
    }

    static void utf8_push(std::string& out, unsigned cp) {
        if (cp < 0x80) out += (char)cp;
        else if (cp < 0x800) {
            out += (char)(0xC0 | (cp >> 6)); out += (char)(0x80 | (cp & 0x3F));
        } else if (cp < 0x10000) {
            out += (char)(0xE0 | (cp >> 12)); out += (char)(0x80 | ((cp >> 6) & 0x3F));
            out += (char)(0x80 | (cp & 0x3F));
        } else {
            out += (char)(0xF0 | (cp >> 18)); out += (char)(0x80 | ((cp >> 12) & 0x3F));
            out += (char)(0x80 | ((cp >> 6) & 0x3F)); out += (char)(0x80 | (cp & 0x3F));
        }
    }

    unsigned hex4() {
        if (i + 4 > n) fail("JSON 格式不正确：\\u 转义不完整");
        unsigned v = 0;
        for (int k = 0; k < 4; k++) {
            char c = s[i++];
            v <<= 4;
            if (c >= '0' && c <= '9') v |= (unsigned)(c - '0');
            else if (c >= 'a' && c <= 'f') v |= (unsigned)(c - 'a' + 10);
            else if (c >= 'A' && c <= 'F') v |= (unsigned)(c - 'A' + 10);
            else fail("JSON 格式不正确：\\u 转义非法");
        }
        return v;
    }

    std::string string() {
        expect('"', "JSON 格式不正确：期望字符串");
        std::string out;
        while (true) {
            if (i >= n) fail("JSON 格式不正确：字符串未闭合");
            char c = s[i++];
            if (c == '"') break;
            if (c != '\\') { out += c; continue; }
            if (i >= n) fail("JSON 格式不正确：转义不完整");
            char e = s[i++];
            switch (e) {
            case '"': out += '"'; break;
            case '\\': out += '\\'; break;
            case '/': out += '/'; break;
            case 'b': out += '\b'; break;
            case 'f': out += '\f'; break;
            case 'n': out += '\n'; break;
            case 'r': out += '\r'; break;
            case 't': out += '\t'; break;
            case 'u': {
                unsigned cp = hex4();
                if (cp >= 0xD800 && cp <= 0xDBFF) {   // 高代理，尝试配对低代理
                    if (i + 1 < n && s[i] == '\\' && s[i + 1] == 'u') {
                        size_t save = i;
                        i += 2;
                        unsigned lo = hex4();
                        if (lo >= 0xDC00 && lo <= 0xDFFF)
                            cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                        else
                            i = save;   // 未配对，按原样输出高代理
                    }
                }
                utf8_push(out, cp);
                break;
            }
            default: fail("JSON 格式不正确：非法转义");
            }
        }
        return out;
    }

    Value array() {
        i++;   // '['
        Value v; v.type = Type::Array;
        ws();
        if (eat(']')) return v;
        while (true) {
            v.arr.push_back(value());
            ws();
            if (eat(']')) break;
            expect(',', "JSON 格式不正确：数组元素之间缺少逗号");
        }
        return v;
    }

    Value object() {
        i++;   // '{'
        Value v; v.type = Type::Object;
        ws();
        if (eat('}')) return v;
        while (true) {
            ws();
            if (peek() != '"') fail("JSON 格式不正确：对象键必须是字符串");
            std::string key = string();
            ws();
            expect(':', "JSON 格式不正确：对象键后缺少冒号");
            v.obj[key] = value();
            ws();
            if (eat('}')) break;
            expect(',', "JSON 格式不正确：对象成员之间缺少逗号");
        }
        return v;
    }
};

} // namespace

Value parse(const std::string& text) {
    Parser p{ text.data(), text.size(), 0 };
    // 跳过 UTF-8 BOM
    if (p.n >= 3 && (unsigned char)p.s[0] == 0xEF &&
        (unsigned char)p.s[1] == 0xBB && (unsigned char)p.s[2] == 0xBF)
        p.i = 3;
    Value v = p.value();
    p.ws();
    if (p.i < p.n) throw std::runtime_error("JSON 格式不正确：末尾存在多余内容");
    return v;
}

std::string escape(const std::string& s) {
    std::string out;
    for (char ch : s) {
        unsigned char c = (unsigned char)ch;
        switch (c) {
        case '"': out += "\\\""; break;
        case '\\': out += "\\\\"; break;
        case '\b': out += "\\b"; break;
        case '\f': out += "\\f"; break;
        case '\n': out += "\\n"; break;
        case '\r': out += "\\r"; break;
        case '\t': out += "\\t"; break;
        default:
            if (c < 0x20) {
                char b[8];
                sprintf(b, "\\u%04x", c);
                out += b;
            } else {
                out += ch;
            }
        }
    }
    return out;
}

} // namespace jm
