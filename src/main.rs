#![allow(dead_code)]
#![feature(const_destruct)]
#![feature(const_trait_impl)]
#![feature(const_array)]

pub mod encoders;
#[path = "./iso9660/iso9660.rs"]
pub mod iso9660;

fn main() {
    println!("Hello, world!");
}
