import type {
  FormPreferences,
  ProviderPreferences,
} from "../../../src/create-agent-preferences/preferences";
import { findAitDaemon } from "./ait-daemon";

export const TEST_HOST_LABEL = "localhost";

export const TEST_PROVIDER_PREFERENCES = {
  claude: { model: "haiku" },
  codex: { model: "gpt-5.4-mini", thinkingByModel: { "gpt-5.4-mini": "low" } },
} satisfies Record<string, ProviderPreferences>;

export function buildDirectTcpConnection(endpoint: string): {
  id: string;
  type: "directTcp";
  endpoint: string;
  password?: string;
} {
  const connection = findAitDaemon(Number(new URL(`http://${endpoint}`).port));
  return {
    id: `direct:${endpoint}`,
    type: "directTcp",
    endpoint,
    ...(connection ? { password: connection.token } : {}),
  };
}

export function buildSeededHost(input: {
  serverId: string;
  endpoint: string;
  label?: string;
  nowIso: string;
}) {
  const connection = buildDirectTcpConnection(input.endpoint);
  return {
    serverId:
      findAitDaemon(Number(new URL(`http://${input.endpoint}`).port))?.serverId ?? input.serverId,
    label: input.label ?? TEST_HOST_LABEL,
    connections: [connection],
    preferredConnectionId: connection.id,
    createdAt: input.nowIso,
    updatedAt: input.nowIso,
  };
}

export const TEST_MOCK_PROVIDER_PREFERENCES = {
  ...TEST_PROVIDER_PREFERENCES,
  mock: { model: "ten-second-stream" },
} satisfies Record<string, ProviderPreferences>;

export function buildCreateAgentPreferences() {
  return {
    provider: "codex",
    providerPreferences:
      process.env.E2E_REAL_PROVIDERS === "1"
        ? TEST_PROVIDER_PREFERENCES
        : { codex: { model: "offline-model" } },
  } satisfies FormPreferences;
}
