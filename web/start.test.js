import assert from "node:assert/strict";
import { test } from "node:test";
import { createShutdownController } from "./start.js";

function fixture(close) {
  const calls = { exit: [], report: [] };
  const controller = createShutdownController({
    close,
    exit: (code) => calls.exit.push(code),
    report: (message) => calls.report.push(message),
  });
  return { controller, calls };
}

test("first SIGINT starts graceful shutdown; second SIGINT forces exit", async () => {
  let finishClose;
  const { controller, calls } = fixture(() => new Promise((resolve) => { finishClose = resolve; }));

  controller.handleSignal("SIGINT");
  await new Promise((resolve) => setImmediate(resolve));
  assert.equal(controller.state, "closing");
  assert.match(calls.report[0], /Press Ctrl\+C again/);
  controller.handleSignal("SIGINT");
  assert.deepEqual(calls.exit, [130]);

  finishClose();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls.exit, [130]);
});

test("a repeated SIGTERM does not force exit", async () => {
  let finishClose;
  const { controller, calls } = fixture(() => new Promise((resolve) => { finishClose = resolve; }));
  controller.handleSignal("SIGTERM");
  await new Promise((resolve) => setImmediate(resolve));
  controller.handleSignal("SIGTERM");
  assert.deepEqual(calls.exit, []);
  finishClose();
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls.exit, [0]);
});

test("synchronous close errors produce a non-zero exit", async () => {
  const { controller, calls } = fixture(() => { throw new Error("close failed"); });
  controller.handleSignal("SIGINT");
  await new Promise((resolve) => setImmediate(resolve));
  assert.deepEqual(calls.exit, [1]);
  assert.ok(calls.report.some((message) => message.includes("close failed")));
});
