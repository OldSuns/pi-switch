import { Worker } from "node:worker_threads";

export async function createNativeClient() {
  const worker = new Worker(new URL("./native-worker.js", import.meta.url));
  const pending = new Map();
  let nextId = 0;
  let failure;
  let readyResolve;
  let readyReject;
  const ready = new Promise((resolve, reject) => {
    readyResolve = resolve;
    readyReject = reject;
  });

  function fail(error) {
    failure = error;
    readyReject(error);
    for (const entry of pending.values()) entry.reject(error);
    pending.clear();
  }

  worker.on("error", fail);
  worker.on("exit", (code) => fail(new Error("The native Web worker stopped (exit " + code + ").")));
  worker.on("message", (message) => {
    if (message.ready) {
      readyResolve();
      return;
    }
    const entry = pending.get(message.id);
    if (!entry) return;
    pending.delete(message.id);
    if (message.error) entry.reject(new Error(message.error));
    else entry.resolve(message.result);
  });
  await ready;

  return {
    request(payload) {
      if (failure) return Promise.reject(failure);
      return new Promise((resolve, reject) => {
        const id = ++nextId;
        pending.set(id, { resolve, reject });
        worker.postMessage({ id, payload });
      });
    },
    async close() {
      fail(new Error("The Web server is shutting down."));
      await worker.terminate();
    },
  };
}
