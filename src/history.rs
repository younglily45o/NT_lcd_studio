use crate::Project;
#[derive(Default)]
pub struct History {
    undo: Vec<Project>,
    redo: Vec<Project>,
    pending: Option<Project>,
}
impl History {
    pub fn begin(&mut self, before: &Project) {
        if self.pending.is_none() {
            self.pending = Some(before.clone());
        }
    }
    pub fn finish(&mut self, current: &Project) {
        if let Some(before) = self.pending.take()
            && before != *current
        {
            self.undo.push(before);
            self.redo.clear();
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
        }
    }
    pub fn undo(&mut self, current: &mut Project) -> bool {
        self.finish(current);
        if let Some(previous) = self.undo.pop() {
            self.redo.push(std::mem::replace(current, previous));
            true
        } else {
            false
        }
    }
    pub fn redo(&mut self, current: &mut Project) -> bool {
        self.finish(current);
        if let Some(next) = self.redo.pop() {
            self.undo.push(std::mem::replace(current, next));
            true
        } else {
            false
        }
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty() || self.pending.is_some()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
}
