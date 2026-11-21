import { test } from "node:test";
import assert from "node:assert/strict";
import { minutesRegisteredElsewhere } from "./registered-elsewhere.ts";

test("conta o tempo confirmado e o não confirmado que já está no Jira", () => {
  assert.equal(minutesRegisteredElsewhere({ registeredMinutes: 240, unconfirmedMinutes: 30 }, []), 270);
});

test("desconta os worklogs enviados nesta prévia mesmo antes da confirmação", () => {
  const day = { registeredMinutes: 240, unconfirmedMinutes: 240 };
  const drafts = [
    { state: "sent", duration: 240 },
    { state: "pending", duration: 60 },
  ];
  assert.equal(minutesRegisteredElsewhere(day, drafts), 240);
});

test("nunca fica negativo", () => {
  assert.equal(
    minutesRegisteredElsewhere({ registeredMinutes: 0, unconfirmedMinutes: 0 }, [{ state: "sent", duration: 90 }]),
    0,
  );
});
