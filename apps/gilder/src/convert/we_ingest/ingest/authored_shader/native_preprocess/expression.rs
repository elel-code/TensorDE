//! Integer expression evaluation for the strict package shader preprocessor.

use std::collections::{BTreeMap, BTreeSet};

use super::{MacroDefinition, identifier_end, is_identifier_start, next_character};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Token<'a> {
    Number(i64),
    Identifier(&'a str),
    LeftParenthesis,
    RightParenthesis,
    Operator(Operator),
    End,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Operator {
    LogicalOr,
    LogicalAnd,
    BitOr,
    BitXor,
    BitAnd,
    Equal,
    NotEqual,
    Less,
    LessEqual,
    Greater,
    GreaterEqual,
    ShiftLeft,
    ShiftRight,
    Add,
    Subtract,
    Multiply,
    Divide,
    Remainder,
    LogicalNot,
    BitNot,
}

pub(super) fn evaluate_expression(
    expression: &str,
    macros: &BTreeMap<String, MacroDefinition>,
) -> Result<i64, String> {
    let mut resolving = BTreeSet::new();
    evaluate_expression_with_resolving(expression, macros, &mut resolving)
}

struct Parser<'a, 'b> {
    tokens: Vec<Token<'a>>,
    offset: usize,
    macros: &'a BTreeMap<String, MacroDefinition>,
    resolving: &'b mut BTreeSet<String>,
}

impl<'a, 'b> Parser<'a, 'b> {
    fn parse_logical_or(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_logical_and,
            &[Operator::LogicalOr],
            |_, left, right| Ok(i64::from(left != 0 || right != 0)),
        )
    }

    fn parse_logical_and(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_bit_or,
            &[Operator::LogicalAnd],
            |_, left, right| Ok(i64::from(left != 0 && right != 0)),
        )
    }

    fn parse_bit_or(&mut self) -> Result<i64, String> {
        self.parse_binary(Self::parse_bit_xor, &[Operator::BitOr], |_, left, right| {
            Ok(left | right)
        })
    }

    fn parse_bit_xor(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_bit_and,
            &[Operator::BitXor],
            |_, left, right| Ok(left ^ right),
        )
    }

    fn parse_bit_and(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_equality,
            &[Operator::BitAnd],
            |_, left, right| Ok(left & right),
        )
    }

    fn parse_equality(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_relational,
            &[Operator::Equal, Operator::NotEqual],
            |operator, left, right| {
                Ok(i64::from(match operator {
                    Operator::Equal => left == right,
                    Operator::NotEqual => left != right,
                    _ => unreachable!("equality parser received another operator"),
                }))
            },
        )
    }

    fn parse_relational(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_shift,
            &[
                Operator::Less,
                Operator::LessEqual,
                Operator::Greater,
                Operator::GreaterEqual,
            ],
            |operator, left, right| {
                Ok(i64::from(match operator {
                    Operator::Less => left < right,
                    Operator::LessEqual => left <= right,
                    Operator::Greater => left > right,
                    Operator::GreaterEqual => left >= right,
                    _ => unreachable!("relational parser received another operator"),
                }))
            },
        )
    }

    fn parse_shift(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_additive,
            &[Operator::ShiftLeft, Operator::ShiftRight],
            |operator, left, right| {
                let shift = u32::try_from(right).map_err(|_| "negative shift count".to_owned())?;
                match operator {
                    Operator::ShiftLeft => left
                        .checked_shl(shift)
                        .ok_or_else(|| "preprocessor left shift overflows i64".to_owned()),
                    Operator::ShiftRight => left
                        .checked_shr(shift)
                        .ok_or_else(|| "preprocessor right shift overflows i64".to_owned()),
                    _ => unreachable!("shift parser received another operator"),
                }
            },
        )
    }

    fn parse_additive(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_multiplicative,
            &[Operator::Add, Operator::Subtract],
            |operator, left, right| match operator {
                Operator::Add => left
                    .checked_add(right)
                    .ok_or_else(|| "preprocessor addition overflows i64".to_owned()),
                Operator::Subtract => left
                    .checked_sub(right)
                    .ok_or_else(|| "preprocessor subtraction overflows i64".to_owned()),
                _ => unreachable!("additive parser received another operator"),
            },
        )
    }

    fn parse_multiplicative(&mut self) -> Result<i64, String> {
        self.parse_binary(
            Self::parse_unary,
            &[Operator::Multiply, Operator::Divide, Operator::Remainder],
            |operator, left, right| match operator {
                Operator::Multiply => left
                    .checked_mul(right)
                    .ok_or_else(|| "preprocessor multiplication overflows i64".to_owned()),
                Operator::Divide => {
                    if right == 0 {
                        return Err("division by zero in preprocessor expression".to_owned());
                    }
                    left.checked_div(right)
                        .ok_or_else(|| "preprocessor division overflows i64".to_owned())
                }
                Operator::Remainder => {
                    if right == 0 {
                        return Err("remainder by zero in preprocessor expression".to_owned());
                    }
                    left.checked_rem(right)
                        .ok_or_else(|| "preprocessor remainder overflows i64".to_owned())
                }
                _ => unreachable!("multiplicative parser received another operator"),
            },
        )
    }

    fn parse_binary(
        &mut self,
        next: fn(&mut Self) -> Result<i64, String>,
        operators: &[Operator],
        operation: impl Fn(Operator, i64, i64) -> Result<i64, String>,
    ) -> Result<i64, String> {
        let mut value = next(self)?;
        while let Token::Operator(operator) = self.peek() {
            if !operators.contains(&operator) {
                break;
            }
            self.advance();
            value = operation(operator, value, next(self)?)?;
        }
        Ok(value)
    }

    fn parse_unary(&mut self) -> Result<i64, String> {
        match self.peek() {
            Token::Operator(Operator::LogicalNot) => {
                self.advance();
                Ok(i64::from(self.parse_unary()? == 0))
            }
            Token::Operator(Operator::BitNot) => {
                self.advance();
                Ok(!self.parse_unary()?)
            }
            Token::Operator(Operator::Add) => {
                self.advance();
                self.parse_unary()
            }
            Token::Operator(Operator::Subtract) => {
                self.advance();
                self.parse_unary()?
                    .checked_neg()
                    .ok_or_else(|| "preprocessor negation overflows i64".to_owned())
            }
            _ => self.parse_primary(),
        }
    }

    fn parse_primary(&mut self) -> Result<i64, String> {
        match self.advance() {
            Token::Number(value) => Ok(value),
            Token::Identifier("defined") => self.parse_defined(),
            Token::Identifier(name) => self.resolve_identifier(name),
            Token::LeftParenthesis => {
                let value = self.parse_logical_or()?;
                if self.advance() != Token::RightParenthesis {
                    return Err("preprocessor expression has an unmatched parenthesis".to_owned());
                }
                Ok(value)
            }
            _ => Err("expected a preprocessor expression value".to_owned()),
        }
    }

    fn parse_defined(&mut self) -> Result<i64, String> {
        let name = if self.peek() == Token::LeftParenthesis {
            self.advance();
            let Token::Identifier(name) = self.advance() else {
                return Err("defined() requires an identifier".to_owned());
            };
            if self.advance() != Token::RightParenthesis {
                return Err("defined() has an unmatched parenthesis".to_owned());
            }
            name
        } else {
            let Token::Identifier(name) = self.advance() else {
                return Err("defined requires an identifier".to_owned());
            };
            name
        };
        Ok(i64::from(self.macros.contains_key(name)))
    }

    fn resolve_identifier(&mut self, name: &str) -> Result<i64, String> {
        let Some(definition) = self.macros.get(name) else {
            return Ok(0);
        };
        let MacroDefinition::Object(replacement) = definition else {
            return Err(format!("function macro {name} is not valid in #if"));
        };
        if replacement.trim().is_empty() {
            return Ok(0);
        }
        if !self.resolving.insert(name.to_owned()) {
            return Err(format!("recursive macro {name} in #if"));
        }
        let result = evaluate_expression_with_resolving(replacement, self.macros, self.resolving);
        self.resolving.remove(name);
        result
    }

    fn peek(&self) -> Token<'a> {
        self.tokens.get(self.offset).copied().unwrap_or(Token::End)
    }

    fn advance(&mut self) -> Token<'a> {
        let token = self.tokens.get(self.offset).copied().unwrap_or(Token::End);
        self.offset += 1;
        token
    }
}

