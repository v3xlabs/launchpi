use std::fmt::{self, Display, Formatter, Write};

const KEYWORDS: [&str; 10] = [
    "assert", "else", "if", "in", "inherit", "let", "or", "rec", "then", "with",
];

pub struct NixExpression<'a>(pub &'a toml::Value);

impl Display for NixExpression<'_> {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        write_value(out, self.0, 0)
    }
}

pub struct NixString<'a>(pub &'a str);

impl Display for NixString<'_> {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        out.write_char('"')?;
        let mut characters = self.0.chars().peekable();
        while let Some(character) = characters.next() {
            match character {
                '\\' => out.write_str("\\\\")?,
                '"' => out.write_str("\\\"")?,
                '\n' => out.write_str("\\n")?,
                '\r' => out.write_str("\\r")?,
                '\t' => out.write_str("\\t")?,
                '$' if characters.peek() == Some(&'{') => out.write_str("\\$")?,
                other => out.write_char(other)?,
            }
        }
        out.write_char('"')
    }
}

pub struct NixAttribute<'a>(pub &'a str);

impl Display for NixAttribute<'_> {
    fn fmt(&self, out: &mut Formatter<'_>) -> fmt::Result {
        let mut characters = self.0.chars();
        let is_identifier = characters
            .next()
            .is_some_and(|first| first.is_ascii_alphabetic() || first == '_')
            && characters.all(|character| {
                character.is_ascii_alphanumeric() || matches!(character, '_' | '\'' | '-')
            })
            && !KEYWORDS.contains(&self.0);
        if is_identifier {
            out.write_str(self.0)
        } else {
            NixString(self.0).fmt(out)
        }
    }
}

fn write_value(out: &mut Formatter<'_>, value: &toml::Value, depth: usize) -> fmt::Result {
    match value {
        toml::Value::String(text) => NixString(text).fmt(out),
        toml::Value::Integer(number) if *number < 0 => write!(out, "({number})"),
        toml::Value::Integer(number) => write!(out, "{number}"),
        toml::Value::Float(number) => write_float(out, *number),
        toml::Value::Boolean(flag) => write!(out, "{flag}"),
        toml::Value::Datetime(datetime) => NixString(&datetime.to_string()).fmt(out),
        toml::Value::Array(items) if items.is_empty() => out.write_str("[ ]"),
        toml::Value::Array(items) => {
            out.write_str("[\n")?;
            for item in items {
                indent(out, depth + 1)?;
                write_value(out, item, depth + 1)?;
                out.write_char('\n')?;
            }
            indent(out, depth)?;
            out.write_char(']')
        }
        toml::Value::Table(table) if table.is_empty() => out.write_str("{ }"),
        toml::Value::Table(table) => {
            out.write_str("{\n")?;
            for (key, item) in table {
                indent(out, depth + 1)?;
                write!(out, "{} = ", NixAttribute(key))?;
                write_value(out, item, depth + 1)?;
                out.write_str(";\n")?;
            }
            indent(out, depth)?;
            out.write_char('}')
        }
    }
}

/// Nix has no literal for infinity or NaN, and a float literal needs a digit on both sides of the
/// point, which Rust's shortest form leaves out for exponents such as `1e20`.
fn write_float(out: &mut Formatter<'_>, number: f64) -> fmt::Result {
    if !number.is_finite() {
        let literal = if number.is_nan() {
            "nan"
        } else if number > 0.0 {
            "inf"
        } else {
            "-inf"
        };
        return write!(out, "(builtins.fromTOML \"value = {literal}\").value");
    }
    let magnitude = format!("{:?}", number.abs());
    let magnitude = if magnitude.contains('.') {
        magnitude
    } else {
        magnitude.replacen('e', ".0e", 1)
    };
    if number.is_sign_negative() {
        write!(out, "(-{magnitude})")
    } else {
        out.write_str(&magnitude)
    }
}

fn indent(out: &mut Formatter<'_>, depth: usize) -> fmt::Result {
    for _ in 0..depth {
        out.write_str("  ")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_string_escapes_everything_nix_would_otherwise_interpret() {
        assert_eq!(
            NixString("say \"hi\" \\ ${HOME} $5\n").to_string(),
            r#""say \"hi\" \\ \${HOME} $5\n""#
        );
    }
}
