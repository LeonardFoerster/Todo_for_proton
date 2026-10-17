// Release: kein zusätzliches Konsolenfenster (nur Windows relevant, schadet unter Linux nicht).
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    todo_desktop::run();
}