fn evaluate_expression_with_resolving(
    expression: &str,
    macros: &BTreeMap<String, MacroDefinition>,
    resolving: &mut BTreeSet<String>,
) -> Result<i64, String> {
    let mut parser = Parser {
        tokens: tokenize(expression)?,
        offset: 0,
        macros,
        resolving,
    };
    let value = parser.parse_logical_or()?;
    if parser.peek() != Token::End {
        return Err("unexpected trailing preprocessor expression tokens".to_owned());
    }
    Ok(value)
}

fn tokenize(expression: &str) -> Result<Vec<Token<'_>>, String> {
    let mut tokens = Vec::new();
    let mut offset = 0;
    while offset < expression.len() {
        let character = next_character(expression, offset)?;
        if character.is_ascii_whitespace() {
            offset += character.len_utf8();
            continue;
        }
        if is_identifier_start(character) {
            let end = identifier_end(expression, offset);
            tokens.push(Token::Identifier(&expression[offset..end]));
            offset = end;
            continue;
        }
        if character.is_ascii_digit() {
            let end = integer_literal_end(expression, offset);
            tokens.push(Token::Number(parse_integer(&expression[offset..end])?));
            offset = end;
            continue;
        }
        let (operator, width) = operator(&expression[offset..])?;
        match operator {
            Some(operator) => tokens.push(Token::Operator(operator)),
            None if character == '(' => tokens.push(Token::LeftParenthesis),
            None if character == ')' => tokens.push(Token::RightParenthesis),
            None => return Err(format!("unsupported #if token {character:?}")),
        }
        offset += width;
    }
    tokens.push(Token::End);
    Ok(tokens)
}

