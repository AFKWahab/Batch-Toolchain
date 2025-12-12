mod breakpoints;
mod context;
mod session;
mod stepping;

pub use breakpoints::Breakpoint;
pub use context::DebugContext;
pub use session::CmdSession;
pub use stepping::RunMode;

use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Frame {
    pub return_pc: usize,
    pub args: Option<Vec<String>>,
    pub locals: HashMap<String, String>,
    pub has_setlocal: bool,
}

impl Frame {
    pub fn new(return_pc: usize, args: Option<Vec<String>>) -> Self {
        Self {
            return_pc,
            args,
            locals: HashMap::new(),
            has_setlocal: false,
        }
    }
}
pub fn leave_context(call_stack: &mut Vec<Frame>) -> Option<usize> {
    if let Some(frame) = call_stack.pop() {
        Some(frame.return_pc)
    } else {
        None
    }
}
