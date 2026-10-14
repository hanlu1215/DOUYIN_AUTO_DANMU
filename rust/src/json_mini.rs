//! 手写 UTF-8 JSON 解析 / 转义 —— 无第三方依赖（对应 C++ 版 json_mini）。
//! 输入为 &str（已是合法 UTF-8），字符串值直接产出 Rust String。

use std::collections::BTreeMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Bool(bool),
    Number(f64),
    Str(String),
    Array(Vec<Value>),
    Object(BTreeMap<String, Value>),
}

impl Value {
    /// 对象按键查找；非对象返回 None
    pub fn find(&self, key: &str) -> Option<&Value> {
        match self {
            Value::Object(m) => m.get(key),
            _ => None,
        }
    }
}

struct Parser<'a> {
    s: &'a [u8],
    i: usize,
}

impl<'a> Parser<'a> {
    fn fail<T>(&self, m: &str) -> Result<T, String> {
        Err(m.to_string())
    }
    fn ws(&mut self) {
        while self.i < self.s.len() && matches!(self.s[self.i], b' ' | b'\t' | b'\r' | b'\n') {
            self.i += 1;
        }
    }
    fn peek(&self) -> u8 {
        if self.i < self.s.len() { self.s[self.i] } else { 0 }
    }
    fn eat(&mut self, c: u8) -> bool {
        if self.peek() == c {
            self.i += 1;
            true
        } else {
            false
        }
    }
    fn expect(&mut self, c: u8, m: &str) -> Result<(), String> {
        if self.eat(c) { Ok(()) } else { self.fail(m) }
    }

    fn value(&mut self) -> Result<Value, String> {
        self.ws();
        match self.peek() {
            b'"' => Ok(Value::Str(self.string()?)),
            b'[' => self.array(),
            b'{' => self.object(),
            b't' => {
                self.literal(b"true")?;
                Ok(Value::Bool(true))
            }
            b'f' => {
                self.literal(b"false")?;
                Ok(Value::Bool(false))
            }
            b'n' => {
                self.literal(b"null")?;
                Ok(Value::Null)
            }
            b'-' | b'0'..=b'9' => self.number(),
            _ => self.fail("JSON 格式不正确：无法解析的值"),
        }
    }

    fn literal(&mut self, w: &[u8]) -> Result<(), String> {
        for &c in w {
            if self.i >= self.s.len() || self.s[self.i] != c {
                return self.fail("JSON 格式不正确：字面量错误");
            }
            self.i += 1;
        }
        Ok(())
    }

    fn number(&mut self) -> Result<Value, String> {
        let st = self.i;
        if self.peek() == b'-' {
            self.i += 1;
        }
        while self.peek().is_ascii_digit() {
            self.i += 1;
        }
        if self.peek() == b'.' {
            self.i += 1;
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
        }
        if self.peek() == b'e' || self.peek() == b'E' {
            self.i += 1;
            if self.peek() == b'+' || self.peek() == b'-' {
                self.i += 1;
            }
            while self.peek().is_ascii_digit() {
                self.i += 1;
            }
        }
        if self.i == st || (self.i == st + 1 && self.s[st] == b'-') {
            return self.fail("JSON 格式不正确：数字格式错误");
        }
        let text = std::str::from_utf8(&self.s[st..self.i]).unwrap();
        match text.parse::<f64>() {
            Ok(n) => Ok(Value::Number(n)),
            Err(_) => self.fail("JSON 格式不正确：数字格式错误"),
        }
    }

    fn hex4(&mut self) -> Result<u32, String> {
        if self.i + 4 > self.s.len() {
            return self.fail("JSON 格式不正确：\\u 转义不完整");
        }
        let mut v: u32 = 0;
        for _ in 0..4 {
            let c = self.s[self.i];
            self.i += 1;
            v <<= 4;
            match c {
                b'0'..=b'9' => v |= (c - b'0') as u32,
                b'a'..=b'f' => v |= (c - b'a' + 10) as u32,
                b'A'..=b'F' => v |= (c - b'A' + 10) as u32,
                _ => return self.fail("JSON 格式不正确：\\u 转义非法"),
            }
        }
        Ok(v)
    }

    fn push_cp(out: &mut Vec<u8>, cp: u32) {
        // 未配对的代理项按 U+FFFD 输出（C++ 版按原样输出代理项，此处更安全）
        let ch = char::from_u32(cp).unwrap_or('\u{FFFD}');
        let mut buf = [0u8; 4];
        out.extend_from_slice(ch.encode_utf8(&mut buf).as_bytes());
    }

