use bstr::BString;
use bytes::{Buf, BufMut};

use crate::{Endianess, Error};

pub(crate) trait Encodable: Sized {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf;

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut;
}

#[derive(Clone, Debug)]
pub(crate) struct Reader<B> {
    pub(crate) buf: B,
    pub(crate) endian: Endianess,
}

pub(crate) struct Writer<B> {
    pub(crate) buf: B,
    pub(crate) endian: Endianess,
}

impl<B> Writer<B> {
    pub(crate) fn new(buf: B, endian: Endianess) -> Self {
        Self { buf, endian }
    }
}

macro_rules! impl_encodable_primitive {
    ($($t:ty),*) => {
        $(
            impl Encodable for $t {
                fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
                where
                    B: Buf,
                {
                    if reader.buf.remaining() >= size_of::<Self>() {
                        let mut bytes = [0; size_of::<Self>()];
                        reader.buf.copy_to_slice(&mut bytes);

                        match reader.endian {
                            Endianess::Little => Ok(Self::from_le_bytes(bytes)),
                            Endianess::Big => Ok(Self::from_be_bytes(bytes)),
                        }
                    } else {
                        Err(Error::Eof)
                    }
                }

                fn write<B>(&self, writer: &mut Writer<B>)
                where
                    B: BufMut,
                {
                    match writer.endian {
                        Endianess::Little => writer.buf.put_slice(&self.to_le_bytes()),
                        Endianess::Big => writer.buf.put_slice(&self.to_be_bytes()),
                    }
                }
            }
        )*
    };
}

impl_encodable_primitive!(u8, u16, u32, u64, i8, i16, i32, i64, f32, f64);

impl Encodable for bool {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        u8::read(reader).map(|b| b != 0)
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        u8::from(*self).write(writer);
    }
}

impl<T> Encodable for Vec<T>
where
    T: Encodable,
{
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let len = u16::read(reader)?;

        let mut elems = Vec::new();
        for _ in 0..len {
            let elem = T::read(reader)?;
            elems.push(elem);
        }

        Ok(elems)
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        let len: u16 = self.len().try_into().unwrap();
        len.write(writer);

        for elem in self {
            elem.write(writer);
        }
    }
}

impl Encodable for BString {
    fn read<B>(reader: &mut Reader<B>) -> Result<Self, Error>
    where
        B: Buf,
    {
        let len = u16::read(reader)?;
        let mut bytes = Vec::with_capacity(usize::min(len.into(), reader.buf.remaining()));
        for _ in 0..len {
            let b = u8::read(reader)?;
            bytes.push(b);
        }

        Ok(Self::new(bytes))
    }

    fn write<B>(&self, writer: &mut Writer<B>)
    where
        B: BufMut,
    {
        let len: u16 = self.len().try_into().unwrap();
        len.write(writer);
        writer.buf.put_slice(self);
    }
}
