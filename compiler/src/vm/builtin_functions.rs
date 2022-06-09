use super::{value::Value, Vm};
use crate::{builtin_functions::BuiltinFunction, compiler::lir::Instruction, input::Input};
use itertools::Itertools;
use log::{debug, trace};

impl Vm {
    pub(super) fn run_builtin_function(&mut self, builtin_function: BuiltinFunction) {
        trace!("run_builtin_function: builtin{:?}", builtin_function);

        let return_value = match builtin_function {
            BuiltinFunction::Add => self.add(),
            BuiltinFunction::Equals => Ok(self.equals()),
            BuiltinFunction::GetArgumentCount => self.get_argument_count(),
            BuiltinFunction::IfElse => self.if_else(),
            BuiltinFunction::Panic => self.panic_builtin(),
            BuiltinFunction::Print => self.print(),
            BuiltinFunction::StructGet => self.struct_get(),
            BuiltinFunction::StructGetKeys => self.struct_get_keys(),
            BuiltinFunction::StructHasKey => self.struct_has_key(),
            BuiltinFunction::TypeOf => Ok(self.type_of()),
            BuiltinFunction::Use => self.use_(),
            _ => panic!("Unhandled builtin function: {:?}", builtin_function),
        };
        let return_value = match return_value {
            Ok(value) => value,
            Err(panic_message) => self.panic(panic_message),
        };

        let return_object = self.heap.import(return_value);
        self.data_stack.push(return_object);
    }

    fn add(&mut self) -> Result<Value, String> {
        let b = self
            .pop_value()
            .unwrap()
            .try_into_int()
            .map_err(|it| format!("builtinAdd expects numbers as arguments, got {}.", it))?;
        let a = self
            .pop_value()
            .unwrap()
            .try_into_int()
            .map_err(|it| format!("builtinAdd expects numbers as arguments, got {}.", it))?;

        Ok((a + b).into())
    }

    fn equals(&mut self) -> Value {
        let b = self.pop_value().unwrap();
        let a = self.pop_value().unwrap();

        (a == b).into()
    }

    fn get_argument_count(&mut self) -> Result<Value, String> {
        let (_, chunk) = self
            .pop_value()
            .unwrap()
            .try_into_closure()
            .map_err(|it| format!("builtinAddArgumentCount expects a closure, got {}.", it))?;

        let num_args = self.chunks[chunk].num_args;
        Ok(Value::Int(num_args as u64))
    }

    fn if_else(&mut self) -> Result<Value, String> {
        let else_ = self.pop_value().unwrap();
        let then = self.pop_value().unwrap();
        let condition = self.pop_value().unwrap().try_into_symbol().map_err(|it| {
            format!(
                "builtinIfElse expects a boolean symbol as the condition, got {}.",
                it
            )
        })?;
        if then.clone().try_into_closure().is_err() {
            return Err(format!(
                "builtinIfElse expects a closure as the then, got {}.",
                then
            ));
        }
        if else_.clone().try_into_closure().is_err() {
            return Err(format!(
                "builtinIfElse expects a closure as the else, got {}.",
                else_
            ));
        }
        let condition = match condition.as_str() {
            "True" => true,
            "False" => false,
            _ => {
                return Err(format!(
                    "builtinIfElse expected True or False as a condition, but got {}",
                    condition
                ));
            }
        };

        let closure_object = self.heap.import(if condition { then } else { else_ });
        self.data_stack.push(closure_object);
        self.run_instruction(Instruction::Call);
        Ok(self.pop_value().unwrap())
    }

    fn panic_builtin(&mut self) -> Result<Value, String> {
        let message = self
            .pop_value()
            .unwrap()
            .try_into_text()
            .map_err(|it| format!("builtinPanic expects a text as the message, got {}.", it))?;

        Ok(self.panic(message))
    }

    fn print(&mut self) -> Result<Value, String> {
        let message = self
            .pop_value()
            .unwrap()
            .try_into_text()
            .map_err(|it| format!("builtinPrint expects a text as the message, got {}.", it))?;

        println!("{:?}", message);
        Ok(Value::nothing())
    }

    fn struct_get(&mut self) -> Result<Value, String> {
        let key = self.pop_value().unwrap();
        let struct_ = self
            .pop_value()
            .unwrap()
            .try_into_struct()
            .map_err(|it| format!("builtinStructGet expects struct, got {}.", it))?;

        match struct_.get(&key) {
            Some(value) => Ok(value.clone().into()),
            None => Err(format!("Struct does not contain key {:?}.", key)),
        }
    }

    fn struct_get_keys(&mut self) -> Result<Value, String> {
        let struct_ = self
            .pop_value()
            .unwrap()
            .try_into_struct()
            .map_err(|it| format!("builtinStructGetKeys expects struct, got {}.", it))?;

        Ok(Value::list(struct_.keys().cloned().collect()))
    }

    fn struct_has_key(&mut self) -> Result<Value, String> {
        let key = self.pop_value().unwrap();
        let struct_ = self
            .pop_value()
            .unwrap()
            .try_into_struct()
            .map_err(|it| format!("builtinStructHasKey expects struct, got {}.", it))?;

        Ok((struct_.contains_key(&key)).into())
    }

