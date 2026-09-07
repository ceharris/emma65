fn main() {
    tauri_build::build();
    emma65_build_info::emit("emma65-debugger-v");
}
