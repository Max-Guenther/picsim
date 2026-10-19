//! Ausdrücke in Operanden: Zahlen in allen iL_ASS16-Schreibweisen,
//! Symbole, `$` und die üblichen Operatoren.

use std::collections::HashMap;

#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Num(i64),
    /// Textersatz, z.B. `GIE EQU 0BH,7`.
    Text(String),
}

pub type Symbols = HashMap<String, Value>;

#[derive(Debug, Clone, PartialEq)]
pub enum ExprError {
    Unknown(String),
    Syntax(String),
    DivZero,
}

/// Liest eine Zahl wie `35`, `0FFH`, `01B`, `0x1F`, `$1F`, `'A'`.
pub fn parse_number(tok: &str) -> Option<i64> {
    let t = tok.trim();
    if t.len() == 3 && t.starts_with('\'') && t.ends_with('\'') {
        return Some(t.as_bytes()[1] as i64);
    }
    if let Some(h) = t.strip_prefix('$') {
        return i64::from_str_radix(h, 16).ok();
    }
    if !t.chars().next()?.is_ascii_digit() {
        return None;
    }
    let lower = t.to_ascii_lowercase();
    if let Some(h) = lower.strip_prefix("0x") {
        return i64::from_str_radix(h, 16).ok();
    }
    if let Some(h) = lower.strip_suffix('h') {
        return i64::from_str_radix(h, 16).ok();
    }
    if let Some(b) = lower.strip_suffix('b') {
        if let Ok(v) = i64::from_str_radix(b, 2) {
            return Some(v);
        }
    }
    if let Some(d) = lower.strip_suffix('d') {
        if let Ok(v) = d.parse() {
            return Some(v);
        }
    }
    if let Some(o) = lower.strip_suffix('o').or_else(|| lower.strip_suffix('q')) {
        return i64::from_str_radix(o, 8).ok();
    }
    if let Some(b) = lower.strip_prefix("0b") {
        return i64::from_str_radix(b, 2).ok();
    }
    lower.parse().ok()
}

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Num(i64),
    Ident(String),
    Here,
    Op(&'static str),
    LParen,
    RParen,
}

fn is_ident_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_' || c == '$' || c == '.' || c == '?'
}

fn tokenize(s: &str) -> Result<Vec<Tok>, ExprError> {
    const OPS: [&str; 18] = [
        "<<", ">>", "<=", ">=", "<>", "!=", "==", "&&", "||", "+", "-", "*", "/", "%", "&", "|", "^", "~",
    ];
    let mut out = Vec::new();
    let chars: Vec<char> = s.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        if c == '(' {
            out.push(Tok::LParen);
            i += 1;
            continue;
        }
        if c == ')' {
            out.push(Tok::RParen);
            i += 1;
            continue;
        }
        if c == '\'' && i + 2 < chars.len() && chars[i + 2] == '\'' {
            out.push(Tok::Num(chars[i + 1] as i64));
            i += 3;
            continue;
        }
        if is_ident_char(c) {
            let start = i;
            while i < chars.len() && is_ident_char(chars[i]) {
                i += 1;
            }
            let word: String = chars[start..i].iter().collect();
            if word == "$" {
                out.push(Tok::Here);
            } else if let Some(n) = parse_number(&word) {
                out.push(Tok::Num(n));
            } else if word.starts_with(|c: char| c.is_ascii_digit()) {
                return Err(ExprError::Syntax(word));
            } else {
                out.push(Tok::Ident(word.to_ascii_uppercase()));
            }
            continue;
        }
        let rest: String = chars[i..].iter().take(2).collect();
        if let Some(op) = OPS.iter().find(|op| rest.starts_with(**op)) {
            out.push(Tok::Op(op));
            i += op.len();
            continue;
        }
        if c == '=' || c == '<' || c == '>' || c == '!' {
            out.push(Tok::Op(match c {
                '=' => "==",
                '<' => "<",
                '>' => ">",
                _ => "!",
            }));
            i += 1;
            continue;
        }
        return Err(ExprError::Syntax(c.to_string()));
    }
    Ok(out)
}

struct Eval<'a> {
    toks: Vec<Tok>,
    pos: usize,
    syms: &'a Symbols,
    here: i64,
}

/// Wertet einen Ausdruck aus. `here` ist die aktuelle Adresse für `$`.
pub fn eval(s: &str, syms: &Symbols, here: i64) -> Result<i64, ExprError> {
    let toks = tokenize(s)?;
    if toks.is_empty() {
        return Err(ExprError::Syntax(String::new()));
    }
    let mut e = Eval { toks, pos: 0, syms, here };
    let v = e.binary(0)?;
    if e.pos != e.toks.len() {
        return Err(ExprError::Syntax(s.to_string()));
    }
    Ok(v)
}

