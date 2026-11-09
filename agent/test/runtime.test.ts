import { test } from "node:test";
import assert from "node:assert/strict";
import { retryDelayMs } from "../src/runtime.js";

test("failed jobs back off exponentially and cap at 15 minutes", () => {
  assert.equal(retryDelayMs(1), 30_000);
  assert.equal(retryDelayMs(2), 60_000);
  assert.equal(retryDelayMs(3), 120_000);
  assert.equal(retryDelayMs(20), 15 * 60_000);
});
