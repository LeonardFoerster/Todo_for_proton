/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Dev-Selbsttest (src/selftest.ts): Suiten mit Komma oder `runtime` (aus `TASKS_SELFTEST`). */
  readonly VITE_TASKS_SELFTEST?: string;
}
