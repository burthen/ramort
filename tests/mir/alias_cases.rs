pub struct Queue<T> { front: Vec<T>, back: Vec<T> }

impl<T> Queue<T> {
    pub fn reborrow_push(&mut self, x: T) {
        let b = &mut self.back;
        b.push(x);
    }

    pub fn move_alias_push(&mut self, x: T) {
        let b1 = &mut self.back;
        let b2 = b1;
        b2.push(x);
    }

    pub fn branch_conflict(&mut self, x: T, cond: bool) {
        let target = if cond { &mut self.back } else { &mut self.front };
        target.push(x);
    }
}
