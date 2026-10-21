//! Mehrere Tasks abhaken oder wieder öffnen (Sammelaktion).

use std::collections::HashMap;

use chrono::NaiveDate;

use super::{
    bulk_ids, key_after, load_existing,
    recurring::{due_local, next_due, occurrence_id, occurrence_of, today_or},
    Deps,
};
use crate::{
    domain::{Due, SortKey, Task, TaskId, TaskListId},
    ports::{Change, ChangeSet},
    CoreResult,
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompleteTasksInput {
    pub ids: Vec<TaskId>,
    /// `true` = abhaken, `false` = wieder öffnen.
    pub done: bool,
    /// Lokales Datum des Nutzers für Serien (`None` = UTC-Datum der Uhr).
    pub today: Option<NaiveDate>,
}

/// Abgehaktes Vorkommen einer Serie (für „Rückgängig“ per [`super::UndoOccurrence`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompletedOccurrence {
    pub series: TaskId,
    /// Verlaufseintrag; `None`, wenn die Serie damit geendet hat (dann ist sie selbst erledigt).
    pub occurrence: Option<TaskId>,
    /// Fälligkeit der Serie vor dem Abhaken.
    pub previous_due: Due,
}

#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct CompleteTasksOutput {
    /// Tatsächlich geänderte Tasks (Reihenfolge wie `ids`; Verlaufseinträge dahinter).
    pub changed: Vec<Task>,
    pub occurrences: Vec<CompletedOccurrence>,
}

/// Hakt Tasks ab bzw. öffnet sie wieder – in **einem** Commit. Unbekannte IDs und Tasks, die
/// schon im Zielzustand sind, werden übersprungen (kein Op). Eine Serie (ADR-028) bleibt offen:
/// Sie bekommt einen erledigten Verlaufseintrag (deterministische ID) und ihr nächstes Vorkommen.
#[derive(Debug, Clone)]
pub struct CompleteTasks {
    deps: Deps,
}

