use std::marker::PhantomData;
use std::ops::RangeInclusive;
use std::simd::cmp::{SimdPartialEq, SimdPartialOrd};
use std::simd::{mask8x8, u8x8};

use miette::{Diagnostic, IntoDiagnostic, LabeledSpan, SourceSpan, miette};
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
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError>;
}

impl Encode for u8 {
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        writer.write_all(&[*self]).into_diagnostic()?;
        Ok(())
    }
}

macro_rules! impl_encode_primitive {
    ($type:ty) => {
        impl Encode for $type {
            fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
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
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        writer.write_all(self.as_bytes()).into_diagnostic()?;
        Ok(())
    }
}

impl<T: Encode, const N: usize> Encode for [T; N] {
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        for value in self {
            value.encode(writer)?;
        }
        Ok(())
    }
}

impl Encode for &[u8] {
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
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
            fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
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

pub struct PaddedConst<T: Encode, const N: usize, const PAD: u8 = 0> {
    data: T,
}

impl<T: Encode> PaddedConst<T, 0> {
    #[must_use]
    pub fn new<const SIZE: usize>(data: T) -> PaddedConst<T, SIZE> {
        PaddedConst { data }
    }

    #[must_use]
    pub fn new_with_padding<const SIZE: usize, const PAD: u8>(
        data: T,
    ) -> PaddedConst<T, SIZE, PAD> {
        PaddedConst { data }
    }
}

impl<T: Encode, const N: usize, const PAD: u8> Encode for PaddedConst<T, N, PAD> {
    fn size(&self) -> usize {
        N
    }
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        let block_size = self.size();
        assert!(
            self.data.size() <= block_size,
            "data ({}) is {} bytes, greater than data block size {}",
            std::any::type_name::<T>(),
            self.data.size(),
            block_size
        );
        let padding = block_size.saturating_sub(self.data.size());
        let fill = Fill::new(padding, &[PAD]);
        self.data.encode(writer)?;
        fill.encode(writer)?;
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
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
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
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        Fill::new(N, &[VALUE]).encode(writer)
    }
}

#[derive(Default)]
pub struct ByteConst<const VALUE: u8>;

impl<const VALUE: u8> Encode for ByteConst<VALUE> {
    fn size(&self) -> usize {
        1
    }
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        FillConst::<VALUE, 1>.encode(writer)
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

pub trait CharBuf<'a, B: Buffer<'a>>: Sized {
    const VALID_RANGES: &'static [RangeInclusive<u8>];
    const VALID_CHARS: &'static [u8];
    type ValidationErr: Diagnostic;

    fn bytes(&self) -> &[u8];
    fn len(&self) -> usize;

    /// thanks clippy >:(
    fn is_empty(&self) -> bool {
        self.len() == 0
    }

    fn make_error(src: &[u8], offset: usize) -> Self::ValidationErr;

    fn from_parts(bytes: B, len: usize) -> Self;

    fn parse(bytes: B, len: usize) -> Result<Self, Self::ValidationErr> {
        let len = len.min(bytes.as_bytes().len());
        let bytes_slice = &bytes.as_bytes()[..len];
        let (chunks, rem) = bytes_slice.as_bytes().as_chunks::<8>();
        let chunks_n = chunks.len() * 8;
        for (chunk_i, chunk_bytes) in chunks.iter().enumerate() {
            let chunk = u8x8::from_slice(chunk_bytes);
            let in_valid_range = Self::VALID_RANGES
                .iter()
                .map(|valid| {
                    let chunk = chunk - u8x8::splat(*valid.start());
                    chunk.simd_le(u8x8::splat(valid.len() as u8))
                })
                .reduce(|a, b| a | b)
                .unwrap_or_else(|| mask8x8::splat(false));

            let in_valid_chars = Self::VALID_CHARS
                .iter()
                .map(|c| chunk.simd_eq(u8x8::splat(*c)))
                .reduce(|a, b| a | b)
                .unwrap_or_else(|| mask8x8::splat(false));

            let valid = in_valid_range | in_valid_chars;
            if !valid.all() {
                let first_zero = chunk_i * 8 + valid.to_bitmask().trailing_ones() as usize;
                return Err(Self::make_error(bytes.as_bytes(), first_zero));
            }
        }
        for (i, byte) in rem.iter().enumerate() {
            let in_valid_range = Self::VALID_RANGES
                .iter()
                .map(|valid| valid.contains(byte))
                .reduce(|a, b| a | b)
                .unwrap_or(false);

            let in_valid_chars = Self::VALID_CHARS
                .iter()
                .map(|c| c == byte)
                .reduce(|a, b| a | b)
                .unwrap_or(false);

            let valid = in_valid_range | in_valid_chars;
            if !valid {
                return Err(Self::make_error(bytes.as_bytes(), i + chunks_n));
            }
        }

        Ok(Self::from_parts(bytes, len))
    }

