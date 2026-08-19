use bytes::{Buf, BufMut};

use crate::encodable::{Encodable, Reader, Writer};
use crate::raw::Opcode;
use crate::{Error, StringKey, Value};

#[derive(Clone, Debug)]
pub enum Instruction {
    /// No operation
    Nop,
    /// 32-bit integer addition.
    ///
    /// Computes `lhs` + `rhs` and stores the result in `dst`.
    IAdd(Binop),
    /// 32-bit float addition.
    ///
    /// Computes `lhs` + `rhs` and stores the result in `dst`.
    FAdd(Binop),
    /// 32-bit integer subtraction.
    ///
    /// Computes `lhs` - `rhs` and stores the result in `dst`.
    ISub(Binop),
    FSub(Binop),
    IMul(Binop),
    FMul(Binop),
    IDiv(Binop),
    FDiv(Binop),
    IMod(Binop),
    Not(Unop),
    INeg(Unop),
    FNeg(Unop),
    Store(Unop),
    Cast(Unop),
    CmpEq(Binop),
    CmpLt(Binop),
    CmpLe(Binop),
    CmpGt(Binop),
    CmpGe(Binop),
    Jump(Jump),
    JumpIfTrue(ConditionalJump),
    JumpIfFalse(ConditionalJump),
    CallMethod(CallMethod),
    CallParent(CallParent),
    CallStatic(CallStatic),
    Return(Value),
    StringConcat(Binop),
    PropertyGet(PropertyGet),
    PropertySet(PropertySet),
    ArrayCreate(Unop),
    ArrayLength(Unop),
    ArrayGet(ArrayGet),
    ArraySet(ArraySet),
    ArrayFind(ArrayFind),
    ArrayRFind(ArrayFind),
    Is(Is),
    StructCreate(StructCreate),
    StructGet(StructGet),
    StructSet(StructSet),
    ArrayFindStruct(ArrayFindStruct),
    ArrayRFindStruct(ArrayFindStruct),
    ArrayAdd(ArrayAdd),
    ArrayInsert(ArrayInsert),
    ArrayPop(ArrayPop),
    ArrayRemove(ArrayRemove),
    ArrayClear(ArrayClear),
}

impl Encodable for Instruction {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let op = Opcode::read(reader)?;
        let mut args = Vec::with_capacity(op.arg_count() as usize);
        for _ in 0..op.arg_count() {
            let arg = Value::read(reader)?;
            args.push(arg);
        }

        if op.has_variable_arg_count() {
            let arg_count = Value::read(reader)?;
            let Value::Int(count) = arg_count else {
                return Err(Error::Other(format!(
                    "opcode {:?} requires variable data with integer, was: {:?}",
                    op, arg_count
                )));
            };

            for _ in 0..count {
                let arg = Value::read(reader)?;
                args.push(arg);
            }
        }

