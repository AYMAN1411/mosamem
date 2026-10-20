//! Undo history as a list of full snapshots. Snapshots share unchanged tiles, so a step costs only
//! the tiles it changed. Old steps are dropped past a step limit or a memory budget.

use std::collections::HashSet;
use std::sync::Arc;

use lawha_core::tile::TILE_BYTES;
use serde::Serialize;

use super::model::{Content, Node};

/// A history label: a translation key plus arguments, so the panel shows it in the UI language.
#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Label {
    pub key: String,
    pub arg: Option<String>,
}

impl Label {
    pub fn new(key: &str) -> Self {
        Label { key: key.into(), arg: None }
    }
    pub fn with(key: &str, arg: impl Into<String>) -> Self {
        Label { key: key.into(), arg: Some(arg.into()) }
    }
}

pub struct State {
    pub label: Label,
    pub content: Arc<Content>,
}

pub struct History {
    pub states: Vec<State>,
    pub current: usize,
    pub max_steps: usize,
    pub budget_bytes: usize,
    pushes: usize,
}

impl History {
    pub fn new(initial: Content, label: Label) -> Self {
        History { states: vec![State { label, content: Arc::new(initial) }], current: 0, max_steps: 200, budget_bytes: 2 << 30, pushes: 0 }
    }

    pub fn push(&mut self, label: Label, content: Content) {
        self.states.truncate(self.current + 1);
        self.states.push(State { label, content: Arc::new(content) });
        self.current = self.states.len() - 1;
        self.pushes += 1;
        while self.states.len() > self.max_steps + 1 {
            self.drop_oldest();
        }
        if self.pushes % 8 == 0 {
            while self.states.len() > 2 && self.memory_bytes() > self.budget_bytes {
                self.drop_oldest();
            }
        }
    }

    fn drop_oldest(&mut self) {
        self.states.remove(0);
        self.current -= 1;
    }

    pub fn can_undo(&self) -> bool {
        self.current > 0
    }

    pub fn can_redo(&self) -> bool {
        self.current + 1 < self.states.len()
    }

    pub fn jump(&mut self, index: usize) -> Option<Arc<Content>> {
        (index < self.states.len()).then(|| {
            self.current = index;
            self.states[index].content.clone()
        })
    }

    /// Bytes of distinct tiles held by all states.
    pub fn memory_bytes(&self) -> usize {
        let mut seen = HashSet::new();
        fn walk(nodes: &[Node], seen: &mut HashSet<usize>) {
            for n in nodes {
                if let Some(t) = n.tiles() {
                    for tile in t.values() {
                        seen.insert(Arc::as_ptr(tile) as usize);
                    }
                }
                if let Some(c) = n.children() {
                    walk(c, seen);
                }
            }
        }
        for s in &self.states {
            walk(&s.content.root, &mut seen);
        }
        seen.len() * TILE_BYTES
    }
}