impl CompleteTasks {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei mehr als [`super::BULK_LIMIT_MAX`] IDs oder einer Serie mit
    /// Uhrzeit ohne Zeitzonen; Fehler des Repositories (dann ist nichts geschrieben).
    pub async fn execute(&self, input: CompleteTasksInput) -> CoreResult<CompleteTasksOutput> {
        let ids = bulk_ids(input.ids)?;
        // Verlaufseinträge bekommen neue Sort-Keys (ADR-024).
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        let clock = self.deps.clock.as_ref();
        let zones = self.deps.zones.as_deref();
        let today = today_or(input.today, clock.now());
        let mut out = CompleteTasksOutput::default();
        let mut entries = Vec::new();
        let mut last_keys: HashMap<TaskListId, Option<SortKey>> = HashMap::new();
        for mut task in load_existing(repo, &ids).await? {
            if task.is_done() == input.done {
                continue;
            }
            if input.done && task.is_recurring() {
                let previous = task.due().cloned().expect("is_recurring has a due");
                let (day, _) = due_local(&previous, zones)?;
                let next = next_due(&task, today, zones)?;
                let mut occurrence = None;
                if let Some(next) = next {
                    let id = occurrence_id(repo, task.id(), day).await?;
                    if repo.get_task(id).await?.is_none()
                        && !entries.iter().any(|e: &Task| e.id() == id)
                    {
                        let last = match last_keys.get(&task.list_id()) {
                            Some(key) => key.clone(),
                            None => repo.last_sort_key(task.list_id()).await?,
                        };
                        let key = key_after(last.as_ref())?;
                        last_keys.insert(task.list_id(), Some(key.clone()));
                        entries.push(occurrence_of(&task, id, key, clock.now())?);
                    }
                    occurrence = Some(id);
                    task.set_due(Some(next), clock)?;
                } else {
                    task.complete(clock);
                }
                out.occurrences.push(CompletedOccurrence {
                    series: task.id(),
                    occurrence,
                    previous_due: previous,
                });
            } else if input.done {
                task.complete(clock);
            } else {
                task.reopen(clock);
            }
            out.changed.push(task);
        }
        out.changed.extend(entries);
        if !out.changed.is_empty() {
            let mut changes = ChangeSet::new();
            for task in &out.changed {
                changes.push(Change::UpsertTask(task.clone()));
            }
            repo.commit(changes).await?;
        }
        Ok(out)
    }
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{test_util::deps, CreateTask, CreateTaskInput, BULK_LIMIT_MAX},
        ports::TaskRepository,
        CoreError,
    };

    fn setup(
        n: usize,
    ) -> (
        CompleteTasks,
        std::sync::Arc<crate::testing::InMemoryTaskRepository>,
        Vec<Task>,
    ) {
        let (deps, repo) = deps();
        let create = CreateTask::new(deps.clone());
        let tasks = (0..n)
            .map(|i| {
                block_on(create.execute(CreateTaskInput {
                    title: format!("t{i}"),
                    list_id: None,
                    ..CreateTaskInput::default()
                }))
                .unwrap()
            })
            .collect();
        (CompleteTasks::new(deps), repo, tasks)
    }

    fn run(uc: &CompleteTasks, ids: Vec<TaskId>, done: bool) -> CoreResult<Vec<Task>> {
        block_on(uc.execute(CompleteTasksInput {
            ids,
            done,
            today: None,
        }))
        .map(|out| out.changed)
    }

    #[test]
    fn completes_and_reopens_and_skips_tasks_in_target_state() {
        let (uc, repo, tasks) = setup(3);
        let ids: Vec<TaskId> = tasks.iter().map(Task::id).collect();
        let first = run(&uc, vec![ids[0]], true).unwrap();
        assert_eq!(first.len(), 1);

        let done = run(&uc, ids.clone(), true).unwrap();
        assert_eq!(
            done.iter().map(Task::id).collect::<Vec<_>>(),
            [ids[1], ids[2]],
            "bereits erledigte übersprungen"
        );
        for id in &ids {
            assert!(block_on(repo.get_task(*id)).unwrap().unwrap().is_done());
        }
        let reopened = run(&uc, ids.clone(), false).unwrap();
        assert_eq!(reopened.len(), 3);
        assert!(reopened
            .iter()
            .all(|t| !t.is_done() && t.completed_at().is_none()));
    }

    #[test]
    fn unknown_duplicate_and_empty_ids() {
        let (uc, repo, tasks) = setup(2);
        let a = tasks[0].id();
        let changed = run(&uc, vec![TaskId::new(), a, a], true).unwrap();
        assert_eq!(changed.len(), 1, "unbekannt übersprungen, Duplikat einmal");
        assert_eq!(changed[0].id(), a);

        let before = block_on(repo.pending_op_count()).unwrap();
        assert!(run(&uc, Vec::new(), true).unwrap().is_empty());
        assert!(
            run(&uc, vec![a], true).unwrap().is_empty(),
            "schon erledigt"
        );
        assert_eq!(
            block_on(repo.pending_op_count()).unwrap(),
            before,
            "kein Commit"
        );
    }

    fn day(s: &str) -> NaiveDate {
        s.parse().unwrap()
    }

    fn series(deps: &Deps, due: Due, rule: crate::domain::Recurrence) -> Task {
        block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "Gießen".into(),
            due: Some(due),
            recurrence: Some(rule),
            ..CreateTaskInput::default()
        }))
        .unwrap()
    }

    fn complete_on(uc: &CompleteTasks, id: TaskId, today: &str) -> CoreResult<CompleteTasksOutput> {
        block_on(uc.execute(CompleteTasksInput {
            ids: vec![id],
            done: true,
            today: Some(day(today)),
        }))
    }

    #[test]
    fn series_stay_open_with_a_history_entry_and_skip_missed_occurrences() {
        use crate::{
            app::{SkipOccurrence, SkipOccurrenceInput, UndoOccurrence, UndoOccurrenceInput},
            domain::{Frequency, Recurrence},
        };
        let (deps, repo) = deps();
        let task = series(
            &deps,
            Due::Date(day("2026-10-15")),
            Recurrence::every(Frequency::Daily),
        );
        let uc = CompleteTasks::new(deps.clone());

        let out = complete_on(&uc, task.id(), "2026-10-21").unwrap();
        let series_now = block_on(repo.get_task(task.id())).unwrap().unwrap();
        assert!(!series_now.is_done(), "Serie bleibt offen");
        assert_eq!(
            series_now.due(),
            Some(&Due::Date(day("2026-10-21"))),
            "verpasste übersprungen"
        );
        let [occ] = out.occurrences.as_slice() else {
            panic!("ein Vorkommen")
        };
        assert_eq!(occ.previous_due, Due::Date(day("2026-10-15")));
        let entry_id = occ.occurrence.unwrap();
        let entry = block_on(repo.get_task(entry_id)).unwrap().unwrap();
        assert!(entry.is_done() && !entry.is_recurring());
        assert_eq!(entry.due(), Some(&Due::Date(day("2026-10-15"))));
        assert_eq!(out.changed.len(), 2);

        // Rückgängig: Eintrag weg, alte Fälligkeit; erneutes Abhaken nimmt eine neue ID.
        let undone = block_on(
            UndoOccurrence::new(deps.clone()).execute(UndoOccurrenceInput {
                series: task.id(),
                occurrence: Some(entry_id),
                previous_due: Some(occ.previous_due.clone()),
            }),
        )
        .unwrap();
        assert_eq!(undone.due(), Some(&Due::Date(day("2026-10-15"))));
        assert!(block_on(repo.get_task(entry_id)).unwrap().is_none());
        let again = complete_on(&uc, task.id(), "2026-10-21").unwrap();
        let again_id = again.occurrences[0].occurrence.unwrap();
        assert_ne!(again_id, entry_id, "Tombstone wird nicht wiederbelebt");
        assert!(block_on(repo.get_task(again_id))
            .unwrap()
            .unwrap()
            .is_done());

        // Überspringen: nur fortschreiben, kein Eintrag.
        let before = block_on(repo.list_tasks(
            &crate::ports::TaskQuery::view(crate::ports::SmartView::All),
            crate::ports::PageRequest::first(100),
        ))
        .unwrap()
        .items
        .len();
        let skipped = block_on(SkipOccurrence::new(deps).execute(SkipOccurrenceInput {
            id: task.id(),
            today: Some(day("2026-10-21")),
        }))
        .unwrap();
        assert_eq!(skipped.due(), Some(&Due::Date(day("2026-10-22"))));
        let after = block_on(repo.list_tasks(
            &crate::ports::TaskQuery::view(crate::ports::SmartView::All),
            crate::ports::PageRequest::first(100),
        ))
        .unwrap()
        .items
        .len();
        assert_eq!(before, after);
    }

    #[test]
    fn until_ends_the_series_and_times_need_zones() {
        use crate::{
            domain::{Frequency, Recurrence, RecurrenceAnchor, Timestamp, WeekdaySet},
            ports::FixedOffsetZones,
        };
        let (deps, repo) = deps();
        let rule = Recurrence::new(
            Frequency::Weekly,
            1,
            WeekdaySet::default(),
            None,
            RecurrenceAnchor::Schedule,
            Some(day("2026-10-25")),
        )
        .unwrap();
        let last = series(&deps, Due::Date(day("2026-10-21")), rule);
        let uc = CompleteTasks::new(deps.clone());
        let out = complete_on(&uc, last.id(), "2026-10-21").unwrap();
        assert_eq!(out.occurrences[0].occurrence, None);
        assert!(
            block_on(repo.get_task(last.id()))
                .unwrap()
                .unwrap()
                .is_done(),
            "Serie endet"
        );

        // 2026-10-21 12:00 UTC = 14:00 bei +120 min; nächste Woche gleiche lokale Uhrzeit.
        let at = Timestamp::from_millis(1_792_584_000_000);
        let timed = Due::DateTime {
            at,
            tz: "Europe/Berlin".into(),
        };
        let zoned = deps
            .clone()
            .with_zones(std::sync::Arc::new(FixedOffsetZones {
                offset_minutes: 120,
            }));
        let task = series(&zoned, timed.clone(), Recurrence::every(Frequency::Weekly));
        assert!(
            matches!(
                complete_on(&uc, task.id(), "2026-10-21"),
                Err(CoreError::Validation(_))
            ),
            "ohne Zeitzonen"
        );
        let out = complete_on(&CompleteTasks::new(zoned), task.id(), "2026-10-21").unwrap();
        let next = block_on(repo.get_task(task.id())).unwrap().unwrap();
        assert_eq!(
            next.due(),
            Some(&Due::DateTime {
                at: Timestamp::from_millis(at.as_millis() + 7 * 86_400_000),
                tz: "Europe/Berlin".into()
            })
        );
        assert!(out.occurrences[0].occurrence.is_some());
    }

    /// Europe/Berlin 2026 vereinfacht: Sommerzeit vom 29.03. 01:00 UTC bis 25.10. 01:00 UTC.
    #[derive(Debug)]
    struct Berlin2026;

    impl Berlin2026 {
        const SUMMER: (i64, i64) = (1_774_746_000_000, 1_792_890_000_000);
    }

    impl crate::ports::TimeZones for Berlin2026 {
        fn to_local(&self, at: crate::domain::Timestamp, _: &str) -> Option<chrono::NaiveDateTime> {
            let ms = at.as_millis();
            let off = if (Self::SUMMER.0..Self::SUMMER.1).contains(&ms) {
                2
            } else {
                1
            };
            Some(chrono::DateTime::from_timestamp_millis(ms + off * 3_600_000)?.naive_utc())
        }

        fn to_utc(
            &self,
            local: chrono::NaiveDateTime,
            _: &str,
        ) -> Option<crate::domain::Timestamp> {
            let naive = local.and_utc().timestamp_millis();
            // Erst Sommerzeit versuchen (doppelte Stunde → früher), sonst Winterzeit.
            [2, 1].into_iter().find_map(|off| {
                let ms = naive - off * 3_600_000;
                let summer = (Self::SUMMER.0..Self::SUMMER.1).contains(&ms);
                (summer == (off == 2)).then_some(crate::domain::Timestamp::from_millis(ms))
            })
        }
    }

    #[test]
    fn timed_series_keep_the_local_time_across_dst_and_bulk_mixes() {
        use crate::domain::{Frequency, Recurrence, Timestamp};
        let (deps, repo) = deps();
        let deps = deps.with_zones(std::sync::Arc::new(Berlin2026));
        // Sa 24.10. 14:00 Sommerzeit = 12:00 UTC → So 25.10. 14:00 Winterzeit = 13:00 UTC.
        let at = Timestamp::from_millis(1_792_843_200_000);
        let task = series(
            &deps,
            Due::DateTime {
                at,
                tz: "Europe/Berlin".into(),
            },
            Recurrence::every(Frequency::Daily),
        );
        let plain = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "einmalig".into(),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        let uc = CompleteTasks::new(deps);
        let out = block_on(uc.execute(CompleteTasksInput {
            ids: vec![task.id(), plain.id()],
            done: true,
            today: Some(day("2026-10-24")),
        }))
        .unwrap();
        assert_eq!(out.changed.len(), 3, "Serie, normale Task, Verlaufseintrag");
        assert_eq!(out.occurrences.len(), 1);
        assert!(block_on(repo.get_task(plain.id()))
            .unwrap()
            .unwrap()
            .is_done());
        let next = block_on(repo.get_task(task.id())).unwrap().unwrap();
        assert_eq!(
            next.due(),
            Some(&Due::DateTime {
                at: Timestamp::from_millis(at.as_millis() + 25 * 3_600_000),
                tz: "Europe/Berlin".into()
            }),
            "14:00 Ortszeit nach der Umstellung"
        );

        // Verlaufseintrag wieder öffnen: normale offene Task, Serie unberührt.
        let entry = out.occurrences[0].occurrence.unwrap();
        let reopened = run(&uc, vec![entry], false).unwrap();
        assert!(!reopened[0].is_done() && !reopened[0].is_recurring());
        assert_eq!(block_on(repo.get_task(task.id())).unwrap().unwrap(), next);
    }

    #[test]
    fn too_many_ids_is_validation() {
        let (uc, _, _) = setup(0);
        let ids = (0..=BULK_LIMIT_MAX).map(|_| TaskId::new()).collect();
        assert!(matches!(run(&uc, ids, true), Err(CoreError::Validation(_))));
    }
}
