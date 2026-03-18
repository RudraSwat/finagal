use std::io::{self, Write, IsTerminal, BufRead};
use std::env;
use std::{collections::HashMap, fs};
use std::ffi::{CString, c_char, c_void, CStr};
use libffi::low::{ffi_type, ffi_cif, prep_cif};
use libffi::raw::{ffi_type_sint64, ffi_type_float, ffi_type_double, ffi_type_pointer, ffi_abi_FFI_DEFAULT_ABI, ffi_call};

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
struct Token {
    token_expr: String,
    token_type: TokenType,
    line: usize,
    column: usize
}

#[allow(unpredictable_function_pointer_comparisons)]
#[derive(Debug, Clone, PartialEq)]
enum Value {
    Int(i64),
    Float(f64),
    Str(String),
    Atom(String),
    UnresolvedAtom(String),
    Bool(bool),
    List(Vec<Value>),
    Eval(Vec<Value>),
    FFI(String, String, Vec<CType>, CType),
    Lambda(Vec<Value>),
    Builtin(fn(Vec<Value>, &mut Context, &RuntimeContext) -> Value),
    Ret(Box<Value>),
}

impl Value {
    fn to_bool(&self) -> bool {
        use Value::*;

        match self.clone() {
            Int(i) => i != 0,
            List(l) => !l.is_empty(),
            Bool(b) => b,
            _ => false
        }
    }

