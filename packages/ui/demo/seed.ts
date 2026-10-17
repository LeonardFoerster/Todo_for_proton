import type { CoreClient } from "@tasks/core-client";
import { addDays, isoDate } from "../src/lib/local-day";

/** Realistische Beispieldaten für die Demo (Browser, FakeCoreClient). `bulk` = zusätzliche Tasks. */
export async function seed(core: CoreClient, bulk = 0): Promise<void> {
  await core.ensureInbox();
  const work = await core.createTaskList("Arbeit");
  const home = await core.createTaskList("Zuhause");
  await core.createTaskList("Reise Lissabon");
  const today = new Date();
  const day = (n: number) => ({ kind: "date" as const, date: isoDate(addDays(today, n)) });

  const add = async (
    title: string,
    opts: { list?: string; due?: number; prio?: "low" | "medium" | "high"; notes?: string; done?: boolean } = {},
  ) => {
    const t = await core.createTask({ title, listId: opts.list });
    if (opts.due !== undefined || opts.prio || opts.notes) {
      await core.updateTask(t.id, {
        due: opts.due !== undefined ? day(opts.due) : undefined,
        priority: opts.prio,
        notes: opts.notes,
      });
    }
    if (opts.done) await core.completeTask(t.id, true);
  };

  await add("Steuererklärung abgeben", { due: -2, prio: "high", notes: "Belege liegen im Ordner „2025“." });
  await add("Zahnarzt anrufen – Termin verschieben", { due: 0 });
  await add("Geschenk für Mara besorgen", { due: 3, prio: "medium" });
  await add("Fahrrad zur Inspektion bringen");
  await add("Milch, Brot, Kaffee", { done: true });
  await add("Quartalsbericht gegenlesen", { list: work.id, due: 0, prio: "high" });
  await add("PR #128 reviewen", { list: work.id, due: 1, notes: "Fokus: Migrationen" });
  await add("1:1 mit Jonas vorbereiten", { list: work.id, due: 2, prio: "low" });
  await add("Onboarding-Doku aktualisieren", { list: work.id });
  await add("Heizungsableser-Termin bestätigen", { list: home.id, due: 5 });
  await add("Balkonpflanzen umtopfen", { list: home.id, done: true });
  for (let i = 0; i < bulk; i++) await add(`Testaufgabe ${i + 1}`, { due: i % 9 === 0 ? i % 5 : undefined });
}
