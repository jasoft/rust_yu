use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager, RunEvent, Runtime, Window, WindowEvent};

const MAIN_WINDOW_LABEL: &str = "main";
const MINIMIZED_STATE_FILE_NAME: &str = ".window-minimized-state.json";

#[derive(Debug, Default, Deserialize, PartialEq, Serialize)]
struct MinimizedState {
    minimized: bool,
}

pub fn plugin<R: Runtime>() -> tauri::plugin::TauriPlugin<R> {
    tauri::plugin::Builder::new("window-state-minimized")
        .on_window_ready(|window| {
            if window.label() != MAIN_WINDOW_LABEL {
                return;
            }

            let state_path = match state_path(window.app_handle()) {
                Ok(path) => path,
                Err(error) => {
                    tracing::warn!(%error, "无法确定窗口最小化状态文件路径");
                    return;
                }
            };

            let event_window = window.clone();
            let event_state_path = state_path.clone();
            window.on_window_event(move |event| {
                if should_persist(event) {
                    persist_minimized_state(&event_state_path, &event_window);
                }
            });

            if load_minimized_state(&state_path) {
                if let Err(error) = window.minimize() {
                    tracing::warn!(%error, "恢复窗口最小化状态失败");
                }
            }
        })
        .on_event(|app, event| {
            if matches!(event, RunEvent::Exit | RunEvent::ExitRequested { .. }) {
                persist_for_app(app);
            }
        })
        .build()
}

fn should_persist(event: &WindowEvent) -> bool {
    matches!(
        event,
        WindowEvent::Resized(_)
            | WindowEvent::Moved(_)
            | WindowEvent::Focused(_)
            | WindowEvent::CloseRequested { .. }
    )
}

fn persist_for_app<R: Runtime>(app: &AppHandle<R>) {
    let Some(window) = app.get_webview_window(MAIN_WINDOW_LABEL) else {
        return;
    };
    let state_path = match state_path(app) {
        Ok(path) => path,
        Err(error) => {
            tracing::warn!(%error, "无法确定窗口最小化状态文件路径");
            return;
        }
    };
    persist_minimized_state(&state_path, &window.as_ref().window());
}

fn state_path<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<PathBuf> {
    Ok(app.path().app_config_dir()?.join(MINIMIZED_STATE_FILE_NAME))
}

fn load_minimized_state(path: &Path) -> bool {
    match fs::read_to_string(path) {
        Ok(contents) => match serde_json::from_str::<MinimizedState>(&contents) {
            Ok(state) => state.minimized,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "窗口最小化状态文件格式无效，将使用正常状态启动");
                false
            }
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => false,
        Err(error) => {
            tracing::warn!(path = %path.display(), %error, "读取窗口最小化状态失败，将使用正常状态启动");
            false
        }
    }
}

fn persist_minimized_state<R: Runtime>(path: &Path, window: &Window<R>) {
    let minimized = match window.is_minimized() {
        Ok(value) => value,
        Err(error) => {
            tracing::warn!(%error, "读取窗口最小化状态失败");
            return;
        }
    };
    let state = MinimizedState { minimized };
    let contents = match serde_json::to_vec_pretty(&state) {
        Ok(contents) => contents,
        Err(error) => {
            tracing::warn!(%error, "序列化窗口最小化状态失败");
            return;
        }
    };
    let Some(parent) = path.parent() else {
        tracing::warn!(path = %path.display(), "窗口最小化状态文件没有父目录");
        return;
    };
    if let Err(error) = fs::create_dir_all(parent) {
        tracing::warn!(path = %parent.display(), %error, "创建窗口状态目录失败");
        return;
    }
    if let Err(error) = fs::write(path, contents) {
        tracing::warn!(path = %path.display(), %error, "保存窗口最小化状态失败");
    }
}

#[cfg(test)]
mod tests {
    use super::MinimizedState;

    #[test]
    fn minimized_state_round_trips() {
        let state = MinimizedState { minimized: true };
        let json = serde_json::to_string(&state).expect("状态应可序列化");
        let restored: MinimizedState = serde_json::from_str(&json).expect("状态应可反序列化");
        assert_eq!(restored, state);
    }

    #[test]
    fn default_state_is_not_minimized() {
        assert!(!MinimizedState::default().minimized);
    }
}
