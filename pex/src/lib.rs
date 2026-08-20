//! Parser for the PEX file format used in compiled scripts.

mod encodable;
mod raw;

pub mod instruction;

const MAGIC_LE: [u8; 4] = [0xde, 0xc0, 0x57, 0xfa];
const MAGIC_BE: [u8; 4] = [0xfa, 0x57, 0xc0, 0xde];

const MAJOR_VERSION: u8 = 3;

use bstr::{BStr, BString};
use bytes::{Buf, BufMut};

use crate::encodable::{Encodable, Reader, Writer};
use crate::instruction::Instruction;
use crate::raw::Opcode;

#[derive(Clone, Debug)]
pub enum Error {
    Eof,
    BadMagic([u8; 4]),
    UnknownVersion(u8),
    Other(String),
    BadEnumValue(&'static str, u64),
    InvalidString(std::string::FromUtf8Error),
    InvalidStringPointer(u16),
    InvalidInstructionArgCount {
        op: Opcode,
        expected: usize,
        found: usize,
    },
    InvalidValue2 {
        op: Opcode,
        operand: &'static str,
        value: Value,
    },
}

impl std::fmt::Display for Error {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // FIXME: Implement this for real.
        std::fmt::Debug::fmt(self, f)
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Endianess {
    Little,
    Big,
}

#[derive(Clone, Debug)]
pub struct Module {
    pub endian: Endianess,
    pub strings: StringTable,
    pub classes: Vec<Class>,
}

impl Module {
    pub fn read(mut buf: &[u8]) -> Result<Self, Error> {
        let mut magic = [0; 4];
        buf.try_copy_to_slice(&mut magic).map_err(|_| Error::Eof)?;

        let endian = match magic {
            MAGIC_LE => Endianess::Little,
            MAGIC_BE => Endianess::Big,
            _ => return Err(Error::BadMagic(magic)),
        };

        let mut reader = Reader { buf, endian };

        let header = raw::Header::read(&mut reader)?;
        if header.major_version != MAJOR_VERSION {
            return Err(Error::UnknownVersion(header.major_version));
        }

        let strings = StringTable::read(&mut reader)?;

        // If debug info is present we must read the entire debug info
        // because we don't know how long it is, so we cannot skip it.
        let has_debug_info = u8::read(&mut reader)?;
        if has_debug_info != 0 {
            crate::raw::DebugInfo::read(&mut reader)?;
        }

        // User flags are only used in the editor. They are not used
        // in the execution environment.
        Vec::<crate::raw::UserFlag>::read(&mut reader)?;

        let classes = Vec::<Class>::read(&mut reader)?;

        Ok(Self {
            endian: reader.endian,
            strings,
            classes,
        })
    }

    pub fn write(&self) -> Vec<u8> {
        let mut buf = Vec::new();

        match self.endian {
            Endianess::Little => buf.put_slice(&MAGIC_LE),
            Endianess::Big => buf.put_slice(&MAGIC_BE),
        }

        let mut writer = Writer::new(&mut buf, self.endian);

        raw::Header {
            major_version: MAJOR_VERSION,
            minor_version: 9,
            // 02 = Fallout 4
            game_id: 02,
            compilation_time: 0,
            source_filename: BString::default(),
            username: BString::default(),
            machine_name: BString::default(),
        }
        .write(&mut writer);

        self.strings.write(&mut writer);

        // TODO: Debug info
        0u8.write(&mut writer);

        // User flags, we will leave this empty for now.
        Vec::<u8>::new().write(&mut writer);

        self.classes.write(&mut writer);

        buf
    }
}

#[derive(Clone, Debug, Default)]
pub struct StringTable {
    strings: Vec<BString>,
}

impl StringTable {
    pub fn get(&self, key: StringKey) -> Option<&BStr> {
        self.strings.get(usize::from(key.0)).map(|s| BStr::new(s))
    }