        match op {
            Opcode::Nop => Ok(Self::Nop),
            Opcode::IAdd => Binop::from_args(op, args).map(Self::IAdd),
            Opcode::FAdd => Binop::from_args(op, args).map(Self::FAdd),
            Opcode::ISub => Binop::from_args(op, args).map(Self::ISub),
            Opcode::FSub => Binop::from_args(op, args).map(Self::FSub),
            Opcode::IMul => Binop::from_args(op, args).map(Self::IMul),
            Opcode::FMul => Binop::from_args(op, args).map(Self::FMul),
            Opcode::IDiv => Binop::from_args(op, args).map(Self::IDiv),
            Opcode::FDiv => Binop::from_args(op, args).map(Self::FDiv),
            Opcode::IMod => Binop::from_args(op, args).map(Self::IMod),
            Opcode::Not => Unop::from_args(op, args).map(Self::Not),
            Opcode::INeg => Unop::from_args(op, args).map(Self::INeg),
            Opcode::FNeg => Unop::from_args(op, args).map(Self::FNeg),
            Opcode::Store => Unop::from_args(op, args).map(Self::Store),
            Opcode::Cast => Unop::from_args(op, args).map(Self::Cast),
            Opcode::CmpEq => Binop::from_args(op, args).map(Self::CmpEq),
            Opcode::CmpLt => Binop::from_args(op, args).map(Self::CmpLt),
            Opcode::CmpLe => Binop::from_args(op, args).map(Self::CmpLe),
            Opcode::CmpGt => Binop::from_args(op, args).map(Self::CmpGt),
            Opcode::CmpGe => Binop::from_args(op, args).map(Self::CmpGe),
            Opcode::Branch => Jump::from_args(op, args).map(Self::Jump),
            Opcode::BranchIfTrue => ConditionalJump::from_args(op, args).map(Self::JumpIfTrue),
            Opcode::BranchIfFalse => ConditionalJump::from_args(op, args).map(Self::JumpIfFalse),
            Opcode::CallMethod => CallMethod::from_args(op, args).map(Self::CallMethod),
            Opcode::CallParent => CallParent::from_args(op, args).map(Self::CallParent),
            Opcode::CallStatic => CallStatic::from_args(op, args).map(Self::CallStatic),
            Opcode::Return => Ok(Self::Return(args[0])),
            Opcode::StringConcat => Binop::from_args(op, args).map(Self::StringConcat),
            Opcode::PropertyGet => PropertyGet::from_args(op, args).map(Self::PropertyGet),
            Opcode::PropertySet => PropertySet::from_args(op, args).map(Self::PropertySet),
            Opcode::ArrayCreate => Unop::from_args(op, args).map(Self::ArrayCreate),
            Opcode::ArrayLength => Unop::from_args(op, args).map(Self::ArrayLength),
            Opcode::ArrayGet => ArrayGet::from_args(op, args).map(Self::ArrayGet),
            Opcode::ArraySet => ArraySet::from_args(op, args).map(Self::ArraySet),
            Opcode::ArrayFind => ArrayFind::from_args(op, args).map(Self::ArrayFind),
            Opcode::ArrayRFind => ArrayFind::from_args(op, args).map(Self::ArrayRFind),
            Opcode::Is => Is::from_args(op, args).map(Self::Is),
            Opcode::StructCreate => StructCreate::from_args(op, args).map(Self::StructCreate),
            Opcode::StructGet => StructGet::from_args(op, args).map(Self::StructGet),
            Opcode::StructSet => StructSet::from_args(op, args).map(Self::StructSet),
            Opcode::ArrayFindStruct => {
                ArrayFindStruct::from_args(op, args).map(Self::ArrayFindStruct)
            }
            Opcode::ArrayRFindStruct => {
                ArrayFindStruct::from_args(op, args).map(Self::ArrayRFindStruct)
            }
            Opcode::ArrayAdd => ArrayAdd::from_args(op, args).map(Self::ArrayAdd),
            Opcode::ArrayInsert => ArrayInsert::from_args(op, args).map(Self::ArrayInsert),
            Opcode::ArrayPop => ArrayPop::from_args(op, args).map(Self::ArrayPop),
            Opcode::ArrayRemove => ArrayRemove::from_args(op, args).map(Self::ArrayRemove),
            Opcode::ArrayClear => ArrayClear::from_args(op, args).map(Self::ArrayClear),
        }
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        let (op, args) = match self {
            Self::Nop => (Opcode::Nop, Vec::new()),
            Self::IAdd(op) => (Opcode::IAdd, op.to_args()),
            Self::ISub(op) => (Opcode::ISub, op.to_args()),
            Self::IMul(op) => (Opcode::IMul, op.to_args()),
            Self::IDiv(op) => (Opcode::IDiv, op.to_args()),
            Self::IMod(op) => (Opcode::IMod, op.to_args()),
            Self::FAdd(op) => (Opcode::FAdd, op.to_args()),
            Self::FSub(op) => (Opcode::FSub, op.to_args()),
            Self::FMul(op) => (Opcode::FMul, op.to_args()),
            Self::FDiv(op) => (Opcode::FDiv, op.to_args()),
            Self::Not(op) => (Opcode::Not, op.to_args()),
            Self::INeg(op) => (Opcode::INeg, op.to_args()),
            Self::FNeg(op) => (Opcode::FNeg, op.to_args()),
            Self::Store(op) => (Opcode::Store, op.to_args()),
            Self::Cast(op) => (Opcode::Cast, op.to_args()),
            Self::CmpEq(op) => (Opcode::CmpEq, op.to_args()),
            Self::CmpLt(op) => (Opcode::CmpLt, op.to_args()),
            Self::CmpLe(op) => (Opcode::CmpLe, op.to_args()),
            Self::CmpGt(op) => (Opcode::CmpGt, op.to_args()),
            Self::CmpGe(op) => (Opcode::CmpGe, op.to_args()),
            Self::Jump(op) => (Opcode::Branch, op.to_args()),
            Self::JumpIfTrue(op) => (Opcode::BranchIfTrue, op.to_args()),
            Self::JumpIfFalse(op) => (Opcode::BranchIfFalse, op.to_args()),
            Self::CallMethod(op) => (Opcode::CallMethod, op.to_args()),
            Self::CallParent(op) => (Opcode::CallParent, op.to_args()),
            Self::CallStatic(op) => (Opcode::CallStatic, op.to_args()),
            Self::Return(val) => (Opcode::Return, vec![*val]),
            Self::StringConcat(op) => (Opcode::StringConcat, op.to_args()),
            Self::PropertyGet(op) => (Opcode::PropertyGet, op.to_args()),
            Self::PropertySet(op) => (Opcode::PropertySet, op.to_args()),
            Self::ArrayCreate(op) => (Opcode::ArrayCreate, op.to_args()),
            Self::ArrayLength(op) => (Opcode::ArrayLength, op.to_args()),
            Self::ArrayGet(op) => (Opcode::ArrayGet, op.to_args()),
            Self::ArraySet(op) => (Opcode::ArraySet, op.to_args()),
            Self::ArrayFind(op) => (Opcode::ArrayFind, op.to_args()),
            Self::ArrayRFind(op) => (Opcode::ArrayRFind, op.to_args()),
            Self::Is(op) => (Opcode::Is, op.to_args()),
            Self::StructCreate(op) => (Opcode::StructCreate, op.to_args()),
            Self::StructGet(op) => (Opcode::StructGet, op.to_args()),
            Self::StructSet(op) => (Opcode::StructSet, op.to_args()),
            Self::ArrayFindStruct(op) => (Opcode::ArrayFindStruct, op.to_args()),
            Self::ArrayRFindStruct(op) => (Opcode::ArrayRFindStruct, op.to_args()),
            Self::ArrayAdd(op) => (Opcode::ArrayAdd, op.to_args()),
            Self::ArrayInsert(op) => (Opcode::ArrayInsert, op.to_args()),
            Self::ArrayPop(op) => (Opcode::ArrayPop, op.to_args()),
            Self::ArrayRemove(op) => (Opcode::ArrayRemove, op.to_args()),
            Self::ArrayClear(op) => (Opcode::ArrayClear, op.to_args()),
        };

