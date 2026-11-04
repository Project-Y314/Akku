// TypeScript mirror of `peripheral-deck-core` serde types (Deliverable 2).

export type CapabilityFlag =
  | "BATTERY"
  | "DPI"
  | "POLLING_RATE"
  | "LIGHTING"
  | "PROFILES"
  | "MACROS"
  | "LIFTOFF"
  | "DEBOUNCE"
  | "ACTUATION";

export interface CapabilitiesWire {
  bits: number;
  flags: CapabilityFlag[];
}

export interface DeviceId {
  backend_id: string;
  vendor_id: number;
  product_id: number;
  usage_page: number;
  usage: number;
  receiver_slot: number | null;
  serial: string | null;
}

export type DeviceKind = "mouse" | "keyboard" | "receiver" | "gamepad" | "other";
export type ConnectionKind = "usb_receiver" | "wired" | "bluetooth_le";

export type FieldStatus<T> =
  | { status: "available"; value: T }
  | { status: "unsupported" }
  | { status: "not_responding"; last_known: T | null; since_unix_ms: number }
  | { status: "sleeping"; last_known: T | null; since_unix_ms: number }
  | { status: "requires_vendor_service"; service_name: string; reason: string }
  | { status: "requires_elevation"; reason: string };

export interface BatteryState {
  percent: number;
  charging: boolean;
  voltage_mv: number | null;
}

export interface DpiState {
  x: number;
  y: number;
  active_stage: number | null;
}

export interface TrayBadgeModel {
  primary_device_name: string | null;
  battery_percent: number | null;
  charging: boolean;
  pill_label: string;
  tooltip: string;
  popover_open: boolean;
  animation_phase: number;
}