fn integer_literal_end(source: &str, start: usize) -> usize {
    let mut end = start;
    while end < source.len() {
        let character = source[end..]
            .chars()
            .next()
            .expect("literal scan is within source");
        if !(character.is_ascii_hexdigit()
            || matches!(character, 'x' | 'X' | 'u' | 'U' | 'l' | 'L'))
        {
            break;
        }
        end += character.len_utf8();
    }
    end
}

fn parse_integer(literal: &str) -> Result<i64, String> {
    let literal = literal.trim_end_matches(['u', 'U', 'l', 'L']);
    if let Some(hexadecimal) = literal
        .strip_prefix("0x")
        .or_else(|| literal.strip_prefix("0X"))
    {
        i64::from_str_radix(hexadecimal, 16)
            .map_err(|error| format!("invalid hexadecimal integer {literal:?}: {error}"))
    } else {
        literal
            .parse::<i64>()
            .map_err(|error| format!("invalid integer {literal:?}: {error}"))
    }
}

fn operator(source: &str) -> Result<(Option<Operator>, usize), String> {
    for (spelling, operator) in [
        ("||", Operator::LogicalOr),
        ("&&", Operator::LogicalAnd),
        ("==", Operator::Equal),
        ("!=", Operator::NotEqual),
        ("<=", Operator::LessEqual),
        (">=", Operator::GreaterEqual),
        ("<<", Operator::ShiftLeft),
        (">>", Operator::ShiftRight),
        ("|", Operator::BitOr),
        ("^", Operator::BitXor),
        ("&", Operator::BitAnd),
        ("<", Operator::Less),
        (">", Operator::Greater),
        ("+", Operator::Add),
        ("-", Operator::Subtract),
        ("*", Operator::Multiply),
        ("/", Operator::Divide),
        ("%", Operator::Remainder),
        ("!", Operator::LogicalNot),
        ("~", Operator::BitNot),
    ] {
        if source.starts_with(spelling) {
            return Ok((Some(operator), spelling.len()));
        }
    }
    Ok((None, next_character(source, 0)?.len_utf8()))
}
