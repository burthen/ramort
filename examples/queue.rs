#![feature(register_tool)]
#![register_tool(amortized)]

#[amortized::infer_potential("front.len,back.len")]
pub struct Queue<T> {
    front: Vec<T>,
    back: Vec<T>,
}

impl<T> Queue<T> {
    #[amortized::cost("3")]
    pub fn push(&mut self, x: T) {
        self.back.push(x);
    }

    #[amortized::cost("1")]
    pub fn pop(&mut self) -> Option<T> {
        if self.front.is_empty() {
            while let Some(x) = self.back.pop() {
                self.front.push(x);
            }
        }
        self.front.pop()
    }
}
