use super::heap::ObjectPointer;
use crate::compiler::lir::ChunkIndex;
use im::HashMap;
use itertools::Itertools;
use std::fmt::{self, Display, Formatter};

/// A self-contained value. Unlike objects, these are not tied to a running VM,
/// which makes them useful for being sent through channels between multiple
/// reference-counted heaps, for example ones running concurrently logically, on
/// other cores, or on different computers.
///
/// VMs can import these values to turn them into heap-contained,
/// reference-counted objects. They can export objects from the heap into
/// self-contained values.
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub enum Value {
    Int(u64),
    Text(String),
    Symbol(String),
    Struct(HashMap<Value, Value>),
    Closure {
        captured: Vec<ObjectPointer>,
        body: ChunkIndex,
    },
}

impl Value {
    pub fn nothing() -> Value {
        Value::Symbol("Nothing".to_owned())
    }
    pub fn list(items: Vec<Value>) -> Self {
        let items = items
            .into_iter()
            .enumerate()
            .map(|(index, it)| (Value::Int(index as u64), it))
            .collect();
        Value::Struct(items)
    }

    pub fn try_into_int(self) -> Result<u64, Value> {
        match self {
            Value::Int(int) => Ok(int),
            it => Err(it),
        }
    }
    pub fn try_into_text(self) -> Result<String, Value> {
        match self {
            Value::Text(text) => Ok(text),
            it => Err(it),
        }
    }
    pub fn try_into_symbol(self) -> Result<String, Value> {
        match self {
            Value::Symbol(symbol) => Ok(symbol),
            it => Err(it),
        }
    }
    pub fn try_into_struct(self) -> Result<HashMap<Value, Value>, Value> {
        match self {
            Value::Struct(entries) => Ok(entries),
            it => Err(it),
        }
    }
    pub fn try_into_closure(self) -> Result<(Vec<ObjectPointer>, ChunkIndex), Value> {
        match self {
            Value::Closure { captured, body } => Ok((captured, body)),
            it => Err(it),
        }
    }
}

impl Display for Value {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Value::Int(int) => write!(f, "{}", int),
            Value::Text(text) => write!(f, "{:?}", text),
            Value::Symbol(symbol) => write!(f, "{}", symbol),
            Value::Struct(entries) => write!(
                f,
                "{{ {} }}",
                entries
                    .iter()
                    .map(|(key, value)| format!("{}: {}", key, value))
                    .join(", ")
            ),
            Value::Closure { body, .. } => {
                write!(f, "{{{}}}", body)
            }
        }
    }
}

impl From<u64> for Value {
    fn from(value: u64) -> Self {
        Value::Int(value)
    }
}
impl From<String> for Value {
    fn from(value: String) -> Self {
        Value::Text(value)
    }
}
impl From<bool> for Value {
    fn from(it: bool) -> Self {
        Value::Symbol(if it { "True" } else { "False" }.to_string())
    }
}
