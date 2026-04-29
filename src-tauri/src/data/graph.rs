use std::collections::{BTreeMap, BTreeSet};

use rusqlite::{params, OptionalExtension};

use crate::{
    memory::MemoryEntity,
    models::{AppPaths, StoredSession},
};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct GraphCounts {
    pub entities: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntitySessionCount {
    pub entity: String,
    pub canonical_name: String,
    pub entity_type: String,
    pub session_count: usize,
    pub created_at: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RelatedSession {
    pub session_id: String,
    pub project_slug: String,
    pub shared_entities: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EntityAliasGroup {
    pub canonical_id: String,
    pub canonical_name: String,
    pub aliases: Vec<String>,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EntityForDisambiguation {
    pub id: i64,
    pub name: String,
    pub entity_type: String,
    pub source_session: String,
    pub source_project: String,
}

struct GraphStore {
    connection: rusqlite::Connection,
}

pub fn write_session_graph(
    paths: &AppPaths,
    session: &StoredSession,
    entities: &[MemoryEntity],
) -> anyhow::Result<()> {
    let store = GraphStore::open(paths)?;
    store.write_session_graph(session, entities)
}

pub fn delete_session_entities(paths: &AppPaths, session_id: &str) -> anyhow::Result<()> {
    GraphStore::open(paths)?.delete_session_graph(session_id)
}

pub fn graph_counts(paths: &AppPaths) -> anyhow::Result<GraphCounts> {
    GraphStore::open(paths)?.counts()
}

pub fn reset_graph(paths: &AppPaths) -> anyhow::Result<()> {
    GraphStore::open(paths)?.reset()
}

pub fn entity_session_counts(paths: &AppPaths) -> anyhow::Result<Vec<EntitySessionCount>> {
    GraphStore::open(paths)?.entity_session_counts()
}

pub fn related_sessions_for_entity(
    paths: &AppPaths,
    entity: &str,
) -> anyhow::Result<Vec<RelatedSession>> {
    GraphStore::open(paths)?.related_sessions_for_entity(entity)
}

pub fn related_sessions_for_session(
    paths: &AppPaths,
    session_id: &str,
) -> anyhow::Result<Vec<RelatedSession>> {
    GraphStore::open(paths)?.related_sessions_for_session(session_id)
}

pub fn all_entities(paths: &AppPaths) -> anyhow::Result<Vec<EntityForDisambiguation>> {
    GraphStore::open(paths)?.all_entities()
}

pub fn write_entity_aliases(paths: &AppPaths, groups: &[EntityAliasGroup]) -> anyhow::Result<()> {
    GraphStore::open(paths)?.write_entity_aliases(groups)
}

impl GraphStore {
    fn open(paths: &AppPaths) -> anyhow::Result<Self> {
        std::fs::create_dir_all(&paths.data_dir)?;
        let connection = rusqlite::Connection::open(paths.data_dir.join("kittynest_graph.db"))?;
        let store = Self { connection };
        store.ensure_schema()?;
        Ok(store)
    }

    fn ensure_schema(&self) -> anyhow::Result<()> {
        self.connection.execute_batch(
            r#"
            PRAGMA foreign_keys = ON;

            CREATE TABLE IF NOT EXISTS entity (
              id INTEGER PRIMARY KEY,
              name TEXT NOT NULL,
              type TEXT NOT NULL,
              source_session TEXT NOT NULL,
              source_project TEXT NOT NULL,
              first_seen TEXT NOT NULL
            );

            CREATE TABLE IF NOT EXISTS entity_alias (
              name TEXT PRIMARY KEY,
              canonical_id TEXT NOT NULL,
              canonical_name TEXT NOT NULL DEFAULT ''
            );
            "#,
        )?;
        if self.entity_alias_has_foreign_key()? {
            self.connection.execute_batch(
                r#"
                DROP TABLE entity_alias;
                CREATE TABLE entity_alias (
                  name TEXT PRIMARY KEY,
                  canonical_id TEXT NOT NULL,
                  canonical_name TEXT NOT NULL DEFAULT ''
                );
                "#,
            )?;
        }
        add_column_if_missing(
            &self.connection,
            "entity_alias",
            "canonical_name",
            "canonical_name TEXT NOT NULL DEFAULT ''",
        )?;
        Ok(())
    }

    fn write_session_graph(
        &self,
        session: &StoredSession,
        entities: &[MemoryEntity],
    ) -> anyhow::Result<()> {
        self.delete_session_graph(&session.session_id)?;
        let mut entities_by_key = BTreeMap::new();
        for entity in entities {
            let Some(normalized) = normalized_entity_name(&entity.name) else {
                continue;
            };
            entities_by_key
                .entry(normalized)
                .or_insert_with(|| sanitized_entity(entity));
        }

        let first_seen = crate::utils::now_rfc3339();

        for (normalized, entity) in &entities_by_key {
            let id = stable_id(&format!("{}|{normalized}", session.session_id));
            self.connection.execute(
                r#"
                INSERT INTO entity (id, name, type, source_session, source_project, first_seen)
                VALUES (?1, ?2, ?3, ?4, ?5, ?6)
                ON CONFLICT(id) DO UPDATE SET
                  name = excluded.name,
                  type = excluded.type,
                  source_session = excluded.source_session,
                  source_project = excluded.source_project
                "#,
                params![
                    id,
                    normalized.as_str(),
                    entity.entity_type.as_str(),
                    session.session_id.as_str(),
                    session.project_slug.as_str(),
                    first_seen.as_str()
                ],
            )?;
            self.connection.execute(
                r#"
                INSERT INTO entity_alias (name, canonical_id, canonical_name)
                VALUES (?1, ?2, ?3)
                ON CONFLICT(name) DO UPDATE SET
                  canonical_id = excluded.canonical_id,
                  canonical_name = excluded.canonical_name
                "#,
                params![normalized.as_str(), id.to_string(), normalized.as_str()],
            )?;
        }

        Ok(())
    }

    fn delete_session_graph(&self, session_id: &str) -> anyhow::Result<()> {
        self.connection.execute(
            r#"
            DELETE FROM entity_alias
            WHERE name IN (SELECT lower(name) FROM entity WHERE source_session = ?1)
               OR canonical_id IN (SELECT id FROM entity WHERE source_session = ?1)
            "#,
            params![session_id],
        )?;
        self.connection.execute(
            "DELETE FROM entity WHERE source_session = ?1",
            params![session_id],
        )?;
        Ok(())
    }

    fn counts(&self) -> anyhow::Result<GraphCounts> {
        Ok(GraphCounts {
            entities: count(&self.connection, "SELECT COUNT(*) FROM entity")?,
        })
    }

    fn reset(&self) -> anyhow::Result<()> {
        self.connection.execute("DELETE FROM entity_alias", [])?;
        self.connection.execute("DELETE FROM entity", [])?;
        Ok(())
    }

    fn entity_session_counts(&self) -> anyhow::Result<Vec<EntitySessionCount>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT COALESCE(NULLIF(a.canonical_name, ''), lower(e.name)),
                   GROUP_CONCAT(DISTINCT e.type),
                   COUNT(DISTINCT e.source_session),
                   MIN(e.first_seen)
            FROM entity e
            LEFT JOIN entity_alias a ON a.name = lower(e.name)
            GROUP BY lower(COALESCE(NULLIF(a.canonical_name, ''), lower(e.name)))
            ORDER BY COUNT(DISTINCT e.source_session) DESC,
                     lower(COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))) ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            let session_count: i64 = row.get(2)?;
            let canonical_name: String = row.get(0)?;
            Ok(EntitySessionCount {
                entity: canonical_name.clone(),
                canonical_name,
                entity_type: row.get(1)?,
                session_count: session_count as usize,
                created_at: row.get(3)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn related_sessions_for_entity(&self, entity: &str) -> anyhow::Result<Vec<RelatedSession>> {
        let Some(normalized) = normalized_entity_name(entity) else {
            return Ok(Vec::new());
        };
        let mut statement = self.connection.prepare(
            r#"
            SELECT e.source_session, e.source_project,
                   COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))
            FROM entity e
            LEFT JOIN entity_alias a ON a.name = lower(e.name)
            WHERE lower(COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))) = ?1
            ORDER BY source_session ASC
            "#,
        )?;
        let rows = statement.query_map(params![normalized.as_str()], |row| {
            let canonical_name: String = row.get(2)?;
            Ok(RelatedSession {
                session_id: row.get(0)?,
                project_slug: row.get(1)?,
                shared_entities: vec![canonical_name],
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn related_sessions_for_session(
        &self,
        session_id: &str,
    ) -> anyhow::Result<Vec<RelatedSession>> {
        let source_entities = self.session_entities(session_id)?;
        if source_entities.is_empty() {
            return Ok(Vec::new());
        }

        let mut by_session: BTreeMap<(String, String), BTreeSet<String>> = BTreeMap::new();
        let mut statement = self.connection.prepare(
            r#"
            SELECT e.source_session, e.source_project
            FROM entity e
            LEFT JOIN entity_alias a ON a.name = lower(e.name)
            WHERE lower(COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))) = ?1
              AND e.source_session != ?2
            ORDER BY source_session ASC
            "#,
        )?;
        for (entity_key, entity_name) in source_entities {
            let rows = statement.query_map(params![entity_key.as_str(), session_id], |row| {
                Ok((row.get::<_, String>(0)?, row.get::<_, String>(1)?))
            })?;
            for row in rows {
                let (related_session_id, project_slug) = row?;
                by_session
                    .entry((related_session_id, project_slug))
                    .or_default()
                    .insert(entity_name.clone());
            }
        }

        let mut related = by_session
            .into_iter()
            .map(|((session_id, project_slug), shared)| RelatedSession {
                session_id,
                project_slug,
                shared_entities: shared.into_iter().collect(),
            })
            .collect::<Vec<_>>();
        related.sort_by(|left, right| {
            right
                .shared_entities
                .len()
                .cmp(&left.shared_entities.len())
                .then_with(|| left.session_id.cmp(&right.session_id))
        });
        Ok(related)
    }

    fn session_entities(&self, session_id: &str) -> anyhow::Result<Vec<(String, String)>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT DISTINCT lower(COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))),
                   COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))
            FROM entity e
            LEFT JOIN entity_alias a ON a.name = lower(e.name)
            WHERE e.source_session = ?1
            ORDER BY lower(COALESCE(NULLIF(a.canonical_name, ''), lower(e.name))) ASC
            "#,
        )?;
        let rows =
            statement.query_map(params![session_id], |row| Ok((row.get(0)?, row.get(1)?)))?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn all_entities(&self) -> anyhow::Result<Vec<EntityForDisambiguation>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT id, name, type, source_session, source_project
            FROM entity
            ORDER BY lower(name) ASC, source_session ASC, id ASC
            "#,
        )?;
        let rows = statement.query_map([], |row| {
            Ok(EntityForDisambiguation {
                id: row.get(0)?,
                name: row.get(1)?,
                entity_type: row.get(2)?,
                source_session: row.get(3)?,
                source_project: row.get(4)?,
            })
        })?;
        rows.collect::<Result<Vec<_>, _>>().map_err(Into::into)
    }

    fn write_entity_aliases(&self, groups: &[EntityAliasGroup]) -> anyhow::Result<()> {
        let mut canonical_ids = BTreeSet::new();
        let mut alias_names = BTreeSet::new();
        let mut rows = Vec::new();
        for group in groups {
            let canonical_name = group.canonical_name.trim();
            if canonical_name.is_empty() {
                anyhow::bail!("canonical_name cannot be empty");
            }
            let canonical_id_base = if group.canonical_id.trim().is_empty() {
                canonical_name.to_lowercase()
            } else {
                group.canonical_id.trim().to_string()
            };
            let mut canonical_id = canonical_id_base.clone();
            let mut suffix = 2usize;
            while !canonical_ids.insert(canonical_id.clone()) {
                canonical_id = format!("{canonical_id_base}-{suffix}");
                suffix += 1;
            }
            let aliases = group
                .aliases
                .iter()
                .chain(std::iter::once(&group.canonical_name))
                .filter_map(|alias| normalized_entity_name(alias))
                .collect::<BTreeSet<_>>();
            for alias in aliases {
                if !alias_names.insert(alias.clone()) {
                    continue;
                }
                rows.push((alias, canonical_id.clone(), canonical_name.to_string()));
            }
        }
        self.connection.execute("DELETE FROM entity_alias", [])?;
        for (alias, canonical_id, canonical_name) in rows {
            self.connection.execute(
                r#"
                INSERT INTO entity_alias (name, canonical_id, canonical_name)
                VALUES (?1, ?2, ?3)
                "#,
                params![alias, canonical_id, canonical_name],
            )?;
        }
        Ok(())
    }

    fn entity_alias_has_foreign_key(&self) -> anyhow::Result<bool> {
        let mut statement = self
            .connection
            .prepare("PRAGMA foreign_key_list(entity_alias)")?;
        let mut rows = statement.query([])?;
        Ok(rows.next()?.is_some())
    }
}

