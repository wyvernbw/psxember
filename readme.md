# `psxember`

Utility for writing disk images for the PlayStation 1.

## Project goals

- small binary (currently 84K with nightly, see [build.sh](./build.sh))
- 0 config by default
- pluggable (*eventually* `no_std` core with a cli implementation, see below)
- meant to work side by side with `cargo-psx`

blockers for `no_std` support:
- `core::io` 
- `miette`

## Todo

Based on the PSX CD-ROM format described in psx-spx:

- [x] mode-2/form-1 sectors 
- [x] mode-2/form-2 sectors
- [ ] mode-1
- [x] ISO Volume Descriptors
  - [x] PSX System area (license string and ps1 logo)
  - [x] primary volume descriptor
  - [x] volume descriptor set terminator
- [x] directory records
- [x] edc checksum
- [ ] ecc error correction codes
- [ ] path table
  - [ ] path table entries
- [ ] write files
- [ ] create directory records from file tree
- [ ] audio data (CDROM XA-ADPCM data)

Currently `psxember` can write a bootable PSX disc, but it will be empty since I
haven't added file writing.

## Dependencies

I try to keep dependencies very minimal, so I will include for every one the
motivation for adding it:
- `bitbybit` and `arbitrary-int`: these 2 go together, offers easy bit field access.
- `miette` and `thiserror`: explicit error handling and formatting for the end user
- `tracing`: logging, only for development, will be removed.
- `derive_more`: convenience derives. this will be removed later.

Dev dependencies can be anything I need for testing.
