use std::collections::HashMap;

#[derive(Debug, Clone)]
pub struct Breakpoint {
    pub line: usize,
    pub condition: Option<String>,
    pub hit_count: usize,
}

pub struct Breakpoints {
    points: HashMap<usize, Breakpoint>,
}

impl Breakpoints {
    pub fn new() -> Self {
        Self {
            points: HashMap::new(),
        }
    }

    pub fn add(&mut self, logical_line: usize) {
        self.add_with_condition(logical_line, None);
    }

    pub fn add_with_condition(&mut self, logical_line: usize, condition: Option<String>) {
        let bp = Breakpoint {
            line: logical_line,
            condition: condition.clone(),
            hit_count: 0,
        };
        self.points.insert(logical_line, bp);

        if let Some(cond) = condition {
            eprintln!(
                "Breakpoint set at logical line {} with condition: {}",
                logical_line, cond
            );
        } else {
            eprintln!("Breakpoint set at logical line {}", logical_line);
        }
    }

    pub fn remove(&mut self, logical_line: usize) {
        self.points.remove(&logical_line);
        eprintln!("Breakpoint removed from logical line {}", logical_line);
    }

    pub fn contains(&self, logical_line: usize) -> bool {
        self.points.contains_key(&logical_line)
    }

    pub fn get(&self, logical_line: usize) -> Option<&Breakpoint> {
        self.points.get(&logical_line)
    }

    pub fn get_mut(&mut self, logical_line: usize) -> Option<&mut Breakpoint> {
        self.points.get_mut(&logical_line)
    }

    #[allow(dead_code)]
    pub fn clear(&mut self) {
        self.points.clear();
    }
}