    fn encode_chars<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        let bytes = &self.bytes()[..self.len()];
        writer.write_all(bytes).into_diagnostic()?;
        let remaining = self.bytes().len().saturating_sub(self.len());
        // pad remaining dchar buffer with spaces
        Fill::new(remaining, b" ").encode(writer)?;

        Ok(())
    }
}

/// # d-characters (Filenames)
/// ```plaintext
///    "0..9", "A..Z", and "_"
/// ```
#[derive(Debug, Clone, Copy)]
pub struct DChar(u8);
#[derive(Debug, Clone, Error, Diagnostic)]
#[error("value '{src}' is not valid dchar data")]
#[diagnostic(
    help = "dchars are numbers '0' through '9', letters 'A' through 'Z' (uppercase), and '_'"
)]
pub struct DCharError {
    #[source_code]
    src: String,

    #[label("not a d-char")]
    char_label: SourceSpan,
}

impl DChar {
    pub fn new(byte: u8) -> Result<Self, DCharError> {
        match byte {
            b'0'..=b'9' => Ok(Self(byte)),
            b'A'..=b'Z' => Ok(Self(byte)),
            b'_' => Ok(Self(byte)),
            _ => Err(DCharError {
                src:        format!("{}", byte as char),
                char_label: (0, 1).into(),
            }),
        }
    }
}

#[derive(Debug, Clone)]
pub struct DCharBuf<'a, B: Buffer<'a>> {
    bytes: B,
    len:   usize,
    _ty:   PhantomData<&'a [u8]>,
}

impl<'a, B: Buffer<'a>> CharBuf<'a, B> for DCharBuf<'a, B> {
    const VALID_RANGES: &'static [RangeInclusive<u8>] = &[b'0'..=b'9', b'A'..=b'Z'];
    const VALID_CHARS: &'static [u8] = b"_";

    type ValidationErr = DCharError;

    fn bytes(&self) -> &[u8] {
        self.bytes.as_bytes()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn make_error(src: &[u8], offset: usize) -> Self::ValidationErr {
        DCharError {
            src:        std::str::from_utf8(src)
                .expect("not valid utf8")
                .to_string(),
            char_label: (offset, 1).into(),
        }
    }

    fn from_parts(bytes: B, len: usize) -> Self {
        Self {
            bytes,
            len,
            _ty: PhantomData,
        }
    }
}

impl<'a, B: Buffer<'a>> Encode for DCharBuf<'a, B> {
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.encode_chars(writer)
    }
}

impl<'a, B: Buffer<'a>> DCharBuf<'a, B> {
    pub fn new(bytes: B, len: usize) -> Result<Self, DCharError> {
        Self::parse(bytes, len)
    }
}

#[derive(Debug, Clone, Error, Diagnostic)]
#[error("value '{src}' is not valid achar data")]
#[diagnostic(
    help = r#"achars are numbers '0' through '9', letters 'A' through 'Z' (uppercase), and '!"%&'()*+,-./:;<=>?_' characters."#r,
)]
pub struct ACharError {
    #[source_code]
    src: String,

