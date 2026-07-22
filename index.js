import binding from "./pi-switch-native.cjs";

export const version = binding.version;
export const doctor = () => JSON.parse(binding.doctorJson());
export const runTui = binding.runTui;
