#![windows_subsystem = "windows"]

fn main() -> anyhow::Result<()> {
    naobi_win32::dll_hardening::harden(); // T-025: first call
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == naobi_app::LIST_APPS_ARG)
    {
        return naobi_app::list_apps();
    }
    if std::env::args_os()
        .nth(1)
        .is_some_and(|a| a == naobi_app::SEARCH_FILES_ARG)
    {
        return naobi_app::search_files();
    }
    naobi_app::run()
}
