use std::collections::HashMap;

use bstr::{BStr, BString};
use pex::instruction::Instruction;
use pex::{Function, Module, StringKey, StringTable, Value};

#[derive(Clone, Debug)]
pub struct Runner {
    modules: Vec<Module>,
    /// (Class, Property) => Property
    properties: HashMap<(BString, BString), Property>,
}

impl Runner {
    pub fn new() -> Self {
        Self {
            modules: Vec::new(),
            properties: HashMap::new(),
        }
    }

    pub fn register(&mut self, module: Module) {
        for class in &module.classes {
            for prop in &class.properties {
                if let Some(var) = prop.autovar_name {
                    let class = module.strings.get(class.name).unwrap();
                    let prop = module.strings.get(prop.name).unwrap();

                    self.properties
                        .insert((class.to_owned(), prop.to_owned()), Property::Auto(var));
                }
            }
        }

        self.modules.push(module);
    }

    pub fn call(&self, class: &BStr, func: &BStr, params: Vec<Value>) -> Value {
        let (module, class, func) = self.get_func(class.into(), func.into());

        self.call_in(module, func, class.name, params, &mut HashMap::new())
    }

    fn get_func(&self, class: &BStr, func: &BStr) -> (&pex::Module, &pex::Class, &pex::Function) {
        for module in &self.modules {
            for class in &module.classes {
                for state in &class.states {
                    for f in &state.functions {
                        if module.strings.get(f.name).unwrap() == func {
                            return (module, class, &f.function);
                        }
                    }
                }
            }
        }

        todo!("unknown function {}.{}", class, func)
    }

    fn call_in(
        &self,
        module: &Module,
        func: &Function,
        self_ty: StringKey,
        params: Vec<Value>,
        globals: &mut HashMap<StringKey, Value>,
    ) -> Value {
        let mut locals = HashMap::new();
        let mut local_types = HashMap::new();
        for (param, value) in func.params.iter().zip(params) {
            locals.insert(param.name, value);
            local_types.insert(param.name, param.ty);
        }

        for local in &func.locals {
            local_types.insert(local.name, local.ty);
            locals.insert(local.name, Value::Null);
        }

        let mut global_types = HashMap::new();
        for class in &module.classes {
            for var in &class.variables {
                if !globals.contains_key(&var.def.name) {
                    globals.insert(var.def.name, var.value);
                }

                global_types.insert(var.def.name, var.def.ty);
            }
        }

        let mut frame = Frame {
            pc: 0,
            locals,
            local_types,
            self_ty,
            global_types,
            globals: globals.clone(),
        };

        loop {
            match &func.instructions[frame.pc] {
                Instruction::Nop => {}
                Instruction::IAdd(op) => {
                    let lhs: i32 = frame.load(op.lhs);
                    let rhs: i32 = frame.load(op.rhs);
                    frame.store(op.dst, Value::Int(lhs + rhs));
                }
                Instruction::ISub(op) => {
                    let lhs: i32 = frame.load(op.lhs);
                    let rhs: i32 = frame.load(op.rhs);
                    frame.store(op.dst, Value::Int(lhs - rhs));
                }
                Instruction::IMul(op) => {
                    let lhs: i32 = frame.load(op.lhs);
                    let rhs: i32 = frame.load(op.rhs);
                    frame.store(op.dst, Value::Int(lhs * rhs));
                }
                Instruction::IDiv(op) => {
                    let lhs: i32 = frame.load(op.lhs);
                    let rhs: i32 = frame.load(op.rhs);
                    frame.store(op.dst, Value::Int(lhs / rhs));
                }
                Instruction::IMod(op) => {
                    let lhs: i32 = frame.load(op.lhs);
                    let rhs: i32 = frame.load(op.rhs);
                    frame.store(op.dst, Value::Int(lhs % rhs));
                }
                Instruction::INeg(op) => {
                    let src: i32 = frame.load(op.src);
                    frame.store(op.dst, Value::Int(-src));
                }
                Instruction::CmpEq(op) => {
                    let lhs: Value = frame.load(op.lhs);
                    let rhs: Value = frame.load(op.rhs);
                    frame.store(op.dst, Value::Bool(lhs == rhs));
                }
                Instruction::CmpLt(op) => {
                    let lhs: Value = frame.load(op.lhs);
                    let rhs: Value = frame.load(op.rhs);
                    match (lhs, rhs) {
                        (Value::Int(lhs), Value::Int(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs < rhs));
                        }
                        (Value::Float(lhs), Value::Float(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs < rhs));
                        }
                        _ => todo!(),
                    }
                }
                Instruction::CmpLe(op) => {
                    let lhs: Value = frame.load(op.lhs);
                    let rhs: Value = frame.load(op.rhs);
                    match (lhs, rhs) {
                        (Value::Int(lhs), Value::Int(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs <= rhs));
                        }
                        (Value::Float(lhs), Value::Float(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs <= rhs));
                        }
                        _ => todo!(),
                    }
                }
                Instruction::CmpGt(op) => {
                    let lhs: Value = frame.load(op.lhs);
                    let rhs: Value = frame.load(op.rhs);
                    match (lhs, rhs) {
                        (Value::Int(lhs), Value::Int(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs > rhs));
                        }
                        (Value::Float(lhs), Value::Float(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs > rhs));
                        }
                        _ => todo!(),
                    }
                }
                Instruction::CmpGe(op) => {
                    let lhs: Value = frame.load(op.lhs);
                    let rhs: Value = frame.load(op.rhs);
                    match (lhs, rhs) {
                        (Value::Int(lhs), Value::Int(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs >= rhs));
                        }
                        (Value::Float(lhs), Value::Float(rhs)) => {
                            frame.store(op.dst, Value::Bool(lhs >= rhs));
                        }
                        _ => todo!(),
                    }
                }
                Instruction::Jump(op) => {
                    frame.pc = (frame.pc as isize + op.offset as isize) as usize;
                    continue;
                }
                Instruction::JumpIfTrue(op) => {
                    let condition: bool = frame.load(op.condition);
                    if condition {
                        frame.pc = (frame.pc as isize + op.offset as isize) as usize;
                        continue;
                    }
                }
                Instruction::JumpIfFalse(op) => {
                    let condition: bool = frame.load(op.condition);
                    if !condition {
                        frame.pc = (frame.pc as isize + op.offset as isize) as usize;
                        continue;
                    }
                }
                Instruction::Return(value) => {
                    let value = frame.load(*value);
                    *globals = frame.globals;
                    return value;
                }
                Instruction::Store(op) => {
                    let src: Value = frame.load(op.src);
                    frame.store(op.dst, src);
                }
                Instruction::CallStatic(op) => {
                    let class = module.strings.get(op.class).unwrap();
                    let func = module.strings.get(op.function).unwrap();
                    let (module, class, func) = self.get_func(class, func);

                    let args = op.args.iter().map(|arg| frame.load(*arg)).collect();
                    let res = self.call_in(module, func, class.name, args, &mut frame.globals);

                    frame.store(op.return_assignment, res);
                }
                Instruction::CallMethod(op) => {
                    let object: Value = frame.load(op.object);
                    let class = frame.ty(object, &module.strings);
                    let func = module.strings.get(op.function_name).unwrap();
                    let (module, class, func) = self.get_func(class.into(), func);

                    let args = op.args.iter().map(|arg| frame.load(*arg)).collect();
                    let res = self.call_in(module, func, class.name, args, &mut frame.globals);

                    frame.store(op.return_assignment, res);
                }
                Instruction::PropertyGet(op) => {
                    let class = frame.ty(op.value, &module.strings);
                    let property = module.strings.get(op.property).unwrap();
                    let prop = self
                        .properties
                        .get(&(class.into(), property.into()))
                        .unwrap();

                    let value = match prop {
                        Property::Auto(var) => frame.globals.get(var).unwrap(),
                    };

                    frame.store(op.return_assignment, *value);
                }
                Instruction::PropertySet(op) => {
                    let class = frame.ty(op.object, &module.strings);
                    let property = module.strings.get(op.property).unwrap();
                    let prop = self
                        .properties
                        .get(&(class.into(), property.into()))
                        .unwrap();

                    let value = frame.load(op.value);

                    match prop {
                        Property::Auto(var) => {
                            frame.store(Value::Ident(*var), value);
                        }
                    }
                }
                ins => todo!("{:?}", ins),
            }

            frame.pc += 1;
        }
    }
}

