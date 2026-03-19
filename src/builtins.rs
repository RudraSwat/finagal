use std::{fs, collections::HashMap};
use crate::{Value, Context, RuntimeContext, eval, eval_str, handle_operands, ffi::string_to_ctype};

pub fn builtin_add(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a + b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a + b),
        (Value::Str(a), Value::Str(b)) => Value::Str(format!("{}{}", a, b)),
        _ => panic!("type error"),
    }
}

pub fn builtin_sub(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a - b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a - b),
        _ => panic!("type error"),
    }
}

pub fn builtin_mul(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Int(a * b),
        (Value::Float(a), Value::Float(b)) => Value::Float(a * b),
        _ => panic!("type error"),
    }
}

pub fn builtin_div(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Float(*a as f64 / *b as f64),
        (Value::Float(a), Value::Float(b)) => Value::Float(a / b),
        _ => panic!("type error"),
    }
}

pub fn builtin_peek(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);

    match (&vals[0], &vals[1]) {
        (Value::List(a), Value::Int(b)) => a[*b as usize].clone(),
        (Value::Str(a), Value::Int(b)) => Value::Str(String::from(a.chars().nth(*b as usize).unwrap())),
        (Value::Dict(a), Value::Str(b)) => a.get(b).unwrap().clone(),
        _ => panic!("type error"),
    }
}

pub fn builtin_eq(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(vals[0] == vals[1])
}

pub fn builtin_gt(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    match (&vals[0], &vals[1]) {
        (Value::Int(a), Value::Int(b)) => Value::Bool(a > b),
        (Value::Float(a), Value::Float(b)) => Value::Bool(a > b),
        _ => panic!("invalid types for comparison: {:?} and {:?}", &vals[0], &vals[1]),
    }
}

pub fn builtin_and(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(vals[0].to_bool() && vals[1].to_bool())
}

pub fn builtin_or(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(vals[0].to_bool() || vals[1].to_bool())
}

pub fn builtin_not(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Bool(!vals[0].to_bool())
}

pub fn builtin_len(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    match &vals[0] {
        Value::Str(s) => Value::Int(s.len() as i64),
        Value::List(l) => Value::Int(l.len() as i64),
        _ => panic!("type error"),
    }
}

pub fn builtin_dict(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    let mut output_dict = HashMap::new();
    let mut key_name = String::from("");
    for (i, x) in vals.iter().enumerate() {
    	if i % 2 == 0 {
    		let Value::Str(inner_key_name) = x else {
    			panic!("expected Value::Str as key name");
    		};
    		key_name = inner_key_name.clone();
		} else {
			output_dict.insert(key_name.clone(), x.clone());
		}
    }

    Value::Dict(output_dict)
}

pub fn builtin_fmt(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
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
            Value::Dict(s) => {
                for (i, (k, v)) in s.into_iter().enumerate() {
                    output_str.push_str(&k);
                    output_str.push_str(": ");

                    let Value::Str(s) = builtin_fmt(Vec::from([v]), ctx, runtime_ctx) else {
                        panic!("expected Value::Str from fmt");
                    };

					output_str.push_str(&s);

					if i + 1 < s.len() {
						output_str.push_str(", ");
					}
                }
            },
            Value::Lambda(_) => output_str.push_str("<Lambda>"),
            Value::Builtin(_) => output_str.push_str("<Builtin>"),
            Value::FFI(_, _, _, _) => output_str.push_str("<FFI>"),
            Value::Ret(v) => {
                let Value::Str(s) = builtin_fmt(vec![*v], ctx, runtime_ctx) else {
                    panic!("expected Value::Str from fmt");
                };
                output_str.push_str(&s);
            }
        }
    }
    Value::Str(output_str)
}

pub fn builtin_print(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let formatted = builtin_fmt(args, ctx, runtime_ctx);
    if let Value::Str(s) = &formatted {
        print!("{}", s);
    }
    formatted
}

pub fn builtin_println(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let formatted = builtin_fmt(args, ctx, runtime_ctx);
    if let Value::Str(s) = &formatted {
        println!("{}", s);
    }
    formatted
}

pub fn builtin_setq(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
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

pub fn builtin_ret(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let vals = handle_operands(args, ctx, runtime_ctx);
    Value::Ret(Box::new(vals[0].clone()))
}

pub fn builtin_ifel(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    let cond_val = eval(args[0].clone(), ctx, runtime_ctx);

    match cond_val {
        Value::Bool(true) => eval(args[1].clone(), ctx, runtime_ctx),
        Value::Bool(false) => eval(args[2].clone(), ctx, runtime_ctx),
        _ => panic!("condition must evaluate to a Bool"),
    }
}

pub fn builtin_inc(args: Vec<Value>, ctx: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    if ctx.depth != 0 {
        panic!("can only include in global scope")
    }

    let filename = eval(args[0].clone(), ctx, runtime_ctx);

    match filename {
        Value::Str(s) => eval_str(&fs::read_to_string(&s).unwrap_or_else(|_| panic!("failed to read include file: {}", &s)), ctx, &RuntimeContext { in_repl: runtime_ctx.in_repl, in_include: true, argv: runtime_ctx.argv.clone() }),
        _ => panic!("include file name must be a String"),
    }
}

pub fn builtin_ffi(args: Vec<Value>, _ctx: &mut Context, _runtime_ctx: &RuntimeContext) -> Value {
    match &args[2] {
        Value::List(type_list) | Value::Eval(type_list) => {
            let fn_type_list = type_list.iter().map(|v| string_to_ctype(&v.unpack_atom())).collect();

            if let Value::Str(file_name) = &args[0] {
                if let Value::Str(fn_name) = &args[1] {
                    if let Value::Atom(ret_type) = &args[3] {
                        Value::FFI(file_name.into(), fn_name.into(), fn_type_list, string_to_ctype(ret_type))
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

pub fn builtin_runtime_ctx(_: Vec<Value>, _: &mut Context, runtime_ctx: &RuntimeContext) -> Value {
    // FIXME: return as struct
    // Value::List(vec![Value::Bool(runtime_ctx.in_repl), Value::Bool(runtime_ctx.in_include), Value::List(runtime_ctx.argv.clone().into_iter().map(|v| Value::Str(v)).collect())])
    Value::Dict(HashMap::from([(String::from("in_repl"), Value::Bool(runtime_ctx.in_repl)), (String::from("in_include"), Value::Bool(runtime_ctx.in_include)), (String::from("argv"), Value::List(runtime_ctx.argv.clone().into_iter().map(|v| Value::Str(v)).collect()))]))
}

pub fn install_builtins(ctx: &mut Context) {
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
    ctx.set("dict".into(), Value::Builtin(builtin_dict));
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
