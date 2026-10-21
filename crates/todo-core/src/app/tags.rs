//! Tags (N8-02, ADR-035): anlegen über den Namen (abgeleitete ID), umbenennen, löschen.

use super::{collect_all, Deps};
use crate::{
    domain::{normalize_tag_name, tag_key, Tag, TagId},
    ports::{Change, ChangeSet, EntityRef, SmartView, TaskQuery, TaskRepository},
    sync::EntityState,
    CoreError, CoreResult,
};

/// Findet den Tag mit diesem Namen (Groß/Klein egal) oder legt ihn an. Die ID ist aus dem Namen
/// abgeleitet: Zwei Geräte, die offline „#uni“ anlegen, haben danach **einen** Tag. Ist die
/// abgeleitete ID schon vergeben (gelöschter oder umbenannter Tag), nimmt er die nächste Variante.
#[derive(Debug, Clone)]
pub struct EnsureTag {
    deps: Deps,
}

impl EnsureTag {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `CoreError::Validation` bei ungültigem Namen; Fehler des Repositories.
    pub async fn execute(&self, name: &str) -> CoreResult<Tag> {
        let name = normalize_tag_name(name)?;
        let key = tag_key(&name);
        let _order = self.deps.lock_order().await;
        let repo = self.deps.repo.as_ref();
        if let Some(tag) = repo
            .list_tags()
            .await?
            .into_iter()
            .find(|t| tag_key(t.name()) == key)
        {
            return Ok(tag);
        }
        let id = free_tag_id(repo, &key).await?;
        let tag = Tag::new(id, &name, self.deps.clock.as_ref())?;
        repo.commit(ChangeSet::new().upsert_tag(tag.clone()))
            .await?;
        Ok(tag)
    }
}

