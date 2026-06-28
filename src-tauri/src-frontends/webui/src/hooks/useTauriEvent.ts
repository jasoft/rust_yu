import { useEffect, useRef } from "react";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

/**
 * 监听 Tauri 事件的 Hook。
 * 自动在组件卸载时取消监听。
 */
export function useTauriEvent<T>(
  eventName: string,
  handler: (payload: T) => void,
) {
  const handlerRef = useRef(handler);
  handlerRef.current = handler;

  useEffect(() => {
    let unlisten: UnlistenFn | undefined;

    listen<T>(eventName, (event) => {
      handlerRef.current(event.payload);
    }).then((fn) => {
      unlisten = fn;
    });

    return () => {
      unlisten?.();
    };
  }, [eventName]);
}