fn sanitized_entity(entity: &MemoryEntity) -> MemoryEntity {
    MemoryEntity {
        name: entity.name.trim().to_string(),
        entity_type: if entity.entity_type.trim().is_empty() {
            "unknown".into()
        } else {
            entity.entity_type.trim().to_string()
        },
    }
}

fn normalized_entity_name(name: &str) -> Option<String> {
    let normalized = name.trim().to_lowercase();
    if normalized.is_empty() {
        None
    } else {
        Some(normalized)
    }
}

fn stable_id(value: &str) -> i64 {
    let mut hash = 0xcbf29ce484222325u64;
    for byte in value.as_bytes() {
        hash ^= u64::from(*byte);
        hash = hash.wrapping_mul(0x100000001b3);
    }
    (hash & 0x7fff_ffff_ffff_ffff) as i64
}

fn count(connection: &rusqlite::Connection, sql: &str) -> anyhow::Result<usize> {
    let value = connection
        .query_row(sql, [], |row| row.get::<_, i64>(0))
        .optional()?
        .unwrap_or(0);
    Ok(value as usize)
}

fn add_column_if_missing(
    connection: &rusqlite::Connection,
    table: &str,
    column: &str,
    definition: &str,
) -> anyhow::Result<()> {
    let columns = connection
        .prepare(&format!("PRAGMA table_info({table})"))?
        .query_map([], |row| row.get::<_, String>(1))?
        .collect::<Result<Vec<_>, _>>()?;
    if !columns.iter().any(|name| name == column) {
        connection.execute(&format!("ALTER TABLE {table} ADD COLUMN {definition}"), [])?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        entity_session_counts, graph_counts, related_sessions_for_entity,
        related_sessions_for_session, write_entity_aliases, write_session_graph, EntityAliasGroup,
    };
    use crate::{
        memory::MemoryEntity,
        models::{AppPaths, StoredSession},
    };

    #[test]
    fn write_session_graph_persists_entities_only() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let session = StoredSession {
            id: 7,
            source: "codex".into(),
            session_id: "graph-session".into(),
            project_id: 1,
            project_slug: "GraphProject".into(),
            task_id: None,
            workdir: "/tmp/graph".into(),
            created_at: "2026-04-27T00:00:00Z".into(),
            updated_at: "2026-04-27T00:00:01Z".into(),
            messages: vec![],
        };

        write_session_graph(
            &paths,
            &session,
            &[MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        assert_eq!(graph_counts(&paths).unwrap().entities, 1);
        assert_eq!(entity_session_counts(&paths).unwrap()[0].session_count, 1);
    }

    #[test]
    fn related_sessions_share_normalized_entities() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let first = stored_session(1, "first-session", "GraphProject");
        let second = stored_session(2, "second-session", "GraphProject");

        write_session_graph(
            &paths,
            &first,
            &[MemoryEntity {
                name: "SQLite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();
        write_session_graph(
            &paths,
            &second,
            &[MemoryEntity {
                name: "sqlite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        let related = related_sessions_for_session(&paths, "first-session").unwrap();
        assert_eq!(related[0].session_id, "second-session");
        assert_eq!(related[0].shared_entities, vec!["sqlite".to_string()]);
    }

    #[test]
    fn related_session_queries_match_legacy_mixed_case_entities() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        graph_counts(&paths).unwrap();
        let connection =
            rusqlite::Connection::open(paths.data_dir.join("kittynest_graph.db")).unwrap();
        connection
            .execute(
                r#"
            INSERT INTO entity (id, name, type, source_session, source_project, first_seen)
            VALUES
              (1, 'KittyNest', 'project', 'current-session', 'KittyNest', '2026-04-27T00:00:00Z'),
              (2, 'kittynest', 'project', 'related-session', 'KittyNest', '2026-04-27T00:00:00Z')
            "#,
                [],
            )
            .unwrap();

        let entity_related = related_sessions_for_entity(&paths, "KittyNest").unwrap();
        let session_related = related_sessions_for_session(&paths, "current-session").unwrap();

        assert_eq!(entity_related.len(), 2);
        assert_eq!(session_related[0].session_id, "related-session");
        assert_eq!(
            session_related[0].shared_entities,
            vec!["kittynest".to_string()]
        );
    }

    #[test]
    fn entity_counts_group_by_visible_entity_name_across_types() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let first = stored_session(1, "jobs-table-session", "GraphProject");
        let second = stored_session(2, "jobs-feature-session", "GraphProject");

        write_session_graph(
            &paths,
            &first,
            &[MemoryEntity {
                name: "jobs".into(),
                entity_type: "sqlite_table".into(),
            }],
        )
        .unwrap();
        write_session_graph(
            &paths,
            &second,
            &[MemoryEntity {
                name: "Jobs".into(),
                entity_type: "database_table".into(),
            }],
        )
        .unwrap();

        let counts = entity_session_counts(&paths).unwrap();

        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].entity, "jobs");
        assert_eq!(counts[0].session_count, 2);
    }

    #[test]
    fn entity_aliases_merge_counts_and_related_sessions_by_canonical_name() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let first = stored_session(1, "sqlite-session", "GraphProject");
        let second = stored_session(2, "sqlite-db-session", "GraphProject");

        write_session_graph(
            &paths,
            &first,
            &[MemoryEntity {
                name: "sqlite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();
        write_session_graph(
            &paths,
            &second,
            &[MemoryEntity {
                name: "SQLite DB".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        write_entity_aliases(
            &paths,
            &[EntityAliasGroup {
                canonical_id: "sqlite".into(),
                canonical_name: "SQLite".into(),
                aliases: vec!["sqlite".into(), "SQLite DB".into()],
            }],
        )
        .unwrap();

        let counts = entity_session_counts(&paths).unwrap();
        let related = related_sessions_for_session(&paths, "sqlite-session").unwrap();
        let entity_related = related_sessions_for_entity(&paths, "SQLite").unwrap();

        assert_eq!(counts.len(), 1);
        assert_eq!(counts[0].entity, "SQLite");
        assert_eq!(counts[0].session_count, 2);
        assert_eq!(related[0].session_id, "sqlite-db-session");
        assert_eq!(related[0].shared_entities, vec!["SQLite".to_string()]);
        assert_eq!(entity_related.len(), 2);
    }

    #[test]
    fn entity_aliases_tolerate_duplicate_aliases_from_llm() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        let first = stored_session(1, "sqlite-session", "GraphProject");
        let second = stored_session(2, "sqlite-db-session", "GraphProject");

        write_session_graph(
            &paths,
            &first,
            &[MemoryEntity {
                name: "sqlite".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();
        write_session_graph(
            &paths,
            &second,
            &[MemoryEntity {
                name: "SQLite DB".into(),
                entity_type: "technology".into(),
            }],
        )
        .unwrap();

        write_entity_aliases(
            &paths,
            &[
                EntityAliasGroup {
                    canonical_id: "sqlite".into(),
                    canonical_name: "SQLite".into(),
                    aliases: vec!["sqlite".into(), "SQLite DB".into()],
                },
                EntityAliasGroup {
                    canonical_id: "sqlite-duplicate".into(),
                    canonical_name: "SQLite Duplicate".into(),
                    aliases: vec!["sqlite".into()],
                },
            ],
        )
        .unwrap();

        let counts = entity_session_counts(&paths).unwrap();
        assert_eq!(counts[0].entity, "SQLite");
        assert_eq!(counts[0].session_count, 2);
    }

    #[test]
    fn entity_aliases_migrate_legacy_foreign_key_schema() {
        let temp = tempfile::tempdir().unwrap();
        let paths = AppPaths::from_data_dir(temp.path().join("kittynest"));
        std::fs::create_dir_all(&paths.data_dir).unwrap();
        let connection =
            rusqlite::Connection::open(paths.data_dir.join("kittynest_graph.db")).unwrap();
        connection
            .execute_batch(
                r#"
                PRAGMA foreign_keys = ON;
                CREATE TABLE entity (
                  id INTEGER PRIMARY KEY,
                  name TEXT NOT NULL,
                  type TEXT NOT NULL,
                  source_session TEXT NOT NULL,
                  source_project TEXT NOT NULL,
                  first_seen TEXT NOT NULL
                );
                CREATE TABLE entity_alias (
                  name TEXT PRIMARY KEY,
                  canonical_id INTEGER NOT NULL REFERENCES entity(id) ON DELETE CASCADE
                );
                INSERT INTO entity (id, name, type, source_session, source_project, first_seen)
                VALUES (1, 'sqlite', 'technology', 'sqlite-session', 'GraphProject', '2026-04-27T00:00:00Z');
                "#,
            )
            .unwrap();
        drop(connection);

        write_entity_aliases(
            &paths,
            &[EntityAliasGroup {
                canonical_id: "sqlite".into(),
                canonical_name: "SQLite".into(),
                aliases: vec!["sqlite".into()],
            }],
        )
        .unwrap();

        let counts = entity_session_counts(&paths).unwrap();
        assert_eq!(counts[0].entity, "SQLite");
    }

    fn stored_session(id: i64, session_id: &str, project_slug: &str) -> StoredSession {
        StoredSession {
            id,
            source: "codex".into(),
            session_id: session_id.into(),
            project_id: 1,
            project_slug: project_slug.into(),
            task_id: None,
            workdir: format!("/tmp/{project_slug}"),
            created_at: "2026-04-27T00:00:00Z".into(),
            updated_at: "2026-04-27T00:00:01Z".into(),
            messages: vec![],
        }
    }
}