fn precedence(op: &str) -> Option<u8> {
    Some(match op {
        "||" => 1,
        "&&" => 2,
        "|" => 3,
        "^" => 4,
        "&" => 5,
        "==" | "!=" | "<>" => 6,
        "<" | ">" | "<=" | ">=" => 7,
        "<<" | ">>" => 8,
        "+" | "-" => 9,
        "*" | "/" | "%" => 10,
        _ => return None,
    })
}

impl Eval<'_> {
    fn binary(&mut self, min: u8) -> Result<i64, ExprError> {
        let mut lhs = self.unary()?;
        while let Some(Tok::Op(op)) = self.toks.get(self.pos).cloned() {
            let Some(p) = precedence(op) else { break };
            if p < min {
                break;
            }
            self.pos += 1;
            let rhs = self.binary(p + 1)?;
            lhs = match op {
                "+" => lhs.wrapping_add(rhs),
                "-" => lhs.wrapping_sub(rhs),
                "*" => lhs.wrapping_mul(rhs),
                "/" if rhs == 0 => return Err(ExprError::DivZero),
                "/" => lhs / rhs,
                "%" if rhs == 0 => return Err(ExprError::DivZero),
                "%" => lhs % rhs,
                "&" => lhs & rhs,
                "|" => lhs | rhs,
                "^" => lhs ^ rhs,
                "<<" => lhs << (rhs & 63),
                ">>" => lhs >> (rhs & 63),
                "==" => (lhs == rhs) as i64,
                "!=" | "<>" => (lhs != rhs) as i64,
                "<" => (lhs < rhs) as i64,
                ">" => (lhs > rhs) as i64,
                "<=" => (lhs <= rhs) as i64,
                ">=" => (lhs >= rhs) as i64,
                "&&" => (lhs != 0 && rhs != 0) as i64,
                "||" => (lhs != 0 || rhs != 0) as i64,
                _ => unreachable!(),
            };
        }
        Ok(lhs)
    }

    fn unary(&mut self) -> Result<i64, ExprError> {
        let tok = self.toks.get(self.pos).cloned();
        self.pos += 1;
        match tok {
            Some(Tok::Num(n)) => Ok(n),
            Some(Tok::Here) => Ok(self.here),
            Some(Tok::Op("-")) => Ok(-self.unary()?),
            Some(Tok::Op("+")) => self.unary(),
            Some(Tok::Op("~")) => Ok(!self.unary()?),
            Some(Tok::Op("!")) => Ok((self.unary()? == 0) as i64),
            Some(Tok::LParen) => {
                let v = self.binary(0)?;
                match self.toks.get(self.pos) {
                    Some(Tok::RParen) => {
                        self.pos += 1;
                        Ok(v)
                    }
                    _ => Err(ExprError::Syntax("(".into())),
                }
            }
            Some(Tok::Ident(name)) => {
                if matches!(name.as_str(), "HIGH" | "LOW")
                    && matches!(self.toks.get(self.pos), Some(Tok::LParen))
                {
                    let v = self.unary()?;
                    return Ok(if name == "HIGH" { (v >> 8) & 0xFF } else { v & 0xFF });
                }
                match self.syms.get(&name) {
                    Some(Value::Num(n)) => Ok(*n),
                    // Alias auf ein anderes Symbol, ohne Komma
                    Some(Value::Text(t)) if !t.contains(',') => eval(t, self.syms, self.here),
                    Some(Value::Text(_)) => Err(ExprError::Syntax(name)),
                    None => Err(ExprError::Unknown(name)),
                }
            }
            _ => Err(ExprError::Syntax(String::new())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn number_formats() {
        assert_eq!(parse_number("35"), Some(35));
        assert_eq!(parse_number("0FFH"), Some(255));
        assert_eq!(parse_number("0Ah"), Some(10));
        assert_eq!(parse_number("01B"), Some(1));
        assert_eq!(parse_number("10101010b"), Some(0xAA));
        assert_eq!(parse_number("0x1f"), Some(31));
        assert_eq!(parse_number("0b101"), Some(5));
        assert_eq!(parse_number("$20"), Some(32));
        assert_eq!(parse_number("'A'"), Some(65));
        assert_eq!(parse_number("0BH"), Some(11));
        assert_eq!(parse_number("abc"), None);
    }

    #[test]
    fn expressions() {
        let mut s = Symbols::new();
        s.insert("ZEIT".into(), Value::Num(35));
        assert_eq!(eval("zeit+1", &s, 0), Ok(36));
        assert_eq!(eval("(1<<3)|1", &s, 0), Ok(9));
        assert_eq!(eval("$-1", &s, 10), Ok(9));
        assert_eq!(eval("high(1234h)", &s, 0), Ok(0x12));
        assert_eq!(eval("2*3+4", &s, 0), Ok(10));
        assert_eq!(eval("zeit = 35", &s, 0), Ok(1));
        assert_eq!(eval("nix", &s, 0), Err(ExprError::Unknown("NIX".into())));
    }
}
