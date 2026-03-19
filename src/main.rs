use std::io::{self, Write, IsTerminal, BufRead};
use std::env;
use std::{collections::HashMap, fs};
use std::ffi::CString;

pub mod builtins;
pub mod context;
pub mod parser;
pub mod ffi;

use crate::context::{Value, Context, RuntimeContext};

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
                        ffi::call_ffi_function(fp, &type_list[..], &args[..], ret_type)
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

pub fn handle_operands(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Vec<Value> {
    args.into_iter()
        .map(|v| eval(v, ctx, runtime_ctx))
        .collect()
}

fn eval_str(s: &str, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let toks = match parser::tokenise(s) {
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

    let (ast, _) = match parser::parse_tokens_to_ast(&toks, 0, false, false, None, 0) {
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
    builtins::install_builtins(&mut ctx);

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

                let ret_val = builtins::builtin_fmt(vec![result], &mut ctx, &RuntimeContext { in_repl: true, in_include: false, argv: vec![] });
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
