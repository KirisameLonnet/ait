/** @vitest-environment jsdom */
import { act, cleanup, renderHook, waitFor } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";

const config = vi.hoisted(() => ({ changelog: "" as string | undefined }));
vi.mock("expo-constants", () => ({
  default: { expoConfig: { extra: config } },
}));

const bundled = "## 0.0.6 - 2026-09-27\n\n- Bundled Ait release notes.";
const newer = "## 0.0.7 - 2026-10-01\n\n- New release notes.";
const fetchChangelog = vi.fn<typeof fetch>();

beforeEach(() => {
  vi.resetModules();
  config.changelog = bundled;
  fetchChangelog.mockReset();
  vi.stubGlobal("fetch", fetchChangelog);
});

afterEach(() => {
  cleanup();
  vi.unstubAllGlobals();
});

describe("changelog availability", () => {
  it("shows bundled notes immediately while the network is pending", async () => {
    fetchChangelog.mockImplementation(() => new Promise(() => {}));
    const { useChangelog } = await import("./changelog-source");
    const { result } = renderHook(() => useChangelog(true));
    expect(result.current.state).toMatchObject({
      status: "ready",
      releases: [{ version: "0.0.6" }],
    });
  });

  it.each(["404", "offline", "invalid document"])(
    "keeps bundled notes readable on %s",
    async (failure) => {
      if (failure === "offline") fetchChangelog.mockRejectedValue(new TypeError("Failed to fetch"));
      else
        fetchChangelog.mockResolvedValue(
          new Response("Unavailable", { status: failure === "404" ? 404 : 200 }),
        );
      const { useChangelog } = await import("./changelog-source");
      const { result } = renderHook(() => useChangelog(true));
      await act(async () => {});
      expect(result.current.state).toMatchObject({
        status: "ready",
        releases: [{ version: "0.0.6" }],
      });
    },
  );

  it("loads newer notes and retains them if revalidation fails on reopening", async () => {
    fetchChangelog.mockResolvedValueOnce(new Response(newer));
    const { useChangelog } = await import("./changelog-source");
    const { result, rerender } = renderHook(({ open }) => useChangelog(open), {
      initialProps: { open: true },
    });
    await waitFor(() =>
      expect(result.current.state).toMatchObject({ releases: [{ version: "0.0.7" }] }),
    );
    rerender({ open: false });
    fetchChangelog.mockRejectedValueOnce(new TypeError("Offline"));
    rerender({ open: true });
    await act(async () => {});
    expect(fetchChangelog).toHaveBeenCalledTimes(2);
    expect(result.current.state).toMatchObject({
      status: "ready",
      releases: [{ version: "0.0.7" }],
    });
  });

  it("ignores a response that finishes after the sheet closes", async () => {
    let finish: (response: Response) => void = () => {};
    fetchChangelog.mockImplementationOnce(
      () =>
        new Promise((resolve) => {
          finish = resolve;
        }),
    );
    const { useChangelog } = await import("./changelog-source");
    const { result, rerender } = renderHook(({ open }) => useChangelog(open), {
      initialProps: { open: true },
    });
    rerender({ open: false });
    await act(async () => {
      finish(new Response(newer));
    });
    expect(result.current.state).toMatchObject({ releases: [{ version: "0.0.6" }] });
  });

  it("offers retry if an older build has no bundled notes", async () => {
    config.changelog = undefined;
    fetchChangelog.mockResolvedValueOnce(new Response("Not found", { status: 404 }));
    const { useChangelog } = await import("./changelog-source");
    const { result } = renderHook(() => useChangelog(true));
    await waitFor(() => expect(result.current.state).toEqual({ status: "error" }));
    fetchChangelog.mockResolvedValueOnce(new Response(newer));
    act(() => result.current.reload());
    await waitFor(() =>
      expect(result.current.state).toMatchObject({
        status: "ready",
        releases: [{ version: "0.0.7" }],
      }),
    );
  });
});
