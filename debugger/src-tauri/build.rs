fn main() {
    tauri_build::build();
    emma6k_build_info::emit("emma6k-debugger-v");
}
