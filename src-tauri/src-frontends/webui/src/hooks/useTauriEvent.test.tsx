// @vitest-environment jsdom

import { StrictMode } from "react";
import { act, render, waitFor } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useTauriEvent } from "./useTauriEvent";

const tauriEventMock = vi.hoisted(() => ({
  listeners: [] as Array<{
    active: boolean;
    handler: (event: { payload: string }) => void;
  }>,
  listen: vi.fn(),
}));

vi.mock("@tauri-apps/api/event", () => ({
  listen: tauriEventMock.listen,
}));

function Harness({ onEvent }: { onEvent: (payload: string) => void }) {
  useTauriEvent("fixture-event", onEvent);
  return null;
}

describe("useTauriEvent", () => {
  beforeEach(() => {
    tauriEventMock.listeners.length = 0;
    tauriEventMock.listen.mockReset();
    tauriEventMock.listen.mockImplementation(
      async (_eventName: string, handler: (event: { payload: string }) => void) => {
        const listener = { active: true, handler };
        tauriEventMock.listeners.push(listener);
        return () => {
          listener.active = false;
        };
      },
    );
  });

  it("keeps only one active listener during the StrictMode setup cycle", async () => {
    const onEvent = vi.fn();
    const view = render(
      <StrictMode>
        <Harness onEvent={onEvent} />
      </StrictMode>,
    );

    await waitFor(() => expect(tauriEventMock.listen).toHaveBeenCalledTimes(2));
    await waitFor(() => {
      expect(tauriEventMock.listeners.filter((listener) => listener.active)).toHaveLength(1);
    });

    act(() => {
      for (const listener of tauriEventMock.listeners.filter((item) => item.active)) {
        listener.handler({ payload: "once" });
      }
    });
    expect(onEvent).toHaveBeenCalledOnce();
    expect(onEvent).toHaveBeenCalledWith("once");

    view.unmount();
    expect(tauriEventMock.listeners.filter((listener) => listener.active)).toHaveLength(0);
  });
});
