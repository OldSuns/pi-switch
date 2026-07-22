export interface DoctorCheck {
  ok: boolean;
  label: string;
  detail: string;
}

export function version(): string;
export function doctor(): DoctorCheck[];
export function runTui(): void;
