import { describe, expect, it } from "vitest";
import {
  AgentTimelineSearchResponseMessageSchema,
  parseServerInfoStatusPayload,
} from "@getpaseo/protocol/messages";
import { METHODS } from "./methods";
import { eventMessage, responseMessage, serverInfo } from "./messages";
import { object } from "./types";

function info(methods: string[]) {
  const message = serverInfo({ server_id: "ait" }, new Set(methods));
  return parseServerInfoStatusPayload(object(object(message.message).payload));
}

it("preserves Rust timeline search counts in the SDK response envelope", () => {
  const result = {
    agentId: "agent",
    epoch: "epoch",
    locations: [{ seq: 1, role: "assistant", count: 3 }],
    nextCursor: null,
    error: null,
  };
  const envelope = responseMessage("agent.timeline.search.response", "search", result, {});
  expect(AgentTimelineSearchResponseMessageSchema.parse(envelope.message).payload).toEqual({
    ...result,
    requestId: "search",
  });
});

describe("Ait host capabilities", () => {
  it("exposes working settings and directory features without unsupported transports", () => {
    const value = info(Object.values(METHODS).map((method) => method.method));
    expect(value?.features).toMatchObject({
      projectList: true,
      projectAdd: true,
      stableProjectIdentity: true,
      workspaceMultiplicity: true,
      projectCreateDirectory: true,
      projectGithubClone: true,
      workspaceGithubRepositorySearch: true,
      agentProfiles: true,
      providerRemoval: true,
      projectCustomIcon: true,
      importSessionWorkspaceTarget: true,
      importSessionSearch: true,
      directorySubscriptions: false,
      daemonPairing: false,
    });
    expect(value?.features?.plugins).not.toBe(true);
    expect(value?.sessionEventTypes).toEqual([
      "status.server_info",
      "status.daemon_config_changed",
      "providers_snapshot_update",
      "agent_attention_required",
    ]);
  });

  it("does not enable controls without all their required methods", () => {
    expect(info(["project.list.request", "daemon.config.get.request"])?.features).toMatchObject({
      projectList: true,
      projectAdd: false,
      stableProjectIdentity: false,
      agentProfiles: false,
      providerRemoval: false,
      projectCustomIcon: false,
      importSessionWorkspaceTarget: false,
      importSessionSearch: false,
    });
  });
});

describe("Rust provider snapshots", () => {
  const entries = [
    {
      provider: "codex",
      status: "ready",
      enabled: true,
      fetchedAt: "2026-09-27T00:00:00Z",
      models: [{ provider: "codex", id: "model-1", label: "Model 1", thinkingOptions: [] }],
    },
  ];
  it("supplies a cacheable compact body for a full hashed response", () => {
    const message = responseMessage(
      "get_providers_snapshot_response",
      "catalog",
      { entries, snapshotHash: "hash", notModified: false },
      {},
    );
    expect(object(message.message).payload).toMatchObject({
      requestId: "catalog",
      snapshotHash: "hash",
      compactSnapshot: { entries: [{ provider: "codex", models: [{ id: "model-1" }] }] },
      fetchedAt: { codex: "2026-09-27T00:00:00Z" },
    });
  });
  it("normalizes pushed catalogs as full snapshots too", () => {
    const message = eventMessage("providers_snapshot_update", {
      entries,
      snapshotHash: "hash",
      subscriptionId: "feed",
    });
    expect(object(message.message).payload).toMatchObject({
      subscriptionId: "feed",
      compactSnapshot: { entries: [{ provider: "codex" }] },
    });
  });
  it("does not replace a not-modified reply with an empty catalog", () => {
    const message = responseMessage(
      "get_providers_snapshot_response",
      "cached",
      { entries: [], snapshotHash: "hash", notModified: true },
      {},
    );
    expect(object(message.message).payload).not.toHaveProperty("compactSnapshot");
  });
  it("keeps a genuinely empty full catalog cacheable", () => {
    const message = responseMessage(
      "get_providers_snapshot_response",
      "empty",
      { entries: [], snapshotHash: "empty", notModified: false },
      {},
    );
    expect(object(message.message).payload).toMatchObject({
      compactSnapshot: { entries: [], thinkingSets: [] },
    });
  });
});
