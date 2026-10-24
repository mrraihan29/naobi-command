#![windows_subsystem = "windows"]

fn main() -> anyhow::Result<()> {
    naobi_win32::dll_hardening::harden(); // T-025: first call
    naobi_app::run()
}
