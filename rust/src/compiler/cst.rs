#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub enum Cst {
    Int(Int),
    Text(String),
    Symbol(Symbol),
    Call(Call),
    Assignment(Assignment),
    Error { rest: String, message: String },
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Int(pub u64);

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Symbol(pub String);

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Call {
    pub name: String,
    pub arguments: Vec<Cst>,
}

#[derive(Debug, PartialEq, Eq, Clone, Hash)]
pub struct Assignment {
    pub name: String,
    pub parameters: Vec<Cst>,
    pub body: Vec<Cst>,
}
