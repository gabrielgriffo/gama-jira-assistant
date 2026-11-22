use std::time::Duration;

use tauri::{AppHandle, Manager, WebviewWindow};
use tauri_plugin_window_state::{StateFlags, WindowExt};

pub const MAIN_WINDOW: &str = "main";
const SHOW_FALLBACK_DELAY: Duration = Duration::from_secs(5);

pub fn window_state_flags() -> StateFlags {
    StateFlags::all() - StateFlags::DECORATIONS - StateFlags::VISIBLE
}

pub fn prepare_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };

    // Janela sem decoração criada oculta mantém a altura da barra de título até o frame ser recalculado.
    #[cfg(not(target_os = "macos"))]
    {
        let _ = window.set_decorations(true);
        let _ = window.set_decorations(false);
    }
    if let Err(e) = window.restore_state(window_state_flags()) {
        log::warn!("falha ao restaurar estado da janela: {e}");
    }

    std::thread::spawn(move || {
        std::thread::sleep(SHOW_FALLBACK_DELAY);
        if !window.is_visible().unwrap_or(true) {
            log::warn!("frontend não exibiu a janela a tempo; exibindo pelo fallback");
            let _ = window.show();
        }
    });
}

pub fn focus_main_window(app: &AppHandle) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW) else {
        return;
    };
    if window.is_minimized().unwrap_or(false) {
        let _ = window.unminimize();
    }
    let _ = window.show();
    let _ = window.set_focus();
}

#[tauri::command]
pub fn show_main_window(window: WebviewWindow) -> Result<(), String> {
    window.show().map_err(|e| e.to_string())
}
