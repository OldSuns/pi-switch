import { parentPort } from "node:worker_threads";
import binding from "../pi-switch-native.cjs";

if (typeof binding.WebSession !== "function") {
  throw new Error("The native module has no Web interface. Run npm run build:native:debug first.");
}
const session = new binding.WebSession();

parentPort.on("message", ({ id, payload }) => {
  try {
    const result = JSON.parse(session.request(JSON.stringify(payload)));
    parentPort.postMessage({ id, result });
  } catch (error) {
    parentPort.postMessage({ id, error: error.message });
  }
});
parentPort.postMessage({ ready: true });
