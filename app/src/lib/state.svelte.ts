// Der Zustand der App. Ein Ort, damit Reiter und Einstellungen dieselben Daten
// sehen und ein Sync alles auf einmal auffrischt.

import * as api from "./api";
import type { Config, Dashboard, SyncReport } from "./types";

class AppState {
  dashboard = $state<Dashboard | null>(null);
  config = $state<Config | null>(null);
  secretNames = $state<string[]>([]);
  appInfo = $state<{ version: string; platform: string; data_dir: string } | null>(null);

  laedt = $state(false);
  synct = $state(false);
  fehler = $state<string | null>(null);
  letzterSync = $state<SyncReport | null>(null);

  /** Erster Aufbau: Zeitzone melden, dann laden und einmal synchronisieren. */
  async start() {
    this.laedt = true;
    try {
      // `getTimezoneOffset` zaehlt andersherum als der Kern.
      await api.setTimezoneOffset(-new Date().getTimezoneOffset());
      await this.reload();
      this.appInfo = await api.appInfo();
      // Nur was faellig ist — ein Kaltstart soll nicht jede Quelle anfassen.
      await this.sync(false);
    } catch (e) {
      this.fehler = String(e);
    } finally {
      this.laedt = false;
    }
  }

  async reload() {
    const [dashboard, config, secrets] = await Promise.all([
      api.getDashboard(),
      api.getConfig(),
      api.listSecretNames(),
    ]);
    this.dashboard = dashboard;
    this.config = config;
    this.secretNames = secrets;
  }

  async sync(force: boolean) {
    if (this.synct) return;
    this.synct = true;
    this.fehler = null;
    try {
      this.letzterSync = await api.syncNow(force);
      this.dashboard = await api.getDashboard();
    } catch (e) {
      this.fehler = String(e);
    } finally {
      this.synct = false;
    }
  }

  /** Speichert und gibt die Probleme zurueck; leer heisst uebernommen. */
  async saveConfig(config: Config): Promise<string[]> {
    const probleme = await api.saveConfig(config);
    if (probleme.length === 0) await this.reload();
    return probleme;
  }

  async setSecret(name: string, value: string) {
    await api.setSecret(name, value);
    this.secretNames = await api.listSecretNames();
  }

  hatSecret = (name: string | undefined): boolean =>
    !!name && this.secretNames.includes(name);
}

export const app = new AppState();
