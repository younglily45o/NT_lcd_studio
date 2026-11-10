use crate::Gamma;
use std::{collections::BTreeMap, fmt::Write, sync::OnceLock};

pub fn import_c(source: &str) -> Result<Gamma, String> {
    if source.len() > crate::project::MAX_FILE_SIZE {
        return Err("C 文件超过 4 MiB".into());
    }
    let clean = strip_comments(source)?;
    static DEFINITION: OnceLock<regex::Regex> = OnceLock::new();
    let re = DEFINITION.get_or_init(|| regex::Regex::new(r"\bgamma_t\s+gamma_config\s*=").unwrap());
    let definitions: Vec<_> = re.find_iter(&clean).collect();
    if definitions.len() != 1 {
        return Err(format!(
            "需要唯一的 gamma_t gamma_config 定义，发现 {} 个；不自动选择条件编译分支",
            definitions.len()
        ));
    }
    let tail = clean[definitions[0].end()..].trim_start();
    if !tail.starts_with('{') {
        return Err("gamma_config 需要使用大括号初始化".into());
    }
    let mut depth = 0;
    let mut end = None;
    for (i, c) in tail.char_indices() {
        match c {
            '{' => {
                depth += 1;
                if depth > 3 {
                    return Err("Gamma 初始化嵌套过深".into());
                }
            }
            '}' => {
                depth -= 1;
                if depth == 0 {
                    end = Some(i + 1);
                    break;
                }
            }
            _ => {}
        }
    }
    let end = end.ok_or("Gamma 初始化缺少闭合大括号")?;
    if !tail[end..].trim_start().starts_with(';') {
        return Err("Gamma 初始化后缺少分号".into());
    }
    let mut parser = Parser {
        tokens: tokenize(&tail[..end])?,
        index: 0,
    };
    let values = parser.list(0)?;
    if parser.index != parser.tokens.len() {
        return Err("Gamma 初始化存在多余内容".into());
    }
    let fields = if values.iter().all(|v| matches!(v, Value::Field(_, _))) {
        let mut fields = BTreeMap::new();
        for value in values {
            if let Value::Field(name, value) = value
                && fields.insert(name, *value).is_some()
            {
                return Err("Gamma 字段重复".into());
            }
        }
        if fields.len() != 4
            || ["enable", "gamma_r", "gamma_g", "gamma_b"]
                .iter()
                .any(|name| !fields.contains_key(*name))
        {
            return Err("需要且只支持 enable、gamma_r、gamma_g、gamma_b 四个字段".into());
        }
        [
            fields.remove("enable").unwrap(),
            fields.remove("gamma_r").unwrap(),
            fields.remove("gamma_g").unwrap(),
            fields.remove("gamma_b").unwrap(),
        ]
    } else {
        <[Value; 4]>::try_from(values).map_err(|_| "顺序初始化需要启用值和三组通道数组")?
    };
    let [enabled, r, g, b] = fields;
    let enabled = match enabled {
        Value::Scalar(0) => false,
        Value::Scalar(1) => true,
        _ => return Err("enable 仅支持 0/1 或 ENABLE/DISABLE/TRUE/FALSE".into()),
    };
    Ok(Gamma {
        enabled,
        red: to_array(r)?,
        green: to_array(g)?,
        blue: to_array(b)?,
    })
}

pub fn export_c(gamma: &Gamma) -> String {
    let mut output = String::from(
        "/* LCD Studio Gamma export.\n * Replace the existing gamma_config initializer in the SDK.\n * gamma_t must be supplied by the SDK's display.h.\n * Software preview uses input positions min(8*i,255). Verify on the target LCD.\n */\nstatic gamma_t gamma_config = {\n",
    );
    writeln!(output, "    .enable = {},", u8::from(gamma.enabled)).unwrap();
    for (name, values) in [
        ("gamma_r", &gamma.red),
        ("gamma_g", &gamma.green),
        ("gamma_b", &gamma.blue),
    ] {
        writeln!(output, "    .{name} = {{").unwrap();
        for chunk in values.chunks(8) {
            output.push_str("        ");
            for value in chunk {
                write!(output, "0x{value:02X}, ").unwrap();
            }
            output.push('\n');
        }
        output.push_str("    },\n");
    }
    output.push_str("};\n");
    output
}

fn to_array(value: Value) -> Result<[u8; 33], String> {
    let Value::List(values) = value else {
        return Err("Gamma 通道需要数组".into());
    };
    let bytes: Result<Vec<u8>, String> = values
        .into_iter()
        .map(|v| match v {
            Value::Scalar(v) => u8::try_from(v).map_err(|_| "Gamma 值必须为 0~255".into()),
            _ => Err("Gamma 数组仅支持整数字面量".into()),
        })
        .collect();
    bytes?
        .try_into()
        .map_err(|v: Vec<u8>| format!("Gamma 通道需要 33 个点，收到 {} 个", v.len()))
}