    fn unpack_atom(&self) -> String {
        use Value::*;

        match self.clone() {
            Atom(a) => a,
            _ => panic!("unpacked non-atom")
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
enum CType {
    Char,
    Double,
    Float,
    Int,
    Long,
    LongLong,
    SChar,
    Short,
    UChar,
    UInt,
    ULong,
    ULongLong,
    UShort,
    Str,
}

fn string_to_ctype(s: &str) -> CType {
    match s {
        "Char" => CType::Char,
        "Double" => CType::Double,
        "Float" => CType::Float,
        "Int" => CType::Int,
        "Long" => CType::Long,
        "LongLong" => CType::LongLong,
        "SChar" => CType::SChar,
        "Short" => CType::Short,
        "UChar" => CType::UChar,
        "UInt" => CType::UInt,
        "ULong" => CType::ULong,
        "ULongLong" => CType::ULongLong,
        "UShort" => CType::UShort,
        "Str" => CType::Str,
        _ => panic!("unknown C type: {}", s),
    }
}

#[allow(static_mut_refs)]
unsafe fn call_ffi_function(func_ptr: unsafe extern "C" fn(), ctypes: &[CType], values: &[Value], return_type: CType) -> Value {
    assert!(ctypes.len() == values.len(), "number of ctypes and values must match");

    let mut int_storage: Vec<i64> = Vec::new();
    let mut float_storage: Vec<f64> = Vec::new();
    let mut string_storage: Vec<CString> = Vec::new();
    let mut string_ptrs: Vec<*const c_char> = Vec::new();
    let mut arg_types: Vec<*mut ffi_type> = Vec::with_capacity(values.len());
    let mut arg_values: Vec<*mut ::std::os::raw::c_void> = Vec::with_capacity(values.len());

    unsafe {
        for (val, ctype) in values.iter().zip(ctypes.iter()) {
            match (val, ctype) {
                (Value::Int(i), CType::Int) => {
                    int_storage.push(*i);
                    arg_types.push(&mut ffi_type_sint64);
                    arg_values.push(int_storage.last_mut().unwrap() as *mut _ as *mut _);
                }
                (Value::Float(f), CType::Float) => {
                    float_storage.push(*f as f64);
                    arg_types.push(&mut ffi_type_float);
                    arg_values.push(float_storage.last_mut().unwrap() as *mut _ as *mut _);
                }
                (Value::Float(f), CType::Double) => {
                    float_storage.push(*f);
                    arg_types.push(&mut ffi_type_double);
                    arg_values.push(float_storage.last_mut().unwrap() as *mut _ as *mut _);
                }
                (Value::Str(s), CType::Str) => {
                    let cstr = CString::new(s.as_str()).unwrap();
                    string_storage.push(cstr);
                    let ptr = string_storage.last().unwrap().as_ptr();
                    string_ptrs.push(ptr);
                    arg_types.push(&mut ffi_type_pointer);
                    arg_values.push(string_ptrs.last_mut().unwrap() as *mut _ as *mut c_void);
                }
                _ => panic!("unsupported type conversion: {:?} -> {:?}", val, ctype),
            }
        }

        let ret_type: *mut ffi_type = match return_type {
            CType::Int => &mut ffi_type_sint64 as *mut _,
            CType::Float => &mut ffi_type_float as *mut _,
            CType::Double => &mut ffi_type_double as *mut _,
            CType::Str => &mut ffi_type_pointer as *mut _,
            _ => panic!("unsupported return type")
        };

        let mut cif: ffi_cif = std::mem::zeroed();
        prep_cif(
            &mut cif,
            ffi_abi_FFI_DEFAULT_ABI,
            arg_types.len(),
            ret_type,
            arg_types.as_mut_ptr(),
        ).expect("prep_cif failed");

        let mut ret_storage: [u8; 16] = [0; 16];
        let ret_ptr: *mut ::std::os::raw::c_void = ret_storage.as_mut_ptr() as *mut _;

        ffi_call(
            &mut cif,
            Some(std::mem::transmute(func_ptr)),
            ret_ptr,
            arg_values.as_mut_ptr(),
        );

        match return_type {
            CType::Int => Value::Int(*(ret_ptr as *mut i64)),
            CType::Float => Value::Float(*(ret_ptr as *mut f32) as f64),
            CType::Double => Value::Float(*(ret_ptr as *mut f64)),
            CType::Str => {
                let ptr = *(ret_ptr as *mut *const c_char);
                if ptr.is_null() {
                    Value::Str("".to_string())
                } else {
                    Value::Str(CStr::from_ptr(ptr).to_string_lossy().into_owned())
                }
            },
            _ => panic!("unsupported")
        }
    }
}

struct Context {
    scopes: Vec<HashMap<String, Value>>,
    depth: i32,
}

impl Context {
    fn set(&mut self, k: String, v: Value) {
        self.scopes[self.depth as usize].insert(k, v);
    }

    fn get(&self, k: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(k) {
                return Some(v.clone());
            }
        }
        None
    }

    fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.depth += 1;
    }

    fn pop_scope(&mut self) {
        if self.depth > 0 {
            self.scopes.pop();
            self.depth -= 1;
        }
    }
}

#[derive(Debug, Clone)]
struct RuntimeContext {
    in_repl: bool,
    in_include: bool,
    argv: Vec<String>,
}

fn tokenise(input: &str) -> Result<Vec<Token>, String> {
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

fn parse_tokens_to_ast(tokens: &[Token], depth: usize, orig_quote: bool, orig_backquote: bool, start_token: Option<&Token>, index_offset: usize) -> Result<(Value, usize), String> {
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

fn eval(ast: Value, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    match &ast {
        Value::Atom(name) => {
            match ctx.get(&name) {
                Some(v) => {
                    v
                },
                None => {
                    Value::UnresolvedAtom(name.into())
                }
            }
        }
        Value::Eval(list) => {
            if list.is_empty() {
                return Value::List(Vec::new());
            }

            if let Value::Eval(_) = &list[0] {
                if list.len() == 1 {
                    return eval(list[0].clone(), ctx, runtime_ctx);
                } else {
                    let mut ret_vals: Vec<Value> = Vec::new();
                    for inner_eval in list {
                        match eval(inner_eval.clone(), ctx, runtime_ctx) {
                            Value::Ret(ret_val) => {
                                return ret_val.as_ref().clone()
                            }
                            v => {
                                ret_vals.push(v);
                            }
                        }
                    }
                    return Value::List(ret_vals);
                }
            }

            let mut eval_list = list.clone();
            let oper = eval(eval_list.remove(0), ctx, runtime_ctx);

            match oper {
                Value::Builtin(f) => f(eval_list, ctx, runtime_ctx),
                Value::Lambda(body) => {
                    let args = eval_list.iter().map(|v| {
                        match v {
                            Value::Atom(_) | Value::Eval(_) => {
                                eval(v.clone(), ctx, runtime_ctx)
                            }
                            _ => {
                                v.clone()
                            }
                        }
                    }).collect();

                    ctx.push_scope();
                    ctx.set("args".into(), Value::List(args));
                    let res = eval(Value::Eval(body.clone()), ctx, runtime_ctx);
                    ctx.pop_scope();
                    res
                },
                Value::FFI(file_name, fn_name, type_list, ret_type) => {
                    unsafe {
                        let args: Vec<Value> = eval_list.iter().map(|v| {
                            match v {
                                Value::Atom(_) | Value::Eval(_) => {
                                    eval(v.clone(), ctx, runtime_ctx)
                                }
                                _ => {
                                    v.clone()
                                }
                            }
                        }).collect();

                        let lib = libloading::Library::new(file_name).unwrap();

                        type FP = unsafe extern "C" fn ();
                        let fp = lib.get::<FP>(CString::new(fn_name).unwrap().to_bytes_with_nul()).unwrap();
                        let fp: FP = *fp;
                        call_ffi_function(fp, &type_list[..], &args[..], ret_type)
                    }
                }
                _ => {
                    let mut res = vec![oper.clone()];
                    res.extend(eval_list);
                    Value::List(res)
                }
            }
        }
        _ => ast,
    }
}

fn handle_operands(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Vec<Value> {
    args.into_iter()
        .map(|v| eval(v, ctx, runtime_ctx))
        .collect()
}

fn builtin_add(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
        (Value::Str(a), Value::Str(b)) => Value::Str(format!("{}{}", a, b)),
        _ => panic!("type error"),
    }
}

fn builtin_sub(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
        _ => panic!("type error"),
    }
}

fn builtin_mul(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
        _ => panic!("type error"),
    }
}

fn builtin_div(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Float(*a as f64 / *b as f64),
        (Value::Float(a), Value::Float(b)) => Value::Float(a / b),
        _ => panic!("type error"),
    }
}