    pub fn insert(&mut self, string: BString) -> StringKey {
        let index = self.strings.len() as u16;
        self.strings.push(string);
        StringKey(index)
    }
}

impl Encodable for StringTable {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let strings = Vec::<BString>::read(reader)?;
        Ok(Self { strings })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.strings.write(writer);
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub struct StringKey(u16);

impl Encodable for StringKey {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        u16::read(reader).map(Self)
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.0.write(writer);
    }
}

#[derive(Clone, Debug)]
pub struct Class {
    pub name: StringKey,
    pub parent: StringKey,
    pub docstring: StringKey,
    pub is_const: bool,
    pub user_flags: u32,
    pub auto_state_name: StringKey,
    pub structs: Vec<Struct>,
    pub variables: Vec<Variable>,
    pub properties: Vec<Property>,
    pub states: Vec<State>,
}

impl Encodable for Class {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let _size = u32::read(reader)?;
        let parent = StringKey::read(reader)?;
        let docstring = StringKey::read(reader)?;
        let is_const = bool::read(reader)?;
        let user_flags = u32::read(reader)?;
        let auto_state_name = StringKey::read(reader)?;
        let structs = Vec::<Struct>::read(reader)?;
        let variables = Vec::<Variable>::read(reader)?;
        let properties = Vec::<Property>::read(reader)?;
        let states = Vec::<State>::read(reader)?;

        Ok(Self {
            name,
            parent,
            docstring,
            is_const,
            user_flags,
            auto_state_name,
            structs,
            variables,
            properties,
            states,
        })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);

        let mut buf = Vec::new();
        let mut nested_writer = Writer::new(&mut buf, writer.endian);
        self.parent.write(&mut nested_writer);
        self.docstring.write(&mut nested_writer);
        self.is_const.write(&mut nested_writer);
        self.user_flags.write(&mut nested_writer);
        self.auto_state_name.write(&mut nested_writer);
        self.structs.write(&mut nested_writer);
        self.variables.write(&mut nested_writer);
        self.properties.write(&mut nested_writer);
        self.states.write(&mut nested_writer);

        let size: u32 = buf.len() as u32 + 4;
        size.write(writer);
        writer.buf.put_slice(&buf);
    }
}

#[derive(Clone, Debug)]
pub struct Struct {
    pub name: StringKey,
    pub members: Vec<StructMember>,
}

impl Encodable for Struct {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let members = Vec::<StructMember>::read(reader)?;

        Ok(Self { name, members })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);
        self.members.write(writer);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct StructMember {
    pub name: StringKey,
    pub ty: StringKey,
    pub user_flags: u32,
    pub value: Value,
    pub is_const: bool,
    pub docstring: StringKey,
}

impl Encodable for StructMember {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let ty = StringKey::read(reader)?;
        let user_flags = u32::read(reader)?;
        let value = Value::read(reader)?;
        let is_const = bool::read(reader)?;
        let docstring = StringKey::read(reader)?;

        Ok(Self {
            name,
            ty,
            user_flags,
            value,
            is_const,
            docstring,
        })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);
        self.ty.write(writer);
        self.user_flags.write(writer);
        self.value.write(writer);
        self.is_const.write(writer);
        self.docstring.write(writer);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct VariableDefinition {
    pub name: StringKey,
    pub ty: StringKey,
}

impl Encodable for VariableDefinition {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let ty = StringKey::read(reader)?;

        Ok(Self { name, ty })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);
        self.ty.write(writer);
    }
}

#[derive(Copy, Clone, Debug)]
pub struct Variable {
    pub def: VariableDefinition,
    pub user_flags: u32,
    pub value: Value,
}

impl Encodable for Variable {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let def = VariableDefinition::read(reader)?;
        let user_flags = u32::read(reader)?;
        let value = Value::read(reader)?;

        Ok(Self {
            def,
            user_flags,
            value,
        })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.def.write(writer);
        self.user_flags.write(writer);
        self.value.write(writer);
    }
}

#[derive(Copy, Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Ident(StringKey),
    String(StringKey),
    Int(i32),
    Float(f32),
    Bool(bool),
}

impl Encodable for Value {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let tag = u8::read(reader)?;
        match tag {
            0 => Ok(Self::Null),
            1 => {
                let val = StringKey::read(reader)?;
                Ok(Self::Ident(val))
            }
            2 => {
                let val = StringKey::read(reader)?;
                Ok(Self::String(val))
            }
            3 => {
                let val = i32::read(reader)?;
                Ok(Self::Int(val))
            }
            4 => {
                let val = f32::read(reader)?;
                Ok(Self::Float(val))
            }
            5 => {
                let val = bool::read(reader)?;
                Ok(Self::Bool(val))
            }
            _ => Err(Error::BadEnumValue("Value", tag.into())),
        }
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        let tag: u8 = match self {
            Self::Null => 0,
            Self::Ident(_) => 1,
            Self::String(_) => 2,
            Self::Int(_) => 3,
            Self::Float(_) => 4,
            Self::Bool(_) => 5,
        };