    fn string(&mut self) -> Result<String, String> {
        self.expect(b'"', "JSON 格式不正确：期望字符串")?;
        let mut out: Vec<u8> = Vec::new();
        loop {
            if self.i >= self.s.len() {
                return self.fail("JSON 格式不正确：字符串未闭合");
            }
            let c = self.s[self.i];
            self.i += 1;
            if c == b'"' {
                break;
            }
            if c != b'\\' {
                out.push(c);
                continue;
            }
            if self.i >= self.s.len() {
                return self.fail("JSON 格式不正确：转义不完整");
            }
            let e = self.s[self.i];
            self.i += 1;
            match e {
                b'"' => out.push(b'"'),
                b'\\' => out.push(b'\\'),
                b'/' => out.push(b'/'),
                b'b' => out.push(0x08),
                b'f' => out.push(0x0C),
                b'n' => out.push(b'\n'),
                b'r' => out.push(b'\r'),
                b't' => out.push(b'\t'),
                b'u' => {
                    let mut cp = self.hex4()?;
                    if (0xD800..=0xDBFF).contains(&cp) {
                        // 高代理，尝试配对低代理
                        if self.i + 1 < self.s.len()
                            && self.s[self.i] == b'\\'
                            && self.s[self.i + 1] == b'u'
                        {
                            let save = self.i;
                            self.i += 2;
                            let lo = self.hex4()?;
                            if (0xDC00..=0xDFFF).contains(&lo) {
                                cp = 0x10000 + ((cp - 0xD800) << 10) + (lo - 0xDC00);
                            } else {
                                self.i = save; // 未配对，按原样输出
                            }
                        }
                    }
                    Self::push_cp(&mut out, cp);
                }
                _ => return self.fail("JSON 格式不正确：非法转义"),
            }
        }
        String::from_utf8(out).map_err(|_| "JSON 格式不正确：字符串不是合法 UTF-8".to_string())
    }

    fn array(&mut self) -> Result<Value, String> {
        self.i += 1; // '['
        let mut arr = Vec::new();
        self.ws();
        if self.eat(b']') {
            return Ok(Value::Array(arr));
        }
        loop {
            arr.push(self.value()?);
            self.ws();
            if self.eat(b']') {
                break;
            }
            self.expect(b',', "JSON 格式不正确：数组元素之间缺少逗号")?;
        }
        Ok(Value::Array(arr))
    }

    fn object(&mut self) -> Result<Value, String> {
        self.i += 1; // '{'
        let mut obj = BTreeMap::new();
        self.ws();
        if self.eat(b'}') {
            return Ok(Value::Object(obj));
        }
        loop {
            self.ws();
            if self.peek() != b'"' {
                return self.fail("JSON 格式不正确：对象键必须是字符串");
            }
            let key = self.string()?;
            self.ws();
            self.expect(b':', "JSON 格式不正确：对象键后缺少冒号")?;
            obj.insert(key, self.value()?);
            self.ws();
            if self.eat(b'}') {
                break;
            }
            self.expect(b',', "JSON 格式不正确：对象成员之间缺少逗号")?;
        }
        Ok(Value::Object(obj))
    }
}

/// 解析 JSON 文本（自动跳过 UTF-8 BOM；from_str 已保证 UTF-8 合法）
pub fn parse(text: &str) -> Result<Value, String> {
    let bytes = text.as_bytes();
    let mut p = Parser { s: bytes, i: 0 };
    // 跳过 UTF-8 BOM
    if bytes.len() >= 3 && bytes[0] == 0xEF && bytes[1] == 0xBB && bytes[2] == 0xBF {
        p.i = 3;
    }
    let v = p.value()?;
    p.ws();
    if p.i < bytes.len() {
        return Err("JSON 格式不正确：末尾存在多余内容".to_string());
    }
    Ok(v)
}

/// 字符串 → JSON 转义（控制字符转 \u00XX）
pub fn escape(s: &str) -> String {
    let mut out = String::with_capacity(s.len() + 2);
    for ch in s.chars() {
        match ch {
            '"' => out.push_str("\\\""),
            '\\' => out.push_str("\\\\"),
            '\u{08}' => out.push_str("\\b"),
            '\u{0C}' => out.push_str("\\f"),
            '\n' => out.push_str("\\n"),
            '\r' => out.push_str("\\r"),
            '\t' => out.push_str("\\t"),
            c if (c as u32) < 0x20 => {
                out.push_str(&format!("\\u{:04x}", c as u32));
            }
            c => out.push(c),
        }
    }
    out
}
