//! Numeric literal parsing.

use crate::error::{CtxResult, ErrorCode};
use crate::value::Value;

use crate::parse::reader::Parser;

impl<'a> Parser<'a> {
    pub(super) fn parse_number(&mut self) -> CtxResult<Value<'a>> {
        let start = self.index;
        let s = self.remaining();

        let signed = s.starts_with('+') || s.starts_with('-');
        let sign_len = usize::from(signed);
        let rest = &s[sign_len..];

        if rest.starts_with("0x") || rest.starts_with("0X") {
            self.index += sign_len + 2;
            let num_start = self.index;
            self.consume_digits_underscores(|c| c.is_ascii_hexdigit())?;
            if self.index == num_start {
                return Err(self.err_at(ErrorCode::InvalidNumber, start));
            }
            self.reject_number_trailer(start)?;
            let digits: String = self.input[num_start..self.index]
                .chars()
                .filter(|c| *c != '_')
                .collect();
            let mut val = i128::from_str_radix(&digits, 16)
                .map_err(|_| self.err_at(ErrorCode::InvalidNumber, start))?;
            if s.starts_with('-') {
                val = -val;
            }
            return Ok(Value::Int(val));
        }
        if rest.starts_with("0o") || rest.starts_with("0O") {
            self.index += sign_len + 2;
            let num_start = self.index;
            self.consume_digits_underscores(|c| matches!(c, '0'..='7'))?;
            if self.index == num_start {
                return Err(self.err_at(ErrorCode::InvalidNumber, start));
            }
            self.reject_number_trailer(start)?;
            let digits: String = self.input[num_start..self.index]
                .chars()
                .filter(|c| *c != '_')
                .collect();
            let mut val = i128::from_str_radix(&digits, 8)
                .map_err(|_| self.err_at(ErrorCode::InvalidNumber, start))?;
            if s.starts_with('-') {
                val = -val;
            }
            return Ok(Value::Int(val));
        }
        if rest.starts_with("0b") || rest.starts_with("0B") {
            self.index += sign_len + 2;
            let num_start = self.index;
            self.consume_digits_underscores(|c| matches!(c, '0' | '1'))?;
            if self.index == num_start {
                return Err(self.err_at(ErrorCode::InvalidNumber, start));
            }
            self.reject_number_trailer(start)?;
            let digits: String = self.input[num_start..self.index]
                .chars()
                .filter(|c| *c != '_')
                .collect();
            let mut val = i128::from_str_radix(&digits, 2)
                .map_err(|_| self.err_at(ErrorCode::InvalidNumber, start))?;
            if s.starts_with('-') {
                val = -val;
            }
            return Ok(Value::Int(val));
        }

        // Decimal: sign? integer ('.' integer)? exponent?
        // Integer part is required (rejects `.0`, `.1`).
        if signed {
            self.bump_byte();
        }
        let int_start = self.index;
        self.consume_digits_underscores(|c| c.is_ascii_digit())?;
        if self.index == int_start {
            return Err(self
                .err_at(ErrorCode::InvalidNumber, start)
                .with_message("number requires an integer digit before optional fraction"));
        }
        // Leading underscore after sign is invalid (consume stops immediately).
        let mut is_float = false;
        if self.peek_byte() == Some(b'.') {
            let after = self.bytes.get(self.index + 1).copied();
            if after == Some(b'_') {
                return Err(self
                    .err_at(ErrorCode::InvalidNumber, self.index)
                    .with_message("underscore cannot start a fraction"));
            }
            if after.is_some_and(|b| b.is_ascii_digit()) {
                is_float = true;
                self.bump_byte();
                let frac_start = self.index;
                self.consume_digits_underscores(|c| c.is_ascii_digit())?;
                if self.index == frac_start {
                    return Err(self.err_at(ErrorCode::InvalidNumber, start));
                }
            } else if after == Some(b'e') || after == Some(b'E') || after.is_none() {
                // `1.` or `1.e10` — fraction required if dot present
                return Err(self
                    .err_at(ErrorCode::InvalidNumber, self.index)
                    .with_message("expected fraction digits after `.`"));
            } else {
                // `1.0.0` — second dot handled by trailer check after first float parse…
                // Actually first loop: 1 then .0 then peek is . — not digit so we don't
                // consume second dot here. Trailer rejects.
            }
        }
        if matches!(self.peek_byte(), Some(b'e') | Some(b'E')) {
            is_float = true;
            self.bump_byte();
            if matches!(self.peek_byte(), Some(b'+') | Some(b'-')) {
                self.bump_byte();
            }
            if self.peek_byte() == Some(b'_') {
                return Err(self
                    .err_at(ErrorCode::InvalidNumber, self.index)
                    .with_message("underscore cannot start an exponent"));
            }
            let exp_start = self.index;
            self.consume_digits_underscores(|c| c.is_ascii_digit())?;
            if self.index == exp_start {
                return Err(self.err_at(ErrorCode::InvalidNumber, start));
            }
        }

        self.reject_number_trailer(start)?;

        let lex = &self.input[start..self.index];
        let raw: String = lex.chars().filter(|c| *c != '_').collect();
        if is_float {
            let v: f64 = raw.parse().unwrap_or(f64::NAN);
            // Always keep lexical form for floats so pretty-print can match the
            // official suite (including extreme exponents beyond f64 range).
            Ok(Value::float_raw(v, crate::value::KdlStr::borrowed(lex)))
        } else {
            let v: i128 = raw
                .parse()
                .map_err(|_| self.err_at(ErrorCode::InvalidNumber, start))?;
            Ok(Value::Int(v))
        }
    }

    /// After a number token, the next character must not glue into an identifier/number.
    fn reject_number_trailer(&self, start: usize) -> CtxResult<()> {
        match self.peek_char() {
            None => Ok(()),
            // Alphanumeric / `_` / extra `.` continue the same token illegally.
            // `+`/`-` start a new signed value only after whitespace (enforced elsewhere).
            Some(c) if c.is_ascii_alphanumeric() || c == '_' || c == '.' => Err(self
                .err_at(ErrorCode::InvalidNumber, start)
                .with_message("invalid character in number")),
            Some(_) => Ok(()),
        }
    }

    pub(super) fn consume_digits_underscores(
        &mut self,
        mut is_digit: impl FnMut(char) -> bool,
    ) -> CtxResult<()> {
        // KDL allows `_` between digits and also a trailing `_` as part of the
        // number token (filtered out when converting). Double `__` is not valid.
        let mut last_underscore = false;
        let mut saw_digit = false;
        while let Some(c) = self.peek_char() {
            if c == '_' {
                if !saw_digit || last_underscore {
                    break;
                }
                last_underscore = true;
                self.bump_char();
                continue;
            }
            if is_digit(c) {
                saw_digit = true;
                last_underscore = false;
                self.bump_char();
                continue;
            }
            break;
        }
        Ok(())
    }
}