    #[label("not an a-char")]
    char_label: SourceSpan,
}
#[derive(Debug, Clone)]
pub struct ACharBuf<'a, B: Buffer<'a>> {
    bytes: B,
    len:   usize,
    _ty:   PhantomData<&'a [u8]>,
}

impl<'a, B: Buffer<'a>> CharBuf<'a, B> for ACharBuf<'a, B> {
    const VALID_RANGES: &'static [RangeInclusive<u8>] = &[b'0'..=b'9', b'A'..=b'Z'];

    const VALID_CHARS: &'static [u8] = br#"!"%&'()*+,-./:;<=>?_ "#;

    type ValidationErr = ACharError;

    fn bytes(&self) -> &[u8] {
        self.bytes.as_bytes()
    }

    fn len(&self) -> usize {
        self.len
    }

    fn make_error(src: &[u8], offset: usize) -> Self::ValidationErr {
        ACharError {
            src:        std::str::from_utf8(src)
                .expect("not valid utf8")
                .to_string(),
            char_label: (offset, 1).into(),
        }
    }

    fn from_parts(bytes: B, len: usize) -> Self {
        Self {
            bytes,
            len,
            _ty: PhantomData,
        }
    }
}

impl<'a, B: Buffer<'a>> Encode for ACharBuf<'a, B> {
    fn encode<W: DiscWrite + ?Sized>(&self, writer: &mut W) -> Result<(), EncodeError> {
        self.encode_chars(writer)
    }
}

impl<'a, B: Buffer<'a>> ACharBuf<'a, B> {
    pub fn new(bytes: B, len: usize) -> Result<Self, ACharError> {
        Self::parse(bytes, len)
    }
}

pub trait Buffer<'a> {
    fn as_bytes(&self) -> &[u8];
}

impl Buffer<'static> for Vec<u8> {
    fn as_bytes(&self) -> &[u8] {
        self
    }
}
impl<'a> Buffer<'a> for &'a [u8] {
    fn as_bytes(&self) -> &[u8] {
        self
    }
}

impl<const N: usize> Buffer<'static> for [u8; N] {
    fn as_bytes(&self) -> &[u8] {
        self
    }
}

#[test]
#[should_panic]
#[cfg(test)]
fn invalid_dchar() {
    miette::set_panic_hook();
    DChar::new(b'a').into_diagnostic().unwrap();
}

#[test]
#[cfg(test)]
fn dchar_buffer() {
    let buffer = "HELLOWORLD_1".as_bytes();
    assert!(DCharBuf::new(buffer, buffer.len()).is_ok());
    let buffer = "HELLO_world_1".as_bytes();
    assert!(DCharBuf::new(buffer, buffer.len()).is_err());
    let buffer = "HELLO:".as_bytes();
    assert!(DCharBuf::new(buffer, buffer.len()).is_err())
}

#[test]
#[cfg(test)]
#[should_panic]
fn dchar_buffer_error() {
    let buffer = "HELLO_world_1".as_bytes();
    DCharBuf::new(buffer, buffer.len())
        .into_diagnostic()
        .unwrap();
}

#[test]
#[cfg(test)]
fn achar_buffer() -> miette::Result<()> {
    let buffer = "HELLOWORLD_1".as_bytes();
    ACharBuf::new(buffer, buffer.len())?;
    let buffer = "%&!* YOU WORLD!".as_bytes();
    ACharBuf::new(buffer, buffer.len())?;
    let buffer = "HELLO_world_1".as_bytes();
    assert!(ACharBuf::new(buffer, buffer.len()).is_err());
    Ok(())
}

#[test]
#[cfg(test)]
#[should_panic]
fn achar_buffer_error() {
    let buffer = "HELLO_world_1".as_bytes();
    ACharBuf::new(buffer, buffer.len())
        .into_diagnostic()
        .unwrap();
}