fn builtin_peek(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::List(a), Value::Int(b)) => a[*b as usize].clone(),
        _ => panic!("type error"),
    }
}

fn builtin_eq(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(&vals[0] == &vals[1])
}

fn builtin_gt(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
        (Value::Float(a), Value::Float(b)) => Value::Bool(a > b),
        _ => panic!("invalid types for comparison: {:?} and {:?}", &vals[0], &vals[1]),
    }
}

fn builtin_and(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(vals[0].to_bool() && vals[1].to_bool())
}

fn builtin_or(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(vals[0].to_bool() || vals[1].to_bool())
}

fn builtin_not(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(!vals[0].to_bool())
}

fn builtin_len(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    match &vals[0] {
        Value::Str(s) => Value::Int(s.len() as i64),
        Value::List(l) => Value::Int(l.len() as i64),
        _ => panic!("type error"),
    }
}

fn builtin_fmt(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let mut output_str: String = "".into();

    let vals = handle_operands(args, ctx, runtime_ctx);

    for x in vals {
        match x {
            Value::Int(i) => output_str.push_str(&i.to_string()),
            Value::Float(f) => output_str.push_str(&f.to_string()),
            Value::Str(s) => output_str.push_str(&s),
            Value::Bool(b) => output_str.push_str(&b.to_string()),
            Value::Atom(a) => output_str.push_str(&format!("{}?", a)),
            Value::UnresolvedAtom(a) => output_str.push_str(&format!("?{}?", a)),
            Value::List(l) | Value::Eval(l) => {
                let inner = l.into_iter()
                             .map(|v| if let Value::Str(s) = builtin_fmt(vec![v], ctx, runtime_ctx) { s } else { "".into() })
                             .collect::<Vec<_>>()
                             .join(" ");
                output_str.push_str(&format!("({})", inner));
            },
            Value::Lambda(_) => output_str.push_str("<lambda>"),
            Value::Builtin(_) => output_str.push_str("<builtin>"),
            Value::FFI(_, _, _, _) => output_str.push_str("<ffi>"),
            Value::Ret(v) => {
                let Value::Str(s) = builtin_fmt(vec![*v], ctx, runtime_ctx) else {
                    panic!("expected Value::Str from fmt");
                };
                output_str.push_str(&s);
            },
        }
    }
    Value::Str(output_str)
}

fn builtin_print(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let formatted = builtin_fmt(args, ctx, runtime_ctx);
    if let Value::Str(s) = &formatted {
        print!("{}", s);
    }
    formatted
}

fn builtin_println(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let formatted = builtin_fmt(args, ctx, runtime_ctx);
    if let Value::Str(s) = &formatted {
        println!("{}", s);
    }
    formatted
}

fn builtin_setq(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let name = &args[0];
    let val_expr = &args[1];

    if let Value::Atom(name_str) = name {
        let val = eval(val_expr.clone(), ctx, runtime_ctx);
        ctx.set(name_str.clone(), val.clone());
        val
    } else {
        panic!("first argument must be an Atom");
    }
}

fn builtin_ret(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Ret(Box::new(vals[0].clone()))
}

fn builtin_ifel(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let cond_val = eval(args[0].clone(), ctx, runtime_ctx);

    match cond_val {
        Value::Bool(true) => eval(args[1].clone(), ctx, runtime_ctx),
        Value::Bool(false) => eval(args[2].clone(), ctx, runtime_ctx),
        _ => panic!("condition must evaluate to a Bool"),
    }
}

fn builtin_inc(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    if ctx.depth != 0 {
        panic!("can only include in global scope")
    }

    let filename = eval(args[0].clone(), ctx, runtime_ctx);

    match filename {
        Value::Str(s) => eval_str(&fs::read_to_string(&s).unwrap_or_else(|_| panic!("failed to read include file: {}", &s)), ctx, &RuntimeContext { in_repl: runtime_ctx.in_repl, in_include: true, argv: runtime_ctx.argv.clone() }),
        _ => panic!("include file name must be a string"),
    }
}

