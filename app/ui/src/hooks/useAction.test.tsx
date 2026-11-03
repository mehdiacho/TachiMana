import { act, renderHook } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { useActivity } from "../store/activity";
import { useAction } from "./useAction";

afterEach(() => useActivity.setState({ running: 0, toasts: [] }));

function deferred<T>() {
  let resolve!: (v: T) => void;
  let reject!: (e: unknown) => void;
  const promise = new Promise<T>((res, rej) => {
    resolve = res;
    reject = rej;
  });
  return { promise, resolve, reject };
}

describe("useAction", () => {
  it("is pending from the click, runs once, and counts in the activity bar", async () => {
    const d = deferred<number>();
    let calls = 0;
    const { result } = renderHook(() =>
      useAction(() => {
        calls++;
        return d.promise;
      }),
    );
    let first!: Promise<number | undefined>;
    act(() => {
      first = result.current.run();
      void result.current.run(); // a double click
    });
    expect(result.current.pending).toBe(true);
    expect(calls).toBe(1);
    expect(useActivity.getState().running).toBe(1);
    await act(async () => {
      d.resolve(7);
      await first;
    });
    expect(result.current.pending).toBe(false);
    expect(useActivity.getState().running).toBe(0);
  });

  it("keeps the error for the caller and toasts only when asked", async () => {
    const { result } = renderHook(() => useAction(() => Promise.reject({ kind: "x", message: "Nope" })));
    await act(async () => {
      await result.current.run();
    });
    expect(result.current.error).toBe("Nope");
    expect(useActivity.getState().toasts).toHaveLength(0);

    const toasted = renderHook(() => useAction(() => Promise.reject({ kind: "x", message: "Nope" }), { toastError: true }));
    await act(async () => {
      await toasted.result.current.run();
    });
    expect(useActivity.getState().toasts.map((t) => t.text)).toEqual(["Nope"]);
  });

  it("toasts a success line", async () => {
    const { result } = renderHook(() => useAction(() => Promise.resolve(3), { ok: (n) => `Queued ${n}.` }));
    await act(async () => {
      await result.current.run();
    });
    expect(useActivity.getState().toasts[0]?.kind).toBe("ok");
    expect(useActivity.getState().toasts[0]?.text).toBe("Queued 3.");
  });
});

describe("toasts", () => {
  it("keep at most a few, newest last", () => {
    for (let i = 0; i < 6; i++) useActivity.getState().push("info", `t${i}`);
    expect(useActivity.getState().toasts.map((t) => t.text)).toEqual(["t2", "t3", "t4", "t5"]);
  });
});
