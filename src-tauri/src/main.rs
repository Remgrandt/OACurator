// Copyright (c) 2026 Remgrandt Works. All rights reserved.

// Prevents additional console window on Windows in release, DO NOT REMOVE!!
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    oa_curator_lib::run()
}