    fn type_of(&mut self) -> Value {
        let value = self.pop_value().unwrap();
        match value {
            Value::Int(_) => Value::Symbol("Int".to_owned()).into(),
            Value::Text(_) => Value::Symbol("Text".to_owned()).into(),
            Value::Symbol(_) => Value::Symbol("Symbol".to_owned()).into(),
            Value::Struct(_) => Value::Symbol("Struct".to_owned()).into(),
            Value::Closure { .. } => Value::Symbol("Function".to_owned()).into(),
        }
    }

    fn use_(&mut self) -> Result<Value, String> {
        let target = self
            .pop_value()
            .unwrap()
            .try_into_text()
            .map_err(|it| format!("builtinUse expects text as target, got {}.", it))?;
        let current_path_struct = self
            .pop_value()
            .unwrap()
            .try_into_struct()
            .map_err(|it| format!("builtinUse expects struct as current path, got {}.", it))?;

        let mut current_path = vec![];
        let mut index = 0;
        while let Some(component) = current_path_struct.get(&Value::Int(index)) {
            current_path.push(component.clone().try_into_text().map_err(|it| format!("builtinUse expects a struct as current path with only textual paths as values, but the map contains the value {} for key {}", it, index)));
            index += 1;
        }

        let target = UseTarget::parse(&target)?;

        if target.parent_navigations > current_path.len() {
            return Err("Too many parent navigations.".to_string());
        }

        // let inputs = target.resolve(&current_path[..]);
        // let input = match inputs
        //     .iter()
        //     .filter(|&it| db.get_input(it.to_owned()).is_some())
        //     .next()
        // {
        //     Some(target) => target,
        //     None => {
        //         return self.panic(format!(
        //             "Target doesn't exist. Checked the following path(s): {}",
        //             inputs.iter().map(|it| format!("{}", it)).join(", ")
        //         ));
        //     }
        // };

        Ok(Value::Symbol("Used".to_string()))

        // TODO: Continue implementing use.
        // let (lir, _) = db.lir(input.clone()).unwrap();
        // TODO: Run LIR.
        // let discover_result = db.run_all(input.to_owned(), import_chain.to_owned());

        // TODO: Put public identifiers into map.
        // hir.identifiers
        //     .iter()
        //     .map(|(id, key)| {
        //         let mut key = key.to_owned();
        //         key.get_mut(0..1).unwrap().make_ascii_uppercase();
        //         let key = Value::Symbol(key.to_owned());

        //         let value = match discover_result.get(id) {
        //             Some(value) => value.to_owned()?,
        //             None => return DiscoverResult::ErrorInHir,
        //         };

        //         DiscoverResult::Value((key, value))
        //     })
        //     .collect::<DiscoverResult<HashMap<Value, Value>>>()
        //     .map(|it| Value::Struct(it))
    }
}

struct UseTarget {
    parent_navigations: usize,
    path: Vec<String>,
}
impl UseTarget {
    const PARENT_NAVIGATION_CHAR: char = '.';

    fn parse(target: &str) -> Result<Self, String> {
        let mut parent_navigations = 0;
        let mut target = target;
        while target.chars().next() == Some(UseTarget::PARENT_NAVIGATION_CHAR) {
            parent_navigations += 1;
            target = &target[UseTarget::PARENT_NAVIGATION_CHAR.len_utf8()..];
        }

        let mut path = vec![];
        loop {
            let mut chars = vec![];
            while let Some(c) = target.chars().next() {
                if c == UseTarget::PARENT_NAVIGATION_CHAR {
                    break;
                }
                chars.push(c);
                target = &target[c.len_utf8()..];
            }

            if target.is_empty() {
                path.push(chars.into_iter().join(""));
                break;
            }

            if chars.is_empty() {
                return Err("Target contains consecutive dots (`.`) in the path.".to_owned());
            }

            path.push(chars.into_iter().join(""));
        }
        Ok(UseTarget {
            parent_navigations,
            path,
        })
    }

    fn resolve(&self, current_path: &[String]) -> Vec<Input> {
        let mut path = current_path.to_owned();
        if self.parent_navigations == 0 {
            assert!(!path.is_empty());
            let last = path.last_mut().unwrap();
            if last == ".candy" {
                path.pop();
            } else {
                *last = last
                    .strip_suffix(".candy")
                    .expect("File name must end with `.candy`.")
                    .to_owned();
            }
        } else {
            for _ in 0..self.parent_navigations {
                if path.is_empty() {
                    return vec![];
                }
                path.pop();
            }
        }

        for part in &self.path {
            path.push(part.to_owned());
        }

        let mut result = vec![];

        let mut subdirectory = path.clone();
        subdirectory.push(".candy".to_owned());
        result.push(Input::File(subdirectory));

        if path.len() >= 1 {
            let last = path.last_mut().unwrap();
            *last = format!("{}.candy", last);
            result.push(Input::File(path));
        }
        result
    }
}

impl Vm {
    fn pop_value(&mut self) -> Option<Value> {
        let address = self.data_stack.pop()?;
        Some(self.heap.export(address))
    }
}
