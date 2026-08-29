//! Workspace-local todos for the Glass Agent checklist.
//!
//! The list persists at `.glass/todos/session.json` and is used by Agent and
//! Tasks (`glass.todo.*`). It is not the overnight [`crate::tasks`] DAG or a
//! `glass.task.crew` wake.

use crate::development::{DevelopmentError, DevelopmentResult};
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

const MAX_TODOS: usize = 32;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum TodoStatus {
    Pending,
    Active,
    Done,
}

impl TodoStatus {
    pub fn label(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Active => "active",
            Self::Done => "done",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodo {
    pub id: String,
    pub title: String,
    pub status: TodoStatus,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub surface: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionTodoList {
    pub items: Vec<SessionTodo>,
}

impl SessionTodoList {
    pub fn load(root: &Path) -> Self {
        let path = todo_path(root);
        std::fs::read(&path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    pub fn persist(&self, root: &Path) -> DevelopmentResult<()> {
        let dir = root.join(".glass/todos");
        std::fs::create_dir_all(&dir).map_err(|error| {
            DevelopmentError::Process(format!("todo directory unavailable: {error}"))
        })?;
        let bytes = serde_json::to_vec_pretty(self)?;
        std::fs::write(todo_path(root), bytes)
            .map_err(|error| DevelopmentError::Process(format!("todo write failed: {error}")))
    }

    pub fn write(&mut self, items: Vec<SessionTodo>, root: &Path) -> DevelopmentResult<()> {
        if items.len() > MAX_TODOS {
            return Err(DevelopmentError::InvalidInput(format!(
                "todo list is limited to {MAX_TODOS} items"
            )));
        }
        let mut seen = std::collections::BTreeSet::new();
        let mut active = 0;
        for item in &items {
            if item.id.is_empty() || item.id.len() > 64 || item.title.trim().is_empty() {
                return Err(DevelopmentError::InvalidInput(
                    "todo items need an id and a title".into(),
                ));
            }
            if !seen.insert(&item.id) {
                return Err(DevelopmentError::InvalidInput(format!(
                    "duplicate todo id {}",
                    item.id
                )));
            }
            if item.status == TodoStatus::Active {
                active += 1;
            }
        }
        if active > 1 {
            return Err(DevelopmentError::InvalidInput(
                "at most one todo may be active".into(),
            ));
        }
        self.items = items;
        self.persist(root)
    }

    pub fn complete(&mut self, id: &str, root: &Path) -> DevelopmentResult<SessionTodo> {
        let item = self
            .items
            .iter_mut()
            .find(|item| item.id == id)
            .ok_or_else(|| DevelopmentError::NotFound(format!("todo {id}")))?;
        item.status = TodoStatus::Done;
        let done = item.clone();
        self.persist(root)?;
        Ok(done)
    }

    pub fn activate(&mut self, id: &str, root: &Path) -> DevelopmentResult<SessionTodo> {
        if !self.items.iter().any(|item| item.id == id) {
            return Err(DevelopmentError::NotFound(format!("todo {id}")));
        }
        for item in &mut self.items {
            if item.id == id {
                item.status = TodoStatus::Active;
            } else if item.status == TodoStatus::Active {
                item.status = TodoStatus::Pending;
            }
        }
        self.persist(root)?;
        self.items
            .iter()
            .find(|item| item.id == id)
            .cloned()
            .ok_or_else(|| DevelopmentError::NotFound(format!("todo {id}")))
    }

    pub fn seed_from_plan(&mut self, goal: &str, body: &str, root: &Path) -> DevelopmentResult<()> {
        let mut items = vec![SessionTodo {
            id: "todo-goal".into(),
            title: goal.chars().take(160).collect(),
            status: TodoStatus::Active,
            surface: None,
        }];
        for (index, line) in body
            .lines()
            .map(str::trim)
            .filter(|line| !line.is_empty())
            .take(MAX_TODOS.saturating_sub(1))
            .enumerate()
        {
            let title = line
                .trim_start_matches(|character: char| {
                    character.is_ascii_digit() || matches!(character, '.' | ')' | '-' | ' ')
                })
                .chars()
                .take(160)
                .collect::<String>();
            if title.is_empty() {
                continue;
            }
            items.push(SessionTodo {
                id: format!("todo-{}", index + 1),
                title,
                status: TodoStatus::Pending,
                surface: None,
            });
        }
        self.write(items, root)
    }

    pub fn render(&self) -> String {
        if self.items.is_empty() {
            return "No session todos · Plan accept or glass.todo.write".into();
        }
        self.items
            .iter()
            .map(|item| {
                let mark = match item.status {
                    TodoStatus::Done => "✓",
                    TodoStatus::Active => "●",
                    TodoStatus::Pending => "○",
                };
                format!("{mark} {}  {}", item.id, item.title)
            })
            .collect::<Vec<_>>()
            .join("\n")
    }
}

fn todo_path(root: &Path) -> PathBuf {
    root.join(".glass/todos/session.json")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn write_rejects_two_active_items_and_seeds_from_plan() {
        let root = std::env::temp_dir().join(format!("glass-todos-{}", std::process::id()));
        std::fs::create_dir_all(&root).unwrap();
        let mut list = SessionTodoList::default();
        let err = list
            .write(
                vec![
                    SessionTodo {
                        id: "a".into(),
                        title: "one".into(),
                        status: TodoStatus::Active,
                        surface: None,
                    },
                    SessionTodo {
                        id: "b".into(),
                        title: "two".into(),
                        status: TodoStatus::Active,
                        surface: None,
                    },
                ],
                &root,
            )
            .unwrap_err();
        assert!(err.to_string().contains("at most one"));
        list.seed_from_plan("ship checkout", "1. inspect\n2. patch\n3. prove-it", &root)
            .unwrap();
        assert_eq!(list.items[0].status, TodoStatus::Active);
        assert_eq!(list.items.len(), 4);
        list.complete("todo-1", &root).unwrap();
        list.activate("todo-2", &root).unwrap();
        assert_eq!(list.items[0].status, TodoStatus::Pending);
        assert_eq!(list.items[2].status, TodoStatus::Active);
        std::fs::remove_dir_all(root).unwrap();
    }
}
