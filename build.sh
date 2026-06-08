#!/bin/bash

cargo +nightly build -p psxember-cli --release -Z build-std=std,panic_abort -Z build-std-features=optimize_for_size
