//! Prism desktop entry point.

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    prism_lib::run();
}
