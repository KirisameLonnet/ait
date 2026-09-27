import { describe, expect, it } from "vitest";
import { WSOutboundMessageSchema } from "./messages";
import { validateWSOutboundMessage } from "./validation/ws-outbound";

function response(count?: number) {
  return {
    type: "session",
    message: {
      type: "agent.timeline.search.response",
      payload: {
        requestId: "search",
        agentId: "agent",
        epoch: "epoch",
        locations: [{ seq: 1, role: "assistant", ...(count === undefined ? {} : { count }) }],
        nextCursor: null,
        error: null,
      },
    },
  };
}

describe.each([
  ["Zod", (input: unknown) => WSOutboundMessageSchema.safeParse(input)],
  ["generated validator", validateWSOutboundMessage],
] as const)("timeline search %s", (_name, parse) => {
  it("preserves occurrence estimates and accepts responses from older hosts", () => {
    for (const input of [response(3), response()]) {
      expect(parse(input)).toMatchObject({ success: true, data: input });
    }
  });

  it.each([0, -1, 1.5, Number.NaN, Number.POSITIVE_INFINITY])(
    "rejects invalid occurrence count %s",
    (count) => {
      expect(parse(response(count)).success).toBe(false);
    },
  );
});
