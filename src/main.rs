#![allow(dead_code)]
#![feature(const_destruct)]
#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(ascii_char)]
#![feature(portable_simd)]

pub mod encoders;
#[path = "./iso9660/iso9660.rs"]
pub mod iso9660;

pub type Result<T> = miette::Result<T>;

fn main() {
    println!("Hello, world!");
    // {
    //     use crate::encoders::DCharBuf;
    //     std::hint::black_box(DCharBuf::new(std::hint::black_box("TEST").as_bytes()).unwrap());
    // }
}
