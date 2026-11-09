import { test } from "node:test";
import assert from "node:assert/strict";
import { formatResult, MAX_RESULT_BYTES, mockAnswer, scriptedAnswer, truncateUtf8 } from "../src/llm.js";

test("truncateUtf8 never splits a multi-byte character", () => {
  const s = "ü".repeat(1000); // 2 bytes each
  const out = truncateUtf8(s, 1001);
  assert.equal(Buffer.byteLength(out, "utf8"), 1000);
  assert.ok(!out.includes("�"));
});

test("formatResult stays within the contract's 1500-byte limit", () => {
  const long = { text: "🤖".repeat(2000), model: "Qwen/Qwen3.6-35B-A3B-FP8", requestId: "chatcmpl-123" };
  const out = formatResult(long);
  assert.ok(Buffer.byteLength(out, "utf8") <= MAX_RESULT_BYTES);
  assert.ok(out.endsWith("chatcmpl-123"), "footer naming the model and request id is kept");
});

test("mock answers are clearly labelled", () => {
  const out = formatResult(mockAnswer({ name: "Haiku Bot", skill: "x" }, "write a haiku"));
  assert.match(out, /mock model/);
  assert.match(out, /Haiku Bot/);
});

test("scripted answers are delivered verbatim and labelled as scripted", () => {
  const out = formatResult(scriptedAnswer("line one\nline two"));
  assert.ok(out.startsWith("line one\nline two"));
  assert.match(out, /scripted demo answer, no AI model called/);
  assert.doesNotMatch(out, /NEAR AI Cloud/);
});
