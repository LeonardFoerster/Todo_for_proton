/// <reference types="vite/client" />

interface ImportMetaEnv {
  /** Dev-Selbsttest (src/selftest.ts) aktivieren. */
  readonly VITE_TASKS_SELFTEST?: string;
}
