// Die einzige Stelle, an der die Oberflaeche `invoke` sieht. Jede Funktion
// entspricht genau einem Befehl in `app/src-tauri/src/commands.rs`.

import { invoke } from "@tauri-apps/api/core";
import type {
  Action,
  AppInfo,
  Completion,
  Config,
  Dashboard,
  Signal,
  SyncReport,
} from "./types";

export const getConfig = (): Promise<Config> => invoke("get_config");

/** Leeres Ergebnis heisst: uebernommen. Sonst die Liste der Probleme. */
export const saveConfig = (config: Config): Promise<string[]> =>
  invoke("save_config", { config });

export const validateConfig = (config: Config): Promise<string[]> =>
  invoke("validate_config", { config });

export const getDashboard = (): Promise<Dashboard> => invoke("get_dashboard");

export const getSignals = (source: string): Promise<Signal[]> =>
  invoke("get_signals", { source });

export const syncNow = (force: boolean): Promise<SyncReport> =>
  invoke("sync_now", { force });

export const runAction = (action: Action): Promise<void> =>
  invoke("run_action", { action });

export const snoozeSignal = (key: string, hours: number): Promise<void> =>
  invoke("snooze_signal", { key, hours });

export const dismissSignal = (key: string): Promise<void> =>
  invoke("dismiss_signal", { key });

export const setSecret = (name: string, value: string): Promise<void> =>
  invoke("set_secret", { name, value });

export const listSecretNames = (): Promise<string[]> => invoke("list_secret_names");

export const setTimezoneOffset = (minutes: number): Promise<void> =>
  invoke("set_timezone_offset", { minutes });

export const beginGmailAuth = (instance: string): Promise<void> =>
  invoke("begin_gmail_auth", { instance });

export const finishGmailAuth = (redirectUrl: string): Promise<string> =>
  invoke("finish_gmail_auth", { redirectUrl });

export const dailyBrief = (): Promise<Completion> => invoke("daily_brief");

export const appInfo = (): Promise<AppInfo> => invoke("app_info");
