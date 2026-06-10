#![feature(explicit_tail_calls)]

use std::collections::VecDeque;
use std::fs::{self, DirEntry, ReadDir};
use std::io::Read;
use std::path::Path;

use miette::{IntoDiagnostic, miette};
use psxember_core::encoders::Encode;
use psxember_core::iso9660::DiscWrite;
use psxember_core::iso9660::fs::{
    DirectoryRecord, DirectoryRecordBuilder, FileFlags, Filename, SystemUse, Timestamp,
};

pub struct WalkDir {
    current:    ReadDir,
    iter_queue: VecDeque<ReadDir>,
}

impl Iterator for WalkDir {
    type Item = miette::Result<DirEntry>;

    fn next(&mut self) -> Option<Self::Item> {
        match self.current.next() {
            Some(dirent) => {
                let dirent = match dirent.into_diagnostic() {
                    Ok(dirent) => dirent,
                    Err(report) => return Some(Err(report)),
                };
                let file_type = match dirent.file_type().into_diagnostic() {
                    Ok(file_type) => file_type,
                    Err(report) => return Some(Err(report)),
                };
                if file_type.is_dir() {
                    let file_path = dirent.path();
                    let readdir = match fs::read_dir(file_path).into_diagnostic() {
                        Ok(readdir) => readdir,
                        Err(report) => return Some(Err(report)),
                    };
                    self.iter_queue.push_back(readdir);
                }
                Some(Ok(dirent))
            }
            None => match self.iter_queue.pop_front() {
                Some(iter) => {
                    self.current = iter;
                    become self.next()
                }
                None => None,
            },
        }
    }
}

impl WalkDir {
    pub fn new(path: &Path) -> miette::Result<Self> {
        Ok(Self {
            current:    fs::read_dir(path).into_diagnostic()?,
            iter_queue: VecDeque::new(),
        })
    }
}

pub fn write_iso<W: DiscWrite>(path: &Path, writer: &mut W) -> miette::Result<()> {
    let walkdir = WalkDir::new(path)?;
    let results = walkdir
        .map(|dirent| {
            let dirent = dirent?;

            let file_type = dirent.file_type().into_diagnostic()?;
            let file_metadata = dirent.metadata().into_diagnostic()?;
            let lba = writer.lba()?;

            let file_name = dirent.file_name();
            let file_name = file_name.to_str().ok_or_else(|| {
                miette!(
                    "filename is not valid utf8: {}",
                    dirent.file_name().to_string_lossy()
                )
            })?;

            if file_type.is_file() {
                let file_path = dirent.path();
                let mut file = fs::File::open(file_path).into_diagnostic()?;
                let file_size = file_metadata.len();
                let mut buf = Vec::with_capacity(file_size as usize);
                file.read_to_end(&mut buf).into_diagnostic()?;
                if buf.len() as u64 != file_size {
                    return Err(miette!(
                        "file was truncated, expected {} bytes, read {}",
                        file_size,
                        buf.len()
                    ));
                }
                let record = DirectoryRecord::new(DirectoryRecordBuilder {
                    filename:   Filename::from_ascii_str(file_name)?,
                    system_use: SystemUse::default(),
                    data_lba:   [*lba as u32; 2],
                    data_size:  [file_size as u32; 2],
                    timestamp:  Timestamp::now(),
                    flags:      FileFlags::File,
                });
                record.encode(writer)?;
                buf.as_mut_slice().encode(writer)?;
                Ok(())
            } else {
                // Ok(())
                todo!()
            }
        })
        .collect::<Vec<_>>();
    Ok(())
}