        tag.write(writer);

        match self {
            Self::Null => {}
            Self::Ident(val) => val.write(writer),
            Self::String(val) => val.write(writer),
            Self::Int(val) => val.write(writer),
            Self::Float(val) => val.write(writer),
            Self::Bool(val) => val.write(writer),
        }
    }
}

#[derive(Clone, Debug)]
pub struct Property {
    pub name: StringKey,
    pub ty: StringKey,
    pub docstring: StringKey,
    pub user_flags: u32,
    pub autovar_name: Option<StringKey>,
    pub getter: Option<Function>,
    pub setter: Option<Function>,
}

impl Encodable for Property {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let ty = StringKey::read(reader)?;
        let docstring = StringKey::read(reader)?;
        let user_flags = u32::read(reader)?;
        let flags = u8::read(reader)?;

        let mut autovar_name = None;
        let mut getter = None;
        let mut setter = None;

        if flags & 0x04 != 0 {
            autovar_name = Some(StringKey::read(reader)?);
        } else {
            if flags & 0x01 != 0 {
                getter = Some(Function::read(reader)?);
            }

            if flags & 0x02 != 0 {
                setter = Some(Function::read(reader)?);
            }
        }

        Ok(Self {
            name,
            ty,
            docstring,
            user_flags,
            autovar_name,
            getter,
            setter,
        })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);
        self.ty.write(writer);
        self.docstring.write(writer);
        self.user_flags.write(writer);

        let mut flags = 0;
        flags |= u8::from(self.getter.is_some()) * 0x01;
        flags |= u8::from(self.setter.is_some()) * 0x02;
        flags |= u8::from(self.autovar_name.is_some()) * 0x04;
        flags.write(writer);

        if let Some(autovar_name) = &self.autovar_name {
            autovar_name.write(writer);
        }

        if let Some(getter) = &self.getter {
            getter.write(writer);
        }

        if let Some(setter) = &self.setter {
            setter.write(writer);
        }
    }
}

#[derive(Clone, Debug)]
pub struct State {
    /// The name of the state.
    pub name: StringKey,
    pub functions: Vec<NamedFunction>,
}

impl Encodable for State {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let functions = Vec::<NamedFunction>::read(reader)?;

        Ok(Self { name, functions })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);
        self.functions.write(writer);
    }
}

#[derive(Clone, Debug)]
pub struct NamedFunction {
    /// The name of this function.
    pub name: StringKey,
    pub function: Function,
}

impl Encodable for NamedFunction {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let name = StringKey::read(reader)?;
        let function = Function::read(reader)?;
        Ok(Self { name, function })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.name.write(writer);
        self.function.write(writer);
    }
}

#[derive(Clone, Debug)]
pub struct Function {
    pub return_ty: StringKey,
    pub docstring: StringKey,
    pub user_flags: u32,
    pub is_static: bool,
    pub is_native: bool,
    pub params: Vec<VariableDefinition>,
    pub locals: Vec<VariableDefinition>,
    pub instructions: Vec<Instruction>,
}

impl Encodable for Function {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let return_ty = StringKey::read(reader)?;
        let docstring = StringKey::read(reader)?;
        let user_flags = u32::read(reader)?;
        let flags = u8::read(reader)?;
        let params = Vec::<VariableDefinition>::read(reader)?;
        let locals = Vec::<VariableDefinition>::read(reader)?;
        let instructions = Vec::<Instruction>::read(reader)?;

        let is_static = flags & 0x01 != 0;
        let is_native = flags & 0x02 != 0;

        Ok(Self {
            return_ty,
            docstring,
            user_flags,
            is_static,
            is_native,
            params,
            locals,
            instructions,
        })
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        self.return_ty.write(writer);
        self.docstring.write(writer);
        self.user_flags.write(writer);

        let mut flags = 0;
        flags |= u8::from(self.is_static) * 0x01;
        flags |= u8::from(self.is_native) * 0x02;
        flags.write(writer);

        self.params.write(writer);
        self.locals.write(writer);
        self.instructions.write(writer);
    }
}
