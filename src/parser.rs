use crate::Value;

#[derive(Debug, Clone, PartialEq)]
enum TokenType {
    Int,
    Float,
    Str,
    Atom,
    LPar,
    RPar,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Token {
    token_expr: String,
    token_type: TokenType,
    line: usize,
    column: usize
}

pub fn tokenise(input: &str) -> Result<Vec<Token>, String> {
    let mut toks: Vec<Token> = Vec::new();
    let mut chars = input.chars().peekable();

    let mut line = 1;
    let mut column = 1;

    while let Some(&c) = chars.peek() {
        match c {
            '(' => {
                toks.push(Token {
                   token_expr: "(".into(),
                   token_type: TokenType::LPar,
                   line,
                   column
                });
                chars.next();
                column += 1;
            },
            ')' => {
                toks.push(Token {
                   token_expr: ")".into(),
                   token_type: TokenType::RPar,
                   line,
                   column
                });
                chars.next();
                column += 1;
            },
            '"' => {
                let mut token_expr: String = "".into();
                let start_line = line;
                let start_column = column;

                chars.next();
                column += 1;

                while let Some(&next_c) = chars.peek() {
                    if next_c == '\\' {
                        chars.next();
                        column += 1;
                        if let Some(&next_next_c) = chars.peek() {
                            match next_next_c {
                                't' => token_expr.push('\t'),
                                'n' => token_expr.push('\n'),
                                'r' => token_expr.push('\r'),
                                _ => token_expr.extend(['\\', next_next_c])
                            }
                            chars.next();
                            column += 1;
                        } else {
                            token_expr.push(next_c);
                            chars.next();
                            column += 1;
                        }
                    } else if next_c == '"' {
                        chars.next();
                        column += 1;
                        break;
                    } else {
                        if next_c == '\n' {
                            line += 1;
                            column = 0;
                        }
                        token_expr.push(next_c);
                        chars.next();
                        column += 1;
                    }
                }

                toks.push(Token {
                    token_expr: token_expr,
                    token_type: TokenType::Str,
                    line: start_line,
                    column: start_column
                });
            },
            'a'..='z' | 'A'..='Z' | '+' | '*' | '/' | '>' | '<' | '=' | '_' => {
                let mut token_expr: String = "".into();
                let start_column = column;

                while let Some(&next_c) = chars.peek() {
                    match next_c {
                        'a'..='z' | 'A'..='Z' | '+' | '*' | '/' | '>' | '<' | '=' | '_' => {
                            token_expr.push(next_c);
                            chars.next();
                            column += 1;
                        },
                        _ => break
                    }
                }

                toks.push(Token {
                    token_expr: token_expr,
                    token_type: TokenType::Atom,
                    line,
                    column: start_column
                })
            },
            '0'..='9' | '-' => {
                let mut token_expr: String = "".into();
                let mut token_type: TokenType = TokenType::Int;
                let start_column = column;

                if c == '-' {
                    if let Some(&next_c) = chars.peek() {
                        if next_c.is_ascii_digit() {
                            token_expr.push(c);
                            chars.next();
                            column += 1;
                        } else {
                            toks.push(Token {
                                token_expr: c.into(),
                                token_type: TokenType::Atom,
                                line,
                                column: start_column
                            });
                            chars.next();
                            column += 1;
                            continue;
                        }
                    } else {
                        toks.push(Token {
                            token_expr: c.into(),
                            token_type: TokenType::Atom,
                            line,
                            column: start_column
                        });
                        chars.next();
                        column += 1;
                        continue;
                    }
                }

                while let Some(&next_c) = chars.peek() {
                    if next_c.is_ascii_digit() {
                        token_expr.push(next_c);
                    } else if next_c == '.' {
                        token_expr.push(next_c);
                        token_type = TokenType::Float;
                    } else {
                        break;
                    }
                    chars.next();
                    column += 1;
                }

                toks.push(Token {
                    token_expr: token_expr,
                    token_type: token_type,
                    line,
                    column: start_column
                });
            },
            '\'' | '`' | ',' | '@' | '.' => {
                toks.push(Token {
                    token_expr: c.into(),
                    token_type: TokenType::Atom,
                    line,
                    column
                });
                chars.next();
                column += 1;
            },
            ' ' | '\t' => {
                chars.next();
                column += 1;
            },
            '\n' => {
                chars.next();
                line += 1;
                column = 1;
            },
            '\r' => {
                chars.next();
            },
            x => return Err(format!("could not recognise character '{}' at line {}, column {}", x, line, column)),
        }
    }

    Ok(toks)
}

pub fn parse_tokens_to_ast(tokens: &[Token], depth: usize, orig_quote: bool, orig_backquote: bool, start_token: Option<&Token>, index_offset: usize) -> Result<(Value, usize), String> {
    let mut out_list = Vec::new();
    let mut i = 0;
    let mut quote = orig_quote;
    let mut backquote = orig_backquote;
    let mut comma = false;
    let mut lambda = false;

    while i < tokens.len() {
        let tok = &tokens[i];

        match tok.token_type {
            TokenType::LPar => {
                backquote = backquote && !comma;
                let (parsed, consumed) = parse_tokens_to_ast(&tokens[i+1..], depth + 1, quote, backquote, Some(&tokens[i]), index_offset + i + 1)?;
                if quote | backquote {
                    if let Value::Eval(inner) = parsed {
                          out_list.push(Value::List(inner));
                    } else {
                          out_list.push(parsed);
                    }
                    quote = false;
                    backquote = false;
                    comma = false;
                } else if lambda {
                    if let Value::Eval(inner) = parsed {
                          out_list.push(Value::Lambda(inner));
                    } else {
                          out_list.push(parsed);
                    }
                    lambda = false;
                } else {
                    out_list.push(parsed);
                }
                i += consumed + 1;
            }
            TokenType::RPar => {
                return Ok((Value::Eval(out_list), i + 1));
            }
            TokenType::Int => {
                let val = tok.token_expr.parse::<i64>().unwrap();
                out_list.push(Value::Int(val));
                i += 1;
            }
            TokenType::Float => {
                let val = tok.token_expr.parse::<f64>().unwrap();
                out_list.push(Value::Float(val));
                i += 1;
            }
            TokenType::Str => {
                out_list.push(Value::Str(tok.token_expr.clone()));
                i += 1;
            }
            TokenType::Atom => {
                match tok.token_expr.as_str() {
                    "true" => out_list.push(Value::Bool(true)),
                    "false" => out_list.push(Value::Bool(false)),
                    "'" => quote = true,
                    "`" => backquote = true,
                    "," => {
                        if backquote {
                            comma = true
                        } else if quote {
                            out_list.push(Value::Atom(",".into()))
                        }
                    },
                    "." => lambda = true,
                    _ => {
                        comma = false;
                        out_list.push(Value::Atom(tok.token_expr.clone()))
                    }
                }
                i += 1;
            }
        }
    }

    if depth > 0 {
        return Err(format!(
            "missing closing ')' for expression starting at line {}, column {}",
            start_token.unwrap().line, start_token.unwrap().column
        ));
    }

    Ok((Value::Eval(out_list), i))
}
