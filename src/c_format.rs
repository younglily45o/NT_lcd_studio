use crate::{ColorControl, Gamma};
use std::{collections::BTreeMap, fmt::Write};

pub fn import_c(source: &str) -> Result<Gamma, String> {
    gamma_from_values(
        parse_definition(source, "gamma_t", "gamma_config")?
            .ok_or("需要唯一的 gamma_t gamma_config 定义")?,
    )
}

pub fn import_config(source: &str) -> Result<(Gamma, ColorControl), String> {
    let gamma = parse_definition(source, "gamma_t", "gamma_config")?
        .map(gamma_from_values)
        .transpose()?;
    let color = parse_definition(source, "cc_t", "color_control_config")?
        .map(color_from_values)
        .transpose()?;
    if gamma.is_none() && color.is_none() {
        return Err("需要 gamma_config 或 color_control_config 的明确初始化定义".into());
    }
    Ok((
        gamma.unwrap_or_else(Gamma::identity),
        color.unwrap_or_default(),
    ))
}

fn parse_definition(source: &str, ty: &str, name: &str) -> Result<Option<Vec<Value>>, String> {
    if source.len() > crate::project::MAX_FILE_SIZE {
        return Err("C 文件超过 4 MiB".into());
    }
    let clean = strip_comments(source)?;
    let re = regex::Regex::new(&format!(r"\b{ty}\s+{name}\s*=")).unwrap();
    let definitions: Vec<_> = re.find_iter(&clean).collect();
    if definitions.is_empty() {
        return Ok(None);
    }
    if definitions.len() != 1 {
        return Err(format!(
            "需要唯一的 {ty} {name} 定义，发现 {} 个；不自动选择条件编译分支",
            definitions.len()
        ));
    }
    let tail = clean[definitions[0].end()..].trim_start();
    if !tail.starts_with('{') {
        return Err(format!("{name} 需要大括号初始化"));
    }
    let mut depth = 0;
    let mut end = None;
    for (i, c) in tail.char_indices() {
        match c {
            '{' => {
                depth += 1;
                if depth > 3 {
                    return Err("配置初始化嵌套过深".into());
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
    let end = end.ok_or("配置初始化缺少闭合大括号")?;
    if !tail[end..].trim_start().starts_with(';') {
        return Err("配置初始化后缺少分号".into());
    }
    let mut parser = Parser {
        tokens: tokenize(&tail[..end])?,
        index: 0,
    };
    let values = parser.list(0)?;
    if parser.index != parser.tokens.len() {
        return Err("配置初始化存在多余内容".into());
    }
    Ok(Some(values))
}

fn ordered_fields(values: Vec<Value>, names: &[&str]) -> Result<Vec<Value>, String> {
    if values.iter().all(|v| matches!(v, Value::Field(_, _))) {
        let mut fields = BTreeMap::new();
        for value in values {
            if let Value::Field(name, value) = value
                && fields.insert(name, *value).is_some()
            {
                return Err("配置字段重复".into());
            }
        }
        if fields.len() != names.len() || names.iter().any(|name| !fields.contains_key(*name)) {
            return Err(format!("需要且只支持字段 {}", names.join("、")));
        }
        Ok(names
            .iter()
            .map(|name| fields.remove(*name).unwrap())
            .collect())
    } else if values.len() == names.len() && !values.iter().any(|v| matches!(v, Value::Field(_, _)))
    {
        Ok(values)
    } else {
        Err("初始化字段数量错误或混合了顺序与指定字段".into())
    }
}

fn enabled(value: Value) -> Result<bool, String> {
    match value {
        Value::Scalar(0) => Ok(false),
        Value::Scalar(1) => Ok(true),
        _ => Err("enable 仅支持 0/1 或 ENABLE/DISABLE/TRUE/FALSE".into()),
    }
}
fn gamma_from_values(values: Vec<Value>) -> Result<Gamma, String> {
    let fields = ordered_fields(values, &["enable", "gamma_r", "gamma_g", "gamma_b"])?;
    let [enable, r, g, b]: [Value; 4] = fields.try_into().map_err(|_| "Gamma 字段数量错误")?;
    Ok(Gamma {
        enabled: enabled(enable)?,
        red: to_array(r)?,
        green: to_array(g)?,
        blue: to_array(b)?,
    })
}
fn color_from_values(values: Vec<Value>) -> Result<ColorControl, String> {
    let fields = ordered_fields(values, &["enable", "sat_ofs", "int_ofs", "ycon", "ccon"])?;
    let [enable, sat, int, ycon, ccon]: [Value; 5] =
        fields.try_into().map_err(|_| "颜色字段数量错误")?;
    fn number(value: Value, low: i64, high: i64) -> Result<i64, String> {
        match value {
            Value::Scalar(n) if (low..=high).contains(&n) => Ok(n),
            _ => Err(format!("颜色参数需要 {low}~{high} 整数；ofs 为 INT8")),
        }
    }
    Ok(ColorControl {
        enabled: enabled(enable)?,
        sat_ofs: number(sat, -128, 127)? as i8,
        int_ofs: number(int, -128, 127)? as i8,
        ycon: number(ycon, 0, 255)? as u8,
        ccon: number(ccon, 0, 255)? as u8,
    })
}

pub fn export_config(gamma: &Gamma, color: &ColorControl) -> String {
    let mut output = export_c(gamma);
    writeln!(output,"\n/* SDK display.h cc_t. Offsets are signed INT8. Preview is approximate. */\nstatic cc_t color_control_config = {{\n    .enable = {},\n    .sat_ofs = {},\n    .int_ofs = {},\n    .ycon = {},\n    .ccon = {}\n}};",u8::from(color.enabled),color.sat_ofs,color.int_ofs,color.ycon,color.ccon).unwrap();
    output
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
    Number(i64),
    Minus,
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
            b'-' => Token::Minus,
            b'0'..=b'9' => {
                let start = i;
                i += 1;
                while i < bytes.len() && bytes[i].is_ascii_alphanumeric() {
                    i += 1;
                }
                let word = &source[start..i];
                let value = if word.starts_with("0x") || word.starts_with("0X") {
                    i64::from_str_radix(&word[2..], 16)
                } else {
                    if word.len() > 1 && word.starts_with('0') {
                        return Err(format!("不支持前导零整数 {word}；请使用十进制或 0x 十六进制，避免与 C 八进制混淆"));
                    }
                    word.parse::<i64>()
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
    Scalar(i64),
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
            Some(Token::Minus) => {
                self.index += 1;
                let Some(Token::Number(n)) = self.tokens.get(self.index).cloned() else {
                    return Err("负号后需要整数字面量".into());
                };
                self.index += 1;
                Ok(Value::Scalar(-n))
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