        op.write(writer);
        for arg in &args[..op.arg_count() as usize] {
            arg.write(writer);
        }

        if op.has_variable_arg_count() {
            let count = args.len() - op.arg_count() as usize;
            Value::Int(count as i32).write(writer);

            for arg in &args[op.arg_count() as usize..] {
                arg.write(writer);
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct Unop {
    pub src: Value,
    pub dst: Value,
}

impl Unop {
    fn from_args(op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        if args.len() != 2 {
            return Err(Error::InvalidInstructionArgCount {
                op,
                expected: 2,
                found: args.len(),
            });
        }

        let dst = args[0];
        let src = args[1];

        Ok(Self { src, dst })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.dst, self.src]
    }
}

#[derive(Clone, Debug)]
pub struct Binop {
    pub dst: Value,
    pub lhs: Value,
    pub rhs: Value,
}

impl Binop {
    fn from_args(op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        if args.len() != 3 {
            return Err(Error::InvalidInstructionArgCount {
                op,
                expected: 3,
                found: args.len(),
            });
        }

        let dst = args[0];
        let lhs = args[1];
        let rhs = args[2];

        Ok(Self { lhs, rhs, dst })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.dst, self.lhs, self.rhs]
    }
}

/// An unconditional jump.
#[derive(Copy, Clone, Debug)]
pub struct Jump {
    /// The position to jump to relative to the current instruction.
    pub offset: i32,
}

impl Jump {
    fn from_args(op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        if args.len() != 1 {
            return Err(Error::InvalidInstructionArgCount {
                op,
                expected: 1,
                found: args.len(),
            });
        }

        let offset = args[0];
        let Value::Int(offset) = offset else {
            return Err(Error::InvalidValue2 {
                op: Opcode::Branch,
                operand: "offset",
                value: offset,
            });
        };

        Ok(Self { offset })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![Value::Int(self.offset)]
    }
}

#[derive(Copy, Clone, Debug)]
pub struct ConditionalJump {
    pub condition: Value,
    pub offset: i32,
}

impl ConditionalJump {
    fn from_args(op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        if args.len() != 2 {
            return Err(Error::InvalidInstructionArgCount {
                op,
                expected: 2,
                found: args.len(),
            });
        }

        let condition = args[0];
        let offset = args[1];
        let Value::Int(offset) = offset else {
            return Err(Error::InvalidValue2 {
                op,
                operand: "offset",
                value: offset,
            });
        };

        Ok(Self { condition, offset })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.condition, Value::Int(self.offset)]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallMethod {
    pub function_name: StringKey,
    /// The receiver object on which the function is called.
    pub object: Value,
    /// The variable that will store the return value.
    pub return_assignment: Value,
    pub args: Vec<Value>,
}

impl CallMethod {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let function_name = args[0];
        let Value::Ident(function_name) = function_name else {
            return Err(Error::InvalidValue2 {
                op: Opcode::CallMethod,
                operand: "function_name",
                value: function_name,
            });
        };

