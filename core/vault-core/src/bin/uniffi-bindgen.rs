//! uniffi-bindgen 命令行入口（库模式：由已编译的 cdylib 生成各端绑定）。
//! 见 docs/technical/engineering-foundation.md §3.2；用法见 L0-05b 任务。
fn main() {
    uniffi::uniffi_bindgen_main()
}