fn strip_comments(source: &str) -> Result<String, String> {
    let bytes = source.as_bytes();
    let mut out = bytes.to_vec();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'/') {
            while i < bytes.len() && bytes[i] != b'\n' {
                out[i] = b' ';
                i += 1;
            }
        } else if bytes[i] == b'/' && bytes.get(i + 1) == Some(&b'*') {
            out[i] = b' ';
            out[i + 1] = b' ';
            i += 2;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == b'*' && bytes.get(i + 1) == Some(&b'/') {
                    out[i] = b' ';
                    out[i + 1] = b' ';
                    i += 2;
                    closed = true;
                    break;
                }
                if bytes[i] != b'\n' {
                    out[i] = b' ';
                }
                i += 1;
            }
            if !closed {
                return Err("C 文件存在未结束的块注释".into());
            }
        } else if bytes[i] == b'"' || bytes[i] == b'\'' {
            let quote = bytes[i];
            i += 1;
            let mut closed = false;
            while i < bytes.len() {
                if bytes[i] == quote {
                    i += 1;
                    closed = true;
                    break;
                }
                if bytes[i] == b'\\' {
                    out[i] = b' ';
                    i += 1;
                    if i < bytes.len() {
                        out[i] = b' ';
                        i += 1;
                    }
                } else {
                    if bytes[i] != b'\n' {
                        out[i] = b' ';
                    }
                    i += 1;
                }
            }
            if !closed {
                return Err("C 文件存在未结束的字符串或字符".into());
            }
        } else {
            i += 1;
        }
    }
    String::from_utf8(out).map_err(|_| "C 文件编码无效".into())
}

#[derive(Clone, Debug, PartialEq)]
enum Token {
    Open,
    Close,
    Comma,
    Dot,
    Equal,
    Number(u32),
    Name(String),
}
fn tokenize(source: &str) -> Result<Vec<Token>, String> {
    let mut result = Vec::new();
    let bytes = source.as_bytes();
    let mut i = 0;
    while i < bytes.len() {
        let c = bytes[i];
        if c.is_ascii_whitespace() {
            i += 1;
            continue;
        }
        let token = match c {
            b'{' => Token::Open,
            b'}' => Token::Close,
            b',' => Token::Comma,
            b'.' => Token::Dot,
            b'=' => Token::Equal,
            b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let word = &source[start..i];
                let value = if word.starts_with("0x") || word.starts_with("0X") {
                    u32::from_str_radix(&word[2..], 16)
                } else {
                    if word.len() > 1 && word.starts_with('0') {
                        return Err(format!("不支持前导零整数 {word}；请使用十进制或 0x 十六进制，避免与 C 八进制混淆"));
                    }
                    word.parse::<u32>()
                }
                .map_err(|_| format!("不支持的整数：{word}"))?;
                result.push(Token::Number(value));
                continue;
            }
            b'a'..=b'z' | b'A'..=b'Z' | b'_' => {
                let start = i;
                i += 1;
                while i < bytes.len() && (bytes[i].is_ascii_alphanumeric() || bytes[i] == b'_') {
                    i += 1;
                }
                result.push(Token::Name(source[start..i].into()));
                continue;
            }
            _ => {
                return Err(format!(
                    "Gamma 初始化不支持字符 {}、条件编译或宏运算",
                    char::from(c)
                ));
            }
        };
        result.push(token);
        i += 1;
    }
    Ok(result)
}
#[derive(Debug)]
enum Value {
    Scalar(u32),
    List(Vec<Value>),
    Field(String, Box<Value>),
}
struct Parser {
    tokens: Vec<Token>,
    index: usize,
}
impl Parser {
    fn eat(&mut self, token: Token) -> bool {
        if self.tokens.get(self.index) == Some(&token) {
            self.index += 1;
            true
        } else {
            false
        }
    }
    fn list(&mut self, depth: usize) -> Result<Vec<Value>, String> {
        if depth > 2 || !self.eat(Token::Open) {
            return Err("数组初始化格式无效".into());
        }
        let mut result = Vec::new();
        if self.eat(Token::Close) {
            return Ok(result);
        }
        loop {
            result.push(if self.eat(Token::Dot) {
                let Some(Token::Name(name)) = self.tokens.get(self.index).cloned() else {
                    return Err("指定字段名称无效".into());
                };
                self.index += 1;
                if !self.eat(Token::Equal) {
                    return Err("指定字段后需要等号".into());
                }
                Value::Field(name, Box::new(self.value(depth + 1)?))
            } else {
                self.value(depth + 1)?
            });
            if self.eat(Token::Close) {
                break;
            }
            if !self.eat(Token::Comma) {
                return Err("Gamma 数值之间需要逗号".into());
            }
            if self.eat(Token::Close) {
                break;
            }
        }
        Ok(result)
    }
    fn value(&mut self, depth: usize) -> Result<Value, String> {
        match self.tokens.get(self.index).cloned() {
            Some(Token::Open) => Ok(Value::List(self.list(depth)?)),
            Some(Token::Number(n)) => {
                self.index += 1;
                Ok(Value::Scalar(n))
            }
            Some(Token::Name(name)) => {
                self.index += 1;
                match name.as_str() {
                    "ENABLE" | "TRUE" => Ok(Value::Scalar(1)),
                    "DISABLE" | "FALSE" => Ok(Value::Scalar(0)),
                    _ => Err(format!("不支持的宏：{name}")),
                }
            }
            _ => Err("Gamma 数值或数组格式无效".into()),
        }
    }
}
