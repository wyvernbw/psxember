#!/bin/bash

cargo +nightly build --release -Z build-std=std,panic_abort -Z build-std-features=optimize_for_size
