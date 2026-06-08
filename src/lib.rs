#![allow(dead_code)]
#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(ascii_char)]
#![feature(portable_simd)]

pub mod encoders;
#[path = "./iso9660/iso9660.rs"]
pub mod iso9660;

pub type Result<T> = miette::Result<T>;
