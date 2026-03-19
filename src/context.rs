use std::collections::HashMap;
use crate::ffi::CType;

#[allow(unpredictable_function_pointer_comparisons)]
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
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
    Dict(HashMap<String, Value>),
    Builtin(fn(Vec<Value>, &mut Context, &RuntimeContext) -> Value),
    Ret(Box<Value>),
}

impl Value {
    pub fn to_bool(&self) -> bool {
        use Value::*;

        match self.clone() {
            Int(i) => i != 0,
            List(l) => !l.is_empty(),
            Bool(b) => b,
            _ => false
        }
    }

    pub fn unpack_atom(&self) -> String {
        use Value::*;

        match self.clone() {
            Atom(a) => a,
            _ => panic!("unpacked non-atom")
        }
    }
}

pub struct Context {
    pub scopes: Vec<HashMap<String, Value>>,
    pub depth: i32,
}

impl Context {
    pub fn set(&mut self, k: String, v: Value) {
        self.scopes[self.depth as usize].insert(k, v);
    }

    pub fn get(&self, k: &str) -> Option<Value> {
        for scope in self.scopes.iter().rev() {
            if let Some(v) = scope.get(k) {
                return Some(v.clone());
            }
        }
        None
    }

    pub fn push_scope(&mut self) {
        self.scopes.push(HashMap::new());
        self.depth += 1;
    }

    pub fn pop_scope(&mut self) {
        if self.depth > 0 {
            self.scopes.pop();
            self.depth -= 1;
        }
    }
}

#[derive(Debug, Clone)]
pub struct RuntimeContext {
    pub in_repl: bool,
    pub in_include: bool,
    pub argv: Vec<String>,
}