fn builtin_ffi(args: Vec<Value>, _ctx: &mut Context, _runtime_ctx: &RuntimeContext) -> Value {
    match &args[2] {
        Value::List(type_list) | Value::Eval(type_list) => {
            let fn_type_list = type_list.iter().map(|v| string_to_ctype(&v.unpack_atom())).collect();

            if let Value::Str(file_name) = &args[0] {
                if let Value::Str(fn_name) = &args[1] {
                    if let Value::Atom(ret_type) = &args[3] {
                        return Value::FFI(file_name.into(), fn_name.into(), fn_type_list, string_to_ctype(ret_type));
                    } else {
                        panic!("ffi needs return type");
                    }
                } else {
                    panic!("ffi needs function name");
                }
            } else {
                panic!("ffi needs file name")
            }
        },
        _ => panic!("ffi needs list of arguments")
    }
}

fn builtin_runtime_ctx(_: Vec<Value>, _: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    // FIXME: return as struct
    Value::List(vec![Value::Bool(runtime_ctx.in_repl), Value::Bool(runtime_ctx.in_include), Value::List(runtime_ctx.argv.clone().into_iter().map(|v| Value::Str(v)).collect())])
}

fn install_builtins(ctx: &mut Context) {
    ctx.set("+".into(), Value::Builtin(builtin_add));
    ctx.set("-".into(), Value::Builtin(builtin_sub));
    ctx.set("*".into(), Value::Builtin(builtin_mul));
    ctx.set("/".into(), Value::Builtin(builtin_div));
    ctx.set("@".into(), Value::Builtin(builtin_peek));
    ctx.set("=".into(), Value::Builtin(builtin_eq));
    ctx.set(">".into(), Value::Builtin(builtin_gt));
    ctx.set("and".into(), Value::Builtin(builtin_and));
    ctx.set("or".into(), Value::Builtin(builtin_or));
    ctx.set("not".into(), Value::Builtin(builtin_not));
    ctx.set("fmt".into(), Value::Builtin(builtin_fmt));
    ctx.set("len".into(), Value::Builtin(builtin_len));
    ctx.set("print".into(), Value::Builtin(builtin_print));
    ctx.set("println".into(), Value::Builtin(builtin_println));
    ctx.set("setq".into(), Value::Builtin(builtin_setq));
    ctx.set("ret".into(), Value::Builtin(builtin_ret));
    ctx.set("ifel".into(), Value::Builtin(builtin_ifel));
    ctx.set("inc".into(), Value::Builtin(builtin_inc));
    ctx.set("runtime_ctx".into(), Value::Builtin(builtin_runtime_ctx));
    ctx.set("ffi".into(), Value::Builtin(builtin_ffi));
}

fn eval_str(s: &str, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let toks = match tokenise(s) {
        Ok(toks) => toks,
        Err(msg) => {
            eprintln!("tokenising error: {}", msg);
            if runtime_ctx.in_repl {
                return Value::List(vec![]);
            } else {
                std::process::exit(1);
            }
        }
    };

    let (ast, _) = match parse_tokens_to_ast(&toks, 0, false, false, None, 0) {
        Ok((ast, consumed)) => (ast, consumed),
        Err(msg) => {
            eprintln!("parsing error: {}", msg);
            if runtime_ctx.in_repl {
                return Value::List(vec![]);
            } else {
                std::process::exit(1);
            }
        }
    };

    eval(ast, ctx, runtime_ctx)
}

fn main() {
    let mut ctx = Context { scopes: vec![HashMap::new()], depth: 0 };
    install_builtins(&mut ctx);

    let args: Vec<String> = env::args().collect();

    if args.len() > 1 {
        let filename = &args[1];
        let contents = fs::read_to_string(filename)
            .unwrap_or_else(|_| panic!("failed to read file: {}", filename));
        eval_str(&contents, &mut ctx, &RuntimeContext { in_repl: false, in_include: false, argv: env::args().skip(1).collect() });
    } else {
        let stdin = io::stdin();
        if std::io::stdin().is_terminal() {
            let mut buffer = String::new();
            loop {
                print!(">>> ");
                io::stdout().flush().unwrap();
                buffer.clear();
                if stdin.read_line(&mut buffer).unwrap() == 0 {
                    break;
                }

                let result = eval_str(&buffer, &mut ctx, &RuntimeContext { in_repl: true, in_include: false, argv: vec![] });

                if let Value::List(ret_list) = &result {
                    if ret_list.is_empty() {
                        continue;
                    }
                }

                let ret_val = builtin_fmt(vec![result], &mut ctx, &RuntimeContext { in_repl: true, in_include: false, argv: vec![] });
                if let Value::Str(s) = ret_val {
                    println!("ret -> {}", s);
                }
            }
        } else {
            let input: String = stdin.lock().lines().map(|l| l.unwrap()).collect::<Vec<_>>().join("\n");
            eval_str(&input, &mut ctx, &RuntimeContext { in_repl: false, in_include: false, argv: vec![] });
        }
    }
}
