#![allow(dead_code)]
#![feature(const_destruct)]
#![feature(const_trait_impl)]
#![feature(const_array)]

use std::io;

use crate::iso9660::{Bcd, DiscWrite, Mss};

#[path = "./iso9660/iso9660.rs"]
pub mod iso9660;

pub struct EncodeCtx {
    cursor: Mss<Bcd>,
}

pub trait Encode: Sized {
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
    }
    fn encode<W: DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> io::Result<()>;
}

impl Encode for u8 {
    fn encode<W: DiscWrite>(&self, writer: &mut W, _: &EncodeCtx) -> io::Result<()> {
        writer.write_all(&[*self])
    }
}

macro_rules! impl_encode_primitive {
    ($type:ty) => {
        impl Encode for $type {
            fn encode<W: DiscWrite>(&self, writer: &mut W, _: &EncodeCtx) -> io::Result<()> {
                writer.write_all(&(*self).to_le_bytes())
            }
        }
    };
}

impl_encode_primitive!(u16);
impl_encode_primitive!(u32);
impl_encode_primitive!(u64);
impl_encode_primitive!(usize);

impl Encode for &str {
    fn encode<W: DiscWrite>(&self, writer: &mut W, _: &EncodeCtx) -> io::Result<()> {
        writer.write_all(self.as_bytes())
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode<W: DiscWrite>(&self, writer: &mut W, ctx: &EncodeCtx) -> io::Result<()> {
        for value in self {
            value.encode(writer, ctx)?;
        }
        Ok(())
    }
}

fn main() {
    println!("Hello, world!");
}