        let object = args[1];
        let return_assignment = args[2];

        let args = args[3..].to_vec();

        Ok(Self {
            function_name,
            object,
            return_assignment,
            args,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        let mut args = vec![
            Value::Ident(self.function_name),
            self.object,
            self.return_assignment,
        ];
        args.extend(&self.args);
        args
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallParent {
    /// The name of the function to call.
    pub function: StringKey,
    pub return_assignment: Value,
    pub args: Vec<Value>,
}

impl CallParent {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let function = args[0];
        let Value::Ident(function) = function else {
            return Err(Error::InvalidValue2 {
                op: Opcode::CallParent,
                operand: "function",
                value: function,
            });
        };

        let return_assignment = args[1];

        let args = args[2..].to_vec();

        Ok(Self {
            function,
            return_assignment,
            args,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        let mut args = vec![Value::Ident(self.function), self.return_assignment];
        args.extend(&self.args);
        args
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct CallStatic {
    pub class: StringKey,
    pub function: StringKey,
    pub return_assignment: Value,
    pub args: Vec<Value>,
}

impl CallStatic {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let class = args[0];
        let Value::Ident(class) = class else {
            return Err(Error::InvalidValue2 {
                op: Opcode::CallStatic,
                operand: "class",
                value: class,
            });
        };

        let function = args[1];
        let Value::Ident(function) = function else {
            return Err(Error::InvalidValue2 {
                op: Opcode::CallStatic,
                operand: "function",
                value: function,
            });
        };

        let return_assignment = args[2];

        let args = args[3..].to_vec();

        Ok(Self {
            class,
            function,
            return_assignment,
            args,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        let mut args = vec![
            Value::Ident(self.class),
            Value::Ident(self.function),
            self.return_assignment,
        ];
        args.extend(&self.args);
        args
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertyGet {
    /// The name of the property to load.
    pub property: StringKey,
    /// The value that contains the property to be loaded.
    pub value: Value,
    /// The variable where the loaded value should be stored.
    pub return_assignment: Value,
}

impl PropertyGet {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let property = args[0];
        let Value::Ident(property) = property else {
            return Err(Error::InvalidValue2 {
                op: Opcode::PropertyGet,
                operand: "property",
                value: property,
            });
        };

        let value = args[1];
        let return_assignment = args[2];

        Ok(Self {
            property,
            value,
            return_assignment,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![
            Value::Ident(self.property),
            self.value,
            self.return_assignment,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct PropertySet {
    /// The name of the property.
    pub property: StringKey,
    /// The object with the property.
    pub object: Value,
    /// The new value to store.
    pub value: Value,
}

impl PropertySet {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let property = args[0];
        let Value::Ident(property) = property else {
            return Err(Error::InvalidValue2 {
                op: Opcode::PropertySet,
                operand: "property",
                value: property,
            });
        };

        let object = args[1];
        let value = args[2];

        Ok(Self {
            property,
            object,
            value,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![Value::Ident(self.property), self.object, self.value]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayGet {
    pub array: Value,
    pub index: Value,
    pub return_assignment: Value,
}

impl ArrayGet {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let return_assignment = args[0];
        let array = args[1];
        let index = args[2];

        Ok(Self {
            array,
            index,
            return_assignment,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.return_assignment, self.array, self.index]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArraySet {
    pub array: Value,
    pub index: Value,
    pub value: Value,
}

impl ArraySet {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array = args[0];
        let index = args[1];
        let value = args[2];

        Ok(Self {
            array,
            index,
            value,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.array, self.index, self.value]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayFind {
    /// The array object to be searched.
    pub array_value: Value,
    /// The variable in which the return index will be written.
    ///
    /// `-1` should be written if the value is not found.
    pub return_index: Value,
    /// The value that should be searched for.
    pub value: Value,
    /// The index where to start searching from.
    pub start_index: Value,
}

impl ArrayFind {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array_value = args[0];
        let return_index = args[1];
        let value = args[2];
        let start_index = args[3];

        Ok(Self {
            array_value,
            return_index,
            value,
            start_index,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![
            self.array_value,
            self.return_index,
            self.value,
            self.start_index,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct Is {
    /// The variable where the bool will be written.
    pub return_assignment: Value,
    /// The object that will be checked.
    pub object: Value,
    /// The class name that will be checked for.
    pub class: StringKey,
}

impl Is {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let return_assignment = args[0];
        let object = args[1];

        let class = args[2];
        let Value::Ident(class) = class else {
            return Err(Error::InvalidValue2 {
                op: Opcode::Is,
                operand: "class",
                value: class,
            });
        };

        Ok(Self {
            return_assignment,
            object,
            class,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![
            self.return_assignment,
            self.object,
            Value::Ident(self.class),
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructCreate {
    pub return_assignment: Value,
}

impl StructCreate {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let return_assignment = args[0];

        Ok(Self { return_assignment })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.return_assignment]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructGet {
    /// The struct value.
    pub struct_value: Value,
    /// The value in which the loaded member value will be stored.
    pub return_assignment: Value,
    /// The name of the member value to load.
    pub member_name: StringKey,
}

impl StructGet {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let return_assignment = args[0];
        let struct_value = args[1];

        let member_name = args[2];
        let Value::Ident(member_name) = member_name else {
            return Err(Error::InvalidValue2 {
                op: Opcode::StructGet,
                operand: "member_name",
                value: member_name,
            });
        };

        Ok(Self {
            struct_value,
            return_assignment,
            member_name,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![
            self.return_assignment,
            self.struct_value,
            Value::Ident(self.member_name),
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct StructSet {
    /// The struct value.
    pub struct_value: Value,
    /// The name of the member.
    pub member_name: StringKey,
    /// The new value.
    pub value: Value,
}

impl StructSet {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let struct_value = args[0];
        let value = args[2];

        let member_name = args[1];
        let Value::Ident(member_name) = member_name else {
            return Err(Error::InvalidValue2 {
                op: Opcode::StructGet,
                operand: "member_name",
                value: member_name,
            });
        };

        Ok(Self {
            struct_value,
            member_name,
            value,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![
            self.struct_value,
            self.value,
            Value::Ident(self.member_name),
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayFindStruct {
    /// The array object.
    pub array_value: Value,
    /// The variable in which the index will be written.
    ///
    /// `-1` should be written if the value is not found.
    pub return_index: Value,
    /// The name of the member to search.
    pub member_name: StringKey,
    /// The value to search for in the given member.
    pub value: Value,
    /// The starting index within the array.
    pub start_index: Value,
}

impl ArrayFindStruct {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array_value = args[0];
        let return_index = args[1];

        let member_name = args[2];
        let Value::String(member_name) = member_name else {
            return Err(Error::InvalidValue2 {
                op: Opcode::StructGet,
                operand: "member_name",
                value: member_name,
            });
        };

        let value = args[3];
        let start_index = args[4];

        Ok(Self {
            array_value,
            return_index,
            member_name,
            value,
            start_index,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![
            self.array_value,
            self.return_index,
            Value::Ident(self.member_name),
            self.value,
            self.start_index,
        ]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayAdd {
    pub array: Value,
    pub value: Value,
    pub count: Value,
}

impl ArrayAdd {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array = args[0];
        let value = args[1];
        let count = args[2];

        Ok(Self {
            array,
            value,
            count,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.array, self.value, self.count]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayInsert {
    pub array: Value,
    pub value: Value,
    pub index: Value,
}

impl ArrayInsert {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array = args[0];
        let value = args[1];
        let index = args[2];

        Ok(Self {
            array,
            value,
            index,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.array, self.value, self.index]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayPop {
    pub array: Value,
}

impl ArrayPop {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array = args[0];

        Ok(Self { array })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.array]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayRemove {
    pub array_value: Value,
    pub index: Value,
    pub count: Value,
}

impl ArrayRemove {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array_value = args[0];
        let index = args[1];
        let count = args[2];

        Ok(Self {
            array_value,
            index,
            count,
        })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.array_value, self.index, self.count]
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct ArrayClear {
    /// The array that will be cleared.
    pub array: Value,
}

impl ArrayClear {
    fn from_args(_op: Opcode, args: Vec<Value>) -> Result<Self, Error> {
        let array = args[0];

        Ok(Self { array })
    }

    fn to_args(&self) -> Vec<Value> {
        vec![self.array]
    }
}
