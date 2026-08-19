use bstr::BString;
use bytes::{Buf, BufMut};

use crate::Error;
use crate::encodable::{Encodable, Reader, Writer};

macro_rules! binary_struct {
    ( $(#[$($struct_attr:tt)*])* $struct_vis:vis struct $struct_name:ident {
        $(
            $(#[$($field_attr:tt)*])* $field_vis:vis $field_name:ident : $field_ty:ty,
        )*
    }) => {
        // Recreate the struct definition.
        $(
            #[ $( $struct_attr )* ]
        )*
        $struct_vis struct $struct_name {
            $(
                $(#[$($field_attr)*])* $field_vis $field_name : $field_ty,
            )*
        }

        impl Encodable for $struct_name {
            fn read<B>(_reader: &mut Reader<B>) -> Result<Self, Error>
            where
                B: Buf,
            {
                Ok(Self {
                    $(
                        $field_name: <$field_ty as Encodable>::read(_reader)?,
                    )*
                })
            }

            fn write<B>(&self, _writer: &mut Writer<B>)
            where
                B: BufMut,
            {
                $(
                    self.$field_name.write(_writer);
                )*
            }
        }
    };
}

binary_struct! {
    #[derive(Clone, Debug)]
    pub struct Header {
        pub major_version: u8,
        pub minor_version: u8,
        pub game_id: u16,
        pub compilation_time: u64,
        pub source_filename: BString,
        pub username: BString,
        pub machine_name: BString,
    }
}

binary_struct! {
    #[derive(Copy, Clone, Debug)]
    pub struct UserFlag {
        _name_index: u16,
        _flag_index: u8,
    }
}

binary_struct! {
    #[derive(Clone, Debug)]
    pub struct DebugInfo {
        _modification_time: u64,
        pub functions: Vec<FunctionDebugInfo>,
        pub property_groups: Vec<PropertyGroupDebugInfo>,
        pub structs: Vec<StructDebugInfo>,
    }
}

binary_struct! {
    #[derive(Clone, Debug)]
    pub struct FunctionDebugInfo {
        pub class_name: u16,
        pub state_name: u16,
        pub function_name: u16,
        pub function_type: FunctionType,
        pub lines: Vec<u16>,
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum FunctionType {
    Unknown,
}

impl Encodable for FunctionType {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        match u8::read(reader)? {
            _ => Ok(Self::Unknown),
        }
    }

    fn write<B>(&self, _writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        todo!()
    }
}

binary_struct! {
    #[derive(Clone, Debug)]
    pub struct PropertyGroupDebugInfo {
        pub class_name: u16,
        pub group_name: u16,
        pub doc_string: u16,
        pub user_flags: u32,
        pub names: Vec<u16>,
    }
}

binary_struct! {
    #[derive(Clone, Debug)]
    pub struct StructDebugInfo {
        pub class_name: u16,
        pub struct_name: u16,
        pub member_names: Vec<u16>,
    }
}

#[derive(Copy, Clone, Debug, PartialEq, Eq, Hash)]
pub enum Opcode {
    Nop,
    IAdd,
    FAdd,
    ISub,
    FSub,
    IMul,
    FMul,
    IDiv,
    FDiv,
    IMod,
    Not,
    INeg,
    FNeg,
    Store,
    Cast,
    CmpEq,
    CmpLt,
    CmpLe,
    CmpGt,
    CmpGe,
    Branch,
    BranchIfTrue,
    BranchIfFalse,
    CallMethod,
    CallParent,
    CallStatic,
    Return,
    StringConcat,
    PropertyGet,
    PropertySet,
    ArrayCreate,
    ArrayLength,
    ArrayGet,
    ArraySet,
    ArrayFind,
    ArrayRFind,
    Is,
    StructCreate,
    StructGet,
    StructSet,
    ArrayFindStruct,
    ArrayRFindStruct,
    ArrayAdd,
    ArrayInsert,
    ArrayPop,
    ArrayRemove,
    ArrayClear,
}

impl Opcode {
    pub fn arg_count(&self) -> u32 {
        match self {
            Self::Nop => 0,
            Self::IAdd => 3,
            Self::FAdd => 3,
            Self::ISub => 3,
            Self::FSub => 3,
            Self::IMul => 3,
            Self::FMul => 3,
            Self::IDiv => 3,
            Self::FDiv => 3,
            Self::IMod => 3,
            Self::Not => 2,
            Self::INeg => 2,
            Self::FNeg => 2,
            Self::Store => 2,
            Self::Cast => 2,
            Self::CmpEq => 3,
            Self::CmpLt => 3,
            Self::CmpLe => 3,
            Self::CmpGt => 3,
            Self::CmpGe => 3,
            Self::Branch => 1,
            Self::BranchIfTrue => 2,
            Self::BranchIfFalse => 2,
            Self::CallMethod => 3,
            Self::CallParent => 2,
            Self::CallStatic => 3,
            Self::Return => 1,
            Self::StringConcat => 3,
            Self::PropertyGet => 3,
            Self::PropertySet => 3,
            Self::ArrayCreate => 2,
            Self::ArrayLength => 2,
            Self::ArrayGet => 3,
            Self::ArraySet => 3,
            Self::ArrayFind => 4,
            Self::ArrayRFind => 4,
            Self::Is => 3,
            Self::StructCreate => 1,
            Self::StructGet => 3,
            Self::StructSet => 3,
            Self::ArrayFindStruct => 5,
            Self::ArrayRFindStruct => 5,
            Self::ArrayAdd => 3,
            Self::ArrayInsert => 3,
            Self::ArrayPop => 1,
            Self::ArrayRemove => 3,
            Self::ArrayClear => 1,
        }
    }

    pub fn has_variable_arg_count(&self) -> bool {
        match self {
            Self::CallMethod => true,
            Self::CallParent => true,
            Self::CallStatic => true,
            _ => false,
        }
    }
}

impl Encodable for Opcode {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        match u8::read(reader)? {
            0x00 => Ok(Self::Nop),
            0x01 => Ok(Self::IAdd),
            0x02 => Ok(Self::FAdd),
            0x03 => Ok(Self::ISub),
            0x04 => Ok(Self::FSub),
            0x05 => Ok(Self::IMul),
            0x06 => Ok(Self::FMul),
            0x07 => Ok(Self::IDiv),
            0x08 => Ok(Self::FDiv),
            0x09 => Ok(Self::IMod),
            0x0a => Ok(Self::Not),
            0x0b => Ok(Self::INeg),
            0x0c => Ok(Self::FNeg),
            0x0d => Ok(Self::Store),
            0x0e => Ok(Self::Cast),
            0x0f => Ok(Self::CmpEq),
            0x10 => Ok(Self::CmpLt),
            0x11 => Ok(Self::CmpLe),
            0x12 => Ok(Self::CmpGt),
            0x13 => Ok(Self::CmpGe),
            0x14 => Ok(Self::Branch),
            0x15 => Ok(Self::BranchIfTrue),
            0x16 => Ok(Self::BranchIfFalse),
            0x17 => Ok(Self::CallMethod),
            0x18 => Ok(Self::CallParent),
            0x19 => Ok(Self::CallStatic),
            0x1a => Ok(Self::Return),
            0x1b => Ok(Self::StringConcat),
            0x1c => Ok(Self::PropertyGet),
            0x1d => Ok(Self::PropertySet),
            0x1e => Ok(Self::ArrayCreate),
            0x1f => Ok(Self::ArrayLength),
            0x20 => Ok(Self::ArrayGet),
            0x21 => Ok(Self::ArraySet),
            0x22 => Ok(Self::ArrayFind),
            0x23 => Ok(Self::ArrayRFind),
            0x24 => Ok(Self::Is),
            0x25 => Ok(Self::StructCreate),
            0x26 => Ok(Self::StructGet),
            0x27 => Ok(Self::StructSet),
            0x28 => Ok(Self::ArrayFindStruct),
            0x29 => Ok(Self::ArrayRFindStruct),
            0x2a => Ok(Self::ArrayAdd),
            0x2b => Ok(Self::ArrayInsert),
            0x2c => Ok(Self::ArrayPop),
            0x2d => Ok(Self::ArrayRemove),
            0x2e => Ok(Self::ArrayClear),
            val => Err(Error::BadEnumValue("Opcode", val.into())),
        }
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        let v: u8 = match self {
            Self::Nop => 0x00,
            Self::IAdd => 0x01,
            Self::FAdd => 0x02,
            Self::ISub => 0x03,
            Self::FSub => 0x04,
            Self::IMul => 0x05,
            Self::FMul => 0x06,
            Self::IDiv => 0x07,
            Self::FDiv => 0x08,
            Self::IMod => 0x09,
            Self::Not => 0x0a,
            Self::INeg => 0x0b,
            Self::FNeg => 0x0c,
            Self::Store => 0x0d,
            Self::Cast => 0x0e,
            Self::CmpEq => 0x0f,
            Self::CmpLt => 0x10,
            Self::CmpLe => 0x11,
            Self::CmpGt => 0x12,
            Self::CmpGe => 0x13,
            Self::Branch => 0x14,
            Self::BranchIfTrue => 0x15,
            Self::BranchIfFalse => 0x16,
            Self::CallMethod => 0x17,
            Self::CallParent => 0x18,
            Self::CallStatic => 0x19,
            Self::Return => 0x1a,
            Self::StringConcat => 0x1b,
            Self::PropertyGet => 0x1c,
            Self::PropertySet => 0x1d,
            Self::ArrayCreate => 0x1e,
            Self::ArrayLength => 0x1f,
            Self::ArrayGet => 0x20,
            Self::ArraySet => 0x21,
            Self::ArrayFind => 0x22,
            Self::ArrayRFind => 0x23,
            Self::Is => 0x24,
            Self::StructCreate => 0x25,
            Self::StructGet => 0x26,
            Self::StructSet => 0x27,
            Self::ArrayFindStruct => 0x28,
            Self::ArrayRFindStruct => 0x29,
            Self::ArrayAdd => 0x2a,
            Self::ArrayInsert => 0x2b,
            Self::ArrayPop => 0x2c,
            Self::ArrayRemove => 0x2d,
            Self::ArrayClear => 0x2e,
        };

        v.write(writer);
    }
}
