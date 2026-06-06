use snafu::{Snafu, Whatever};

use crate::iso9660::{Bcd, DiscWrite, Mss};
pub struct EncodeCtx {
    pub cursor: Mss<Bcd>,
}

#[derive(Debug, Snafu)]
pub enum EncodeError {
    #[snafu(transparent)]
    IOError { source: std::io::Error },

    #[snafu(transparent)]
    Transparent { source: Whatever },
}

pub trait Encode: Sized {
    fn size(&self) -> usize {
        std::mem::size_of::<Self>()
    }
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError>;
}

impl Encode for u8 {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        _: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        writer.write_all(&[*self])?;
        Ok(())
    }
}

macro_rules! impl_encode_primitive {
    ($type:ty) => {
        impl Encode for $type {
            fn encode<W: DiscWrite + ?Sized>(
                &self,
                writer: &mut W,
                _: &EncodeCtx,
            ) -> Result<(), EncodeError> {
                writer.write_all(&(*self).to_le_bytes())?;
                Ok(())
            }
        }
    };
}

impl_encode_primitive!(u16);
impl_encode_primitive!(u32);
impl_encode_primitive!(u64);
impl_encode_primitive!(usize);

impl Encode for &str {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        _: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        writer.write_all(self.as_bytes())?;
        Ok(())
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        for value in self {
            value.encode(writer, ctx)?;
        }
        Ok(())
    }
}

#[derive(derive_more::Deref, derive_more::DerefMut, Clone, Copy)]
pub struct BigEndian<T>(pub T);
#[derive(derive_more::Deref, derive_more::DerefMut, Clone, Copy)]
pub struct LittleEndian<T>(pub T);

macro_rules! impl_endian_wrappers {
    ($type:ty,$method:ident) => {
        impl $type {
            const N: usize = size_of::<Self>();
            pub fn to_bytes(self) -> [u8; Self::N] {
                self.$method()
            }
        }
        impl Encode for $type {
            fn encode<W: DiscWrite + ?Sized>(
                &self,
                writer: &mut W,
                _: &EncodeCtx,
            ) -> Result<(), EncodeError> {
                writer.write_all(&(*self).to_bytes())?;
                Ok(())
            }
        }
    };
}

impl_endian_wrappers!(BigEndian<u8>, to_be_bytes);
impl_endian_wrappers!(BigEndian<u16>, to_be_bytes);
impl_endian_wrappers!(BigEndian<u32>, to_be_bytes);
impl_endian_wrappers!(BigEndian<u64>, to_be_bytes);

impl_endian_wrappers!(LittleEndian<u8>, to_le_bytes);
impl_endian_wrappers!(LittleEndian<u16>, to_le_bytes);
impl_endian_wrappers!(LittleEndian<u32>, to_le_bytes);
impl_endian_wrappers!(LittleEndian<u64>, to_le_bytes);

impl<T> From<T> for LittleEndian<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

impl<T> From<T> for BigEndian<T> {
    fn from(value: T) -> Self {
        Self(value)
    }
}

pub struct PaddedConst<T: Encode, const N: usize> {
    data: T,
}

impl<T: Encode> PaddedConst<T, 0> {
    #[must_use]
    pub fn new<const SIZE: usize>(data: T) -> PaddedConst<T, SIZE> {
        PaddedConst { data }
    }
}

impl<T: Encode, const N: usize> Encode for PaddedConst<T, N> {
    fn size(&self) -> usize {
        N
    }
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let block_size = self.size();
        assert!(
            self.data.size() <= block_size,
            "data ({}) is {} bytes, greater than data block size {}",
            std::any::type_name::<T>(),
            self.data.size(),
            block_size
        );
        let padding = block_size.saturating_sub(self.data.size());
        let fill = Fill::zero(padding);
        self.data.encode(writer, ctx)?;
        fill.encode(writer, ctx)?;
        Ok(())
    }
}
pub struct Fill {
    pub pattern:     &'static [u8],
    pub total_bytes: usize,
}

impl Fill {
    #[must_use]
    pub fn zero(bytes: usize) -> Self {
        Self {
            total_bytes: bytes,
            pattern:     &[0],
        }
    }
    #[must_use]
    pub fn new(bytes: usize, pat: &'static [u8]) -> Self {
        Self {
            pattern:     pat,
            total_bytes: bytes,
        }
    }
}

impl Encode for Fill {
    fn size(&self) -> usize {
        self.total_bytes
    }
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        _ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        let mut written = 0;
        while written < self.total_bytes {
            let remaining = self.total_bytes - written;
            let chunk = &self.pattern[..self.pattern.len().min(remaining)];
            writer.write_all(chunk)?;
            written += chunk.len();
        }
        Ok(())
    }
}

#[derive(Default)]
pub struct FillConst<const VALUE: u8, const N: usize>;

impl<const VALUE: u8, const N: usize> Encode for FillConst<VALUE, N> {
    fn size(&self) -> usize {
        N
    }
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        Fill::new(N, &[VALUE]).encode(writer, ctx)
    }
}

#[derive(Default)]
pub struct ByteConst<const VALUE: u8>;

impl<const VALUE: u8> Encode for ByteConst<VALUE> {
    fn size(&self) -> usize {
        1
    }
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        ctx: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        FillConst::<VALUE, 1>.encode(writer, ctx)
    }
}

#[derive(Debug, Clone, Copy, Snafu)]
pub enum StrToAsciiError {
    #[snafu(display("string is not ascii"))]
    NotAscii,
}

pub fn str_to_ascii_buf<const N: usize>(str: &str) -> Result<[u8; N], StrToAsciiError> {
    let str = str.as_ascii().ok_or(StrToAsciiError::NotAscii)?;
    let mut buf = [0u8; N];
    let len = N.min(str.len());
    buf[..len].copy_from_slice(&str.as_bytes()[..len]);
    Ok(buf)
}