#[derive(Clone, Debug)]
struct Frame {
    pc: usize,
    locals: HashMap<StringKey, Value>,
    local_types: HashMap<StringKey, StringKey>,
    self_ty: StringKey,
    globals: HashMap<StringKey, Value>,
    global_types: HashMap<StringKey, StringKey>,
}

impl Frame {
    fn load<T>(&self, mut value: Value) -> T
    where
        T: ExtractValue,
    {
        if let Value::Ident(name) = value {
            if let Some(v) = self.locals.get(&name) {
                value = *v
            } else if let Some(v) = self.globals.get(&name) {
                value = *v
            }
        }

        T::extract(&value)
    }

    fn store(&mut self, dst: Value, value: Value) {
        match dst {
            Value::Ident(dst) => {
                if self.locals.contains_key(&dst) {
                    self.locals.insert(dst, value);
                } else if self.globals.contains_key(&dst) {
                    self.globals.insert(dst, value);
                }
            }
            _ => todo!(),
        }
    }

    fn ty<'a>(&self, value: Value, strings: &'a StringTable) -> &'a str {
        match value {
            Value::Null => "none",
            Value::Bool(_) => "bool",
            Value::Int(_) => "int",
            Value::Float(_) => "float",
            Value::String(_) => "string",
            Value::Ident(ident) => {
                let name = strings.get(ident).unwrap();
                if name.eq_ignore_ascii_case(b"self") {
                    return strings.get(self.self_ty).unwrap().try_into().unwrap();
                }

                if let Some(ty) = self.local_types.get(&ident) {
                    let ty = strings.get(*ty).unwrap().try_into().unwrap();
                    return ty;
                }

                if let Some(ty) = self.global_types.get(&ident) {
                    let ty = strings.get(*ty).unwrap().try_into().unwrap();
                    return ty;
                }

                todo!()
            }
        }
    }
}

trait ExtractValue {
    fn extract(value: &Value) -> Self;
}

impl ExtractValue for i32 {
    fn extract(value: &Value) -> Self {
        match value {
            Value::Int(v) => *v,
            _ => todo!(),
        }
    }
}

impl ExtractValue for Value {
    fn extract(value: &Value) -> Self {
        *value
    }
}

impl ExtractValue for bool {
    fn extract(value: &Value) -> Self {
        match value {
            Value::Bool(v) => *v,
            _ => todo!(),
        }
    }
}

#[derive(Clone, Debug)]
enum Property {
    Auto(StringKey),
}
