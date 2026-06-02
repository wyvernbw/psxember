#![feature(const_convert)]
#![feature(const_destruct)]
#![feature(const_trait_impl)]

use std::io::{self, Write};

pub mod iso9660;

pub trait Encode {
    fn encode<W: Write>(&self, writer: &mut W) -> io::Result<()>;
}

fn main() {
    println!("Hello, world!");
}
