#![allow(dead_code)]
#![feature(const_destruct)]
#![feature(const_trait_impl)]
#![feature(const_array)]
#![feature(ascii_char)]

use snafu::Whatever;

pub mod encoders;
#[path = "./iso9660/iso9660.rs"]
pub mod iso9660;

pub type Result<T> = core::result::Result<T, Whatever>;

fn main() {
    println!("Hello, world!");
}