async fn free_tag_id(repo: &dyn TaskRepository, key: &str) -> CoreResult<TagId> {
    for attempt in 0u8..16 {
        let id = if attempt == 0 {
            TagId::derived(&[key.as_bytes()])
        } else {
            TagId::derived(&[key.as_bytes(), b"|", &[attempt]])
        };
        match repo.entity_state(EntityRef::Tag(id)).await? {
            None => return Ok(id),
            // Gelöscht (Tombstone) oder als anderer Name in Gebrauch: nächste Variante.
            Some(EntityState::Deleted(_) | EntityState::Live(_)) => {}
        }
    }
    // Sehr unwahrscheinlich; dann eben zufällig.
    Ok(TagId::new())
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RenameTagInput {
    pub id: TagId,
    pub name: String,
}

/// Benennt einen Tag um. Gibt es den Namen schon (anderer Tag), ist das ein Fehler – Tags
/// zusammenführen ist eine eigene Aktion (später).
#[derive(Debug, Clone)]
pub struct RenameTag {
    deps: Deps,
}

impl RenameTag {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// `NotFound`, `Validation` (ungültiger oder schon vergebener Name); Fehler des Repositories.
    pub async fn execute(&self, input: RenameTagInput) -> CoreResult<Tag> {
        let repo = self.deps.repo.as_ref();
        let mut tag = repo
            .get_tag(input.id)
            .await?
            .ok_or_else(|| CoreError::NotFound(format!("tag {}", input.id)))?;
        let name = normalize_tag_name(&input.name)?;
        let key = tag_key(&name);
        if repo
            .list_tags()
            .await?
            .iter()
            .any(|t| t.id() != tag.id() && tag_key(t.name()) == key)
        {
            return Err(CoreError::Validation(format!(
                "tag {name:?} already exists"
            )));
        }
        let before = tag.clone();
        tag.rename(&name, self.deps.clock.as_ref())?;
        if tag != before {
            repo.commit(ChangeSet::new().upsert_tag(tag.clone()))
                .await?;
        }
        Ok(tag)
    }
}

/// Löscht einen Tag und nimmt ihn von allen Tasks (auch erledigten und im Papierkorb) – ein
/// Commit. Liefert, wie viele Tasks ihn trugen.
#[derive(Debug, Clone)]
pub struct DeleteTag {
    deps: Deps,
}

impl DeleteTag {
    #[must_use]
    pub fn new(deps: Deps) -> Self {
        Self { deps }
    }

    /// # Errors
    /// Fehler des Repositories. Ein unbekannter Tag ist kein Fehler (0).
    pub async fn execute(&self, id: TagId) -> CoreResult<usize> {
        let repo = self.deps.repo.as_ref();
        if repo.get_tag(id).await?.is_none() {
            return Ok(0);
        }
        let mut ids = Vec::new();
        for view in [SmartView::All, SmartView::Trash] {
            let query = TaskQuery {
                view,
                status: None,
                tag: Some(id),
            };
            ids.extend(collect_all(repo, &query).await?.into_iter().map(|t| t.id));
        }
        let clock = self.deps.clock.as_ref();
        let mut changes = ChangeSet::new();
        let mut count = 0;
        for mut task in repo.get_tasks(&ids).await? {
            let tags: Vec<TagId> = task.tags().iter().copied().filter(|t| *t != id).collect();
            task.set_tags(tags, clock)?;
            changes.push(Change::UpsertTask(task));
            count += 1;
        }
        changes.push(Change::Delete(EntityRef::Tag(id)));
        repo.commit(changes).await?;
        Ok(count)
    }
}

/// Prüft, dass alle Tags existieren (Eingaben von außen).
///
/// # Errors
/// `CoreError::Validation` für einen unbekannten Tag.
pub(crate) async fn check_tags_exist(repo: &dyn TaskRepository, tags: &[TagId]) -> CoreResult<()> {
    for id in tags {
        if repo.get_tag(*id).await?.is_none() {
            return Err(CoreError::Validation(format!("unknown tag {id}")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use futures::executor::block_on;

    use super::*;
    use crate::{
        app::{
            test_util::deps, CreateTask, CreateTaskInput, TaskPatch, UpdateTask, UpdateTaskInput,
        },
        ports::TaskRepository,
    };

    #[test]
    fn ensure_finds_by_name_and_derives_ids() {
        let (deps, repo) = deps();
        let ensure = EnsureTag::new(deps);
        let uni = block_on(ensure.execute("#Uni")).unwrap();
        assert_eq!(uni.name(), "Uni");
        assert_eq!(
            uni.id(),
            TagId::derived(&[b"uni"]),
            "abgeleitet aus dem Namen"
        );
        assert_eq!(
            block_on(ensure.execute("uni")).unwrap().id(),
            uni.id(),
            "gleicher Name"
        );
        assert_eq!(block_on(repo.list_tags()).unwrap().len(), 1);
    }

    #[test]
    fn rename_rejects_duplicates_and_delete_untags_tasks() {
        let (deps, repo) = deps();
        let ensure = EnsureTag::new(deps.clone());
        let uni = block_on(ensure.execute("uni")).unwrap();
        let bio = block_on(ensure.execute("bio")).unwrap();
        let rename = RenameTag::new(deps.clone());
        assert!(matches!(
            block_on(rename.execute(RenameTagInput {
                id: bio.id(),
                name: "UNI".into()
            })),
            Err(CoreError::Validation(_))
        ));
        let renamed = block_on(rename.execute(RenameTagInput {
            id: uni.id(),
            name: "studium".into(),
        }))
        .unwrap();
        assert_eq!(renamed.name(), "studium");

        let task = block_on(CreateTask::new(deps.clone()).execute(CreateTaskInput {
            title: "Blatt".into(),
            ..CreateTaskInput::default()
        }))
        .unwrap();
        block_on(UpdateTask::new(deps.clone()).execute(UpdateTaskInput {
            id: task.id(),
            patch: TaskPatch {
                tags: Some(vec![uni.id(), bio.id()]),
                ..TaskPatch::default()
            },
        }))
        .unwrap();
        assert_eq!(
            block_on(DeleteTag::new(deps.clone()).execute(uni.id())).unwrap(),
            1
        );
        assert_eq!(
            block_on(repo.get_task(task.id())).unwrap().unwrap().tags(),
            [bio.id()]
        );
        assert!(block_on(repo.get_tag(uni.id())).unwrap().is_none());

        // Die abgeleitete ID von „studium“ ist frei (der Tag hieß bei der Anlage „uni“).
        let again = block_on(ensure.execute("studium")).unwrap();
        assert_eq!(again.id(), TagId::derived(&[b"studium"]));
        // Neu angelegt nach dem Löschen: nicht die gelöschte ID (Tombstone), sondern die nächste Variante.
        block_on(DeleteTag::new(deps.clone()).execute(bio.id())).unwrap();
        let bio2 = block_on(ensure.execute("bio")).unwrap();
        assert_ne!(bio2.id(), bio.id());
        assert_eq!(bio2.id(), TagId::derived(&[b"bio", b"|", &[1]]));
        // Unbekannte Tags an einer Task werden abgelehnt.
        assert!(matches!(
            block_on(UpdateTask::new(deps).execute(UpdateTaskInput {
                id: task.id(),
                patch: TaskPatch {
                    tags: Some(vec![TagId::new()]),
                    ..TaskPatch::default()
                },
            })),
            Err(CoreError::Validation(_))
        ));
    }
}
