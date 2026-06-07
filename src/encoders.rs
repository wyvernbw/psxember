use miette::{IntoDiagnostic, LabeledSpan, miette};
use thiserror::Error;

use crate::iso9660::{Bcd, DiscWrite, Mss};
pub struct EncodeCtx {
    pub cursor: Mss<Bcd>,
}

pub type EncodeError = miette::Report;

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
        writer.write_all(&[*self]).into_diagnostic()?;
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
                writer.write_all(&(*self).to_le_bytes()).into_diagnostic()?;
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
        writer.write_all(self.as_bytes()).into_diagnostic()?;
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

impl Encode for &[u8] {
    fn encode<W: DiscWrite + ?Sized>(
        &self,
        writer: &mut W,
        _: &EncodeCtx,
    ) -> Result<(), EncodeError> {
        writer.write_all(self).into_diagnostic()
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
                writer.write_all(&(*self).to_bytes()).into_diagnostic()?;
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
            writer.write_all(chunk).map_err(|err| {
                miette!(
                    code = err
                        .raw_os_error()
                        .map(|code| code.to_string())
                        .unwrap_or("unknown code".to_string()),
                    labels = [LabeledSpan::at_offset(
                        0,
                        format!("chunk data ({} bytes)", chunk.len())
                    )],
                    "failed to write fill data ({} total bytes): {} - {}",
                    self.total_bytes,
                    err,
                    err.kind()
                )
                .with_source_code(format!("{chunk:?}"))
            })?;
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

#[derive(Debug, Clone, Error)]
pub enum StrToAsciiError {
    #[error("string is not ascii: {0}")]
    NotAscii(String),
}

pub fn str_to_ascii_buf<const N: usize>(str: &str) -> Result<[u8; N], StrToAsciiError> {
    let str = str
        .as_ascii()
        .ok_or_else(|| StrToAsciiError::NotAscii(str.to_owned()))?;
    let mut buf = [0u8; N];
    let len = N.min(str.len());
    buf[..len].copy_from_slice(&str.as_bytes()[..len]);
    Ok(buf)
}
