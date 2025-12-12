use super::breakpoints::Breakpoints;
use super::{CmdSession, Frame, RunMode};
use crate::parser::{ForLoopType, IfCondition, LogicalLine};
use std::collections::HashMap;
use std::io;

pub struct DebugContext {
    session: CmdSession,
    pub variables: HashMap<String, String>,
    pub call_stack: Vec<Frame>,
    pub last_exit_code: i32,
    breakpoints: Breakpoints,
    mode: RunMode,
    step_out_target_depth: usize,
    pub continue_requested: bool,
    pub current_line: Option<usize>,
    data_breakpoints: HashMap<String, String>, // variable name -> previous value
    pub data_breakpoint_hit: Option<(String, String, String)>, // (var_name, old_value, new_value)
    directory_stack: Vec<String>,              // PUSHD/POPD directory stack
}

impl DebugContext {
    pub fn new(session: CmdSession) -> Self {
        Self {
            session,
            variables: HashMap::new(),
            call_stack: Vec::new(),
            last_exit_code: 0,
            data_breakpoints: HashMap::new(),
            data_breakpoint_hit: None,
            breakpoints: Breakpoints::new(),
            mode: RunMode::Continue,
            step_out_target_depth: 0,
            continue_requested: false,
            current_line: None,
            directory_stack: Vec::new(),
        }
    }

    pub fn session_mut(&mut self) -> &mut CmdSession {
        &mut self.session
    }

    pub fn mode(&self) -> RunMode {
        self.mode
    }

    pub fn set_mode(&mut self, mode: RunMode) {
        self.mode = mode;
    }

    pub fn handle_setlocal(&mut self) {
        if let Some(frame) = self.call_stack.last_mut() {
            frame.has_setlocal = true;
            eprintln!("SETLOCAL: Created new variable scope");
        }
    }

    pub fn handle_endlocal(&mut self) {
        if let Some(frame) = self.call_stack.last_mut() {
            if frame.has_setlocal {
                frame.locals.clear();
                frame.has_setlocal = false;
                eprintln!("ENDLOCAL: Restored previous scope");
            }
        }
    }
    pub fn get_visible_variables(&self) -> HashMap<String, String> {
        let mut visible = self.variables.clone();
        // Overlay local variables from current frame if SETLOCAL is active
        if let Some(frame) = self.call_stack.last() {
            if frame.has_setlocal {
                visible.extend(frame.locals.clone());
            }
        }

        visible
    }

    pub fn get_frame_variables(&self, frame_index: usize) -> HashMap<String, String> {
        if frame_index < self.call_stack.len() {
            let frame = &self.call_stack[frame_index];
            if frame.has_setlocal {
                return frame.locals.clone();
            }
        }
        HashMap::new()
    }

    pub fn print_call_stack(&self, logical: &[LogicalLine]) {
        if self.call_stack.is_empty() {
            eprintln!("\n=== Call Stack: <empty - top level> ===");
            return;
        }

        eprintln!("\n=== Call Stack ({} frames) ===", self.call_stack.len());
        for (i, frame) in self.call_stack.iter().enumerate().rev() {
            let return_line = frame.return_pc.saturating_sub(1);
            if return_line < logical.len() {
                let line = &logical[return_line];
                let scope_info = if frame.has_setlocal {
                    format!(" [SETLOCAL: {} vars]", frame.locals.len())
                } else {
                    String::new()
                };
                eprintln!(
                    "  #{}: return to logical line {} (phys line {}){}",
                    i,
                    frame.return_pc,
                    line.phys_start + 1,
                    scope_info
                );
            } else {
                eprintln!("  #{}: return to logical line {}", i, frame.return_pc);
            }
        }
        eprintln!();
    }

    pub fn print_variables(&self) {
        let visible = self.get_visible_variables();
        if visible.is_empty() {
            return;
        }
        eprintln!("\n=== Tracked Variables ===");
        let mut vars: Vec<_> = visible.iter().collect();
        vars.sort_by_key(|(k, _)| *k);
        for (key, val) in vars {
            eprintln!("  {}={}", key, val);
        }
        eprintln!();
    }

    pub fn track_set_command(&mut self, line: &str) {
        let l = line.trim_start();
        if !l.to_uppercase().starts_with("SET ") {
            return;
        }

        let rest = l[3..].trim_start();

        // Handle SET /A (arithmetic)
        if rest.to_uppercase().starts_with("/A") {
            let expr = rest[2..].trim_start();
            if let Some(eq_pos) = expr.find('=') {
                let mut key = expr[..eq_pos].trim().to_string();

                // Handle compound assignment operators (+=, -=, *=, /=, etc.)
                // Remove the operator suffix to get the variable name
                if key.ends_with('+')
                    || key.ends_with('-')
                    || key.ends_with('*')
                    || key.ends_with('/')
                    || key.ends_with('%')
                    || key.ends_with('&')
                    || key.ends_with('|')
                    || key.ends_with('^')
                {
                    key.pop();
                    key = key.trim().to_string();
                }

                // Execute the SET /A command and capture the result
                // SET /A echoes the result, so we capture it
                if let Ok((output, exit_code)) = self.session.run(line) {
                    self.last_exit_code = exit_code;

                    // The result is the last line of output (the echoed value)
                    let val = output.trim().to_string();

                    if !key.is_empty() {
                        // Store in local scope if SETLOCAL is active, otherwise global
                        if let Some(frame) = self.call_stack.last_mut() {
                            if frame.has_setlocal {
                                frame.locals.insert(key.clone(), val.clone());
                                eprintln!("SET /A: {}={} (local scope)", key, val);
                                return;
                            }
                        }
                        self.variables.insert(key.clone(), val.clone());
                        eprintln!("SET /A: {}={}", key, val);
                    }
                }
            }
            return;
        }

        // Handle SET /P (prompt for input)
        if rest.to_uppercase().starts_with("/P") {
            let expr = rest[2..].trim_start();
            if let Some(eq_pos) = expr.find('=') {
                let key = expr[..eq_pos].trim().to_string();

                // For SET /P, we need to detect the result after the command executes
                // The command has already executed by the time we track it
                // We'll query the variable value from the session
                if !key.is_empty() {
                    let query_cmd = format!("echo %{}%", key);
                    if let Ok((output, _)) = self.session.run(&query_cmd) {
                        let val = output.trim().to_string();

                        // Store in local scope if SETLOCAL is active, otherwise global
                        if let Some(frame) = self.call_stack.last_mut() {
                            if frame.has_setlocal {
                                frame.locals.insert(key.clone(), val.clone());
                                eprintln!("SET /P: {}={} (local scope)", key, val);
                                return;
                            }
                        }
                        self.variables.insert(key.clone(), val.clone());
                        eprintln!("SET /P: {}={}", key, val);
                    }
                }
            }
            return;
        }

        // Handle regular SET
        let rest = rest.trim();
        let rest = if rest.starts_with('"') && rest.ends_with('"') && rest.len() >= 2 {
            &rest[1..rest.len() - 1]
        } else {
            rest
        };

        if let Some(eq_pos) = rest.find('=') {
            let key = rest[..eq_pos].trim().to_string();
            let val = rest[eq_pos + 1..].trim().to_string();

            if !key.is_empty()
                && !key.contains('+')
                && !key.contains('-')
                && !key.contains('*')
                && !key.contains('/')
            {
                // Store in local scope if SETLOCAL is active, otherwise global
                if let Some(frame) = self.call_stack.last_mut() {
                    if frame.has_setlocal {
                        frame.locals.insert(key, val);
                        return;
                    }
                }
                self.variables.insert(key, val);
            }
        }
    }

    pub fn add_breakpoint(&mut self, logical_line: usize) {
        self.breakpoints.add(logical_line);
    }

    pub fn add_breakpoint_with_condition(
        &mut self,
        logical_line: usize,
        condition: Option<String>,
    ) {
        self.breakpoints.add_with_condition(logical_line, condition);
    }

    #[allow(dead_code)]
    pub fn remove_breakpoint(&mut self, logical_line: usize) {
        self.breakpoints.remove(logical_line);
    }

    pub fn get_breakpoint(
        &self,
        logical_line: usize,
    ) -> Option<&crate::debugger::breakpoints::Breakpoint> {
        self.breakpoints.get(logical_line)
    }

    /// Add a data breakpoint on a variable
    pub fn add_data_breakpoint(&mut self, variable_name: String) {
        let visible = self.get_visible_variables();
        let current_value = visible.get(&variable_name).cloned().unwrap_or_default();
        self.data_breakpoints
            .insert(variable_name.clone(), current_value);
        eprintln!("Added data breakpoint on variable: {}", variable_name);
    }

    /// Remove a data breakpoint
    pub fn remove_data_breakpoint(&mut self, variable_name: &str) {
        self.data_breakpoints.remove(variable_name);
        eprintln!("Removed data breakpoint on variable: {}", variable_name);
    }

    /// Check if any data breakpoints were hit (variable changed)
    pub fn check_data_breakpoints(&mut self) -> bool {
        self.data_breakpoint_hit = None;
        let visible = self.get_visible_variables();

        for (var_name, old_value) in &self.data_breakpoints {
            let new_value = visible.get(var_name).cloned().unwrap_or_default();
            if &new_value != old_value {
                eprintln!(
                    "Data breakpoint hit: {} changed from '{}' to '{}'",
                    var_name, old_value, new_value
                );
                self.data_breakpoint_hit =
                    Some((var_name.clone(), old_value.clone(), new_value.clone()));
                return true;
            }
        }
        false
    }

    /// Update data breakpoint previous values after stopping
    pub fn update_data_breakpoints(&mut self) {
        let visible = self.get_visible_variables();
        for (var_name, old_value) in self.data_breakpoints.iter_mut() {
            let new_value = visible.get(var_name).cloned().unwrap_or_default();
            *old_value = new_value;
        }
    }

    /// Get all data breakpoints
    pub fn get_data_breakpoints(&self) -> &HashMap<String, String> {
        &self.data_breakpoints
    }

    pub fn should_stop_at(&mut self, pc: usize) -> bool {
        match self.mode {
            RunMode::Continue => {
                if !self.breakpoints.contains(pc) {
                    return false;
                }

                // Extract condition before evaluating to avoid borrow checker issues
                let condition_opt = self.breakpoints.get(pc).and_then(|bp| bp.condition.clone());

                // Increment hit count
                if let Some(bp) = self.breakpoints.get_mut(pc) {
                    bp.hit_count += 1;
                }

                // Check condition if present
                if let Some(condition) = condition_opt {
                    // Evaluate condition
                    match self.evaluate_expression(&condition) {
                        Ok(result) => {
                            let result_trimmed = result.trim();
                            // Consider non-empty, non-zero as true
                            let is_true = !result_trimmed.is_empty()
                                && result_trimmed != "0"
                                && !result_trimmed.eq_ignore_ascii_case("false");

                            if !is_true {
                                eprintln!(
                                    "⊘ Breakpoint condition false: {} = '{}'",
                                    condition, result_trimmed
                                );
                                return false;
                            }
                            eprintln!(
                                "Breakpoint condition true: {} = '{}'",
                                condition, result_trimmed
                            );
                        }
                        Err(e) => {
                            eprintln!("WARNING: Breakpoint condition error: {} - {}", condition, e);
                            // On error, stop anyway (safer)
                            return true;
                        }
                    }
                }

                true
            }
            RunMode::StepOver | RunMode::StepInto => true,
            RunMode::StepOut => self.call_stack.len() <= self.step_out_target_depth,
        }
    }

    pub fn handle_step_command(&mut self, step_type: &str) {
        match step_type {
            "continue" => {
                self.mode = RunMode::Continue;
                eprintln!("Continuing execution...");
            }
            "next" | "stepOver" => {
                self.mode = RunMode::StepOver;
                eprintln!("Step Over");
            }
            "stepIn" | "stepInto" => {
                self.mode = RunMode::StepInto;
                eprintln!("Step Into");
            }
            "stepOut" => {
                self.mode = RunMode::StepOut;
                self.step_out_target_depth = self.call_stack.len().saturating_sub(1);
                eprintln!("Step Out (target depth: {})", self.step_out_target_depth);
            }
            _ => {
                eprintln!("Unknown step command: {}", step_type);
            }
        }
    }

    pub fn run_command(&mut self, cmd: &str) -> io::Result<(String, i32)> {
        self.session.run(cmd)
    }

    /// Set a variable value directly (used by DAP setVariable request)
    pub fn set_variable(&mut self, name: &str, value: &str) -> io::Result<()> {
        // Determine if we're in a SETLOCAL scope
        let in_local_scope = self
            .call_stack
            .last()
            .map(|frame| frame.has_setlocal)
            .unwrap_or(false);

        // Execute SET command in the CMD session
        let set_cmd = format!("SET {}={}", name, value);
        let (_, exit_code) = self.run_command(&set_cmd)?;
        self.last_exit_code = exit_code;

        // Update our tracking
        if in_local_scope {
            if let Some(frame) = self.call_stack.last_mut() {
                frame.locals.insert(name.to_string(), value.to_string());
            }
        } else {
            self.variables.insert(name.to_string(), value.to_string());
        }

        eprintln!("Variable set: {}={}", name, value);
        Ok(())
    }

    /// Evaluate an expression (used by DAP evaluate request)
    pub fn evaluate_expression(&mut self, expression: &str) -> io::Result<String> {
        let expr = expression.trim();

        eprintln!("EVAL: Evaluating expression: '{}'", expr);

        // Handle special cases
        if expr.eq_ignore_ascii_case("ERRORLEVEL") || expr == "%ERRORLEVEL%" {
            return Ok(self.last_exit_code.to_string());
        }

        // Detect string operations for logging
        if expr.contains(":~") {
            eprintln!("   STRING_OP: Detected substring operation");
        } else if expr.contains(':') && (expr.contains('=') || expr.contains('*')) {
            eprintln!("   STRING_OP: Detected string substitution operation");
        }

        // Handle simple variable lookup: %VAR% or VAR
        if expr.starts_with('%') && expr.ends_with('%') && expr.len() > 2 {
            let var_name = &expr[1..expr.len() - 1];

            // Check if it's a simple variable (no string operations)
            if !var_name.contains(':') {
                let visible = self.get_visible_variables();
                if let Some(value) = visible.get(var_name) {
                    return Ok(value.clone());
                }
            }
            // Variable with string operations or not found, try executing in CMD
        } else if !expr.contains(' ')
            && !expr.contains('=')
            && !expr.contains('&')
            && !expr.contains(':')
        {
            // Simple identifier - try looking it up first
            let visible = self.get_visible_variables();
            if let Some(value) = visible.get(expr) {
                return Ok(value.clone());
            }
        }

        // For complex expressions (including string operations), execute in CMD and capture output
        // Use echo to evaluate the expression
        // This handles:
        // - %VAR:~0,5% (substring)
        // - %VAR:~-3% (substring from end)
        // - %VAR:old=new% (string replacement)
        // - %VAR:*=new% (replace from start)
        // - Complex expressions with multiple variables
        let (output, exit_code) = self.run_command(&format!("echo {}", expr))?;

        // Update exit code
        self.last_exit_code = exit_code;

        // Return trimmed output
        let result = output.trim().to_string();
        eprintln!("   Result: '{}'", result);
        Ok(result)
    }

    /// Evaluate an IF condition and return whether it's true
    pub fn evaluate_if_condition(&mut self, condition: &IfCondition) -> io::Result<bool> {
        match condition {
            IfCondition::ErrorLevel { not, level } => {
                // ERRORLEVEL n is true if exit code >= n
                let result = self.last_exit_code >= *level;
                let final_result = if *not { !result } else { result };
                eprintln!(
                    "IF {}ERRORLEVEL {} -> {} (exit code: {})",
                    if *not { "NOT " } else { "" },
                    level,
                    final_result,
                    self.last_exit_code
                );
                Ok(final_result)
            }

            IfCondition::StringEqual { not, left, right } => {
                // Expand variables in both sides
                let left_expanded = self.expand_variables(left)?;
                let right_expanded = self.expand_variables(right)?;

                // String comparison is case-insensitive in batch
                let result = left_expanded.eq_ignore_ascii_case(&right_expanded);
                let final_result = if *not { !result } else { result };
                eprintln!(
                    "IF {}\"{}\" == \"{}\" -> {} (expanded: \"{}\" vs \"{}\")",
                    if *not { "NOT " } else { "" },
                    left,
                    right,
                    final_result,
                    left_expanded,
                    right_expanded
                );
                Ok(final_result)
            }

            IfCondition::Exist { not, path } => {
                // Expand variables in path
                let path_expanded = self.expand_variables(path)?;

                // Use CMD's existence check
                let check_cmd = format!("if exist \"{}\" (echo 1) else (echo 0)", path_expanded);
                let (output, _) = self.run_command(&check_cmd)?;
                let result = output.trim() == "1";
                let final_result = if *not { !result } else { result };
                eprintln!(
                    "IF {}EXIST \"{}\" -> {} (path: \"{}\")",
                    if *not { "NOT " } else { "" },
                    path,
                    final_result,
                    path_expanded
                );
                Ok(final_result)
            }

            IfCondition::Defined { not, variable } => {
                let visible = self.get_visible_variables();
                let result = visible.contains_key(variable);
                let final_result = if *not { !result } else { result };
                eprintln!(
                    "IF {}DEFINED {} -> {}",
                    if *not { "NOT " } else { "" },
                    variable,
                    final_result
                );
                Ok(final_result)
            }

            IfCondition::Compare {
                not,
                left,
                op,
                right,
            } => {
                // Expand variables
                let left_expanded = self.expand_variables(left)?;
                let right_expanded = self.expand_variables(right)?;

                // Try to parse as numbers for numeric comparison
                let left_num = left_expanded.trim().parse::<i32>();
                let right_num = right_expanded.trim().parse::<i32>();

                let result = match (left_num, right_num) {
                    (Ok(l), Ok(r)) => {
                        // Numeric comparison
                        match op.to_uppercase().as_str() {
                            "EQU" => l == r,
                            "NEQ" => l != r,
                            "LSS" => l < r,
                            "LEQ" => l <= r,
                            "GTR" => l > r,
                            "GEQ" => l >= r,
                            _ => false,
                        }
                    }
                    _ => {
                        // String comparison (case-insensitive)
                        match op.to_uppercase().as_str() {
                            "EQU" => left_expanded.eq_ignore_ascii_case(&right_expanded),
                            "NEQ" => !left_expanded.eq_ignore_ascii_case(&right_expanded),
                            "LSS" => left_expanded.to_lowercase() < right_expanded.to_lowercase(),
                            "LEQ" => left_expanded.to_lowercase() <= right_expanded.to_lowercase(),
                            "GTR" => left_expanded.to_lowercase() > right_expanded.to_lowercase(),
                            "GEQ" => left_expanded.to_lowercase() >= right_expanded.to_lowercase(),
                            _ => false,
                        }
                    }
                };

                let final_result = if *not { !result } else { result };
                eprintln!(
                    "IF {}\"{}\" {} \"{}\" -> {} (expanded: \"{}\" {} \"{}\")",
                    if *not { "NOT " } else { "" },
                    left,
                    op,
                    right,
                    final_result,
                    left_expanded,
                    op,
                    right_expanded
                );
                Ok(final_result)
            }
        }
    }

    /// Helper to expand variables in a string
    fn expand_variables(&mut self, text: &str) -> io::Result<String> {
        // Use echo to expand variables
        let (output, _) = self.run_command(&format!("echo {}", text))?;
        Ok(output.trim().to_string())
    }

    /// Expand a FOR loop into individual iterations
    /// Returns a vector of (command, loop_variable, loop_value) tuples
    pub fn expand_for_loop(
        &mut self,
        loop_type: &ForLoopType,
    ) -> io::Result<Vec<(String, String, String)>> {
        match loop_type {
            ForLoopType::Basic {
                variable,
                items,
                command,
            } => {
                eprintln!("Expanding basic FOR loop: {} items", items.len());
                let mut iterations = Vec::new();

                for item in items {
                    let expanded_item = self.expand_variables(item)?;
                    let expanded_command = command.replace(variable, &expanded_item);
                    iterations.push((expanded_command, variable.clone(), expanded_item));
                }

                Ok(iterations)
            }

            ForLoopType::Numeric {
                variable,
                start,
                step,
                end,
                command,
            } => {
                eprintln!(
                    "Expanding numeric FOR loop: {} to {} by {}",
                    start, end, step
                );
                let mut iterations = Vec::new();

                // Handle both positive and negative steps
                if *step > 0 {
                    let mut current = *start;
                    while current <= *end {
                        let value = current.to_string();
                        let expanded_command = command.replace(variable, &value);
                        iterations.push((expanded_command, variable.clone(), value));
                        current += step;
                    }
                } else if *step < 0 {
                    let mut current = *start;
                    while current >= *end {
                        let value = current.to_string();
                        let expanded_command = command.replace(variable, &value);
                        iterations.push((expanded_command, variable.clone(), value));
                        current += step;
                    }
                } else {
                    // Step is 0, infinite loop - just return empty
                    eprintln!("WARNING: FOR /L with step=0 would create infinite loop, skipping");
                }

                Ok(iterations)
            }

            ForLoopType::FileParser {
                variable,
                options,
                source,
                command,
            } => {
                use crate::parser::ForFileSource;

                eprintln!("Expanding FOR /F loop");
                let mut iterations = Vec::new();

                // Build the FOR /F command and execute it to get the results
                let for_cmd = match source {
                    ForFileSource::File(path) => {
                        if options.is_empty() {
                            format!("FOR /F {} IN ({}) DO echo {}", variable, path, variable)
                        } else {
                            format!(
                                "FOR /F \"{}\" {} IN ({}) DO echo {}",
                                options, variable, path, variable
                            )
                        }
                    }
                    ForFileSource::Command(cmd) => {
                        if options.is_empty() {
                            format!("FOR /F {} IN ('{}') DO echo {}", variable, cmd, variable)
                        } else {
                            format!(
                                "FOR /F \"{}\" {} IN ('{}') DO echo {}",
                                options, variable, cmd, variable
                            )
                        }
                    }
                    ForFileSource::String(s) => {
                        if options.is_empty() {
                            format!("FOR /F {} IN (\"{}\") DO echo {}", variable, s, variable)
                        } else {
                            format!(
                                "FOR /F \"{}\" {} IN (\"{}\") DO echo {}",
                                options, variable, s, variable
                            )
                        }
                    }
                };

                // Execute the FOR /F to get all values
                match self.run_command(&for_cmd) {
                    Ok((output, _)) => {
                        for line in output.lines() {
                            let value = line.trim().to_string();
                            if !value.is_empty() {
                                let expanded_command = command.replace(variable, &value);
                                iterations.push((expanded_command, variable.clone(), value));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("WARNING: FOR /F expansion error: {}", e);
                        return Err(e);
                    }
                }

                Ok(iterations)
            }

            ForLoopType::Directory {
                variable,
                pattern,
                command,
            } => {
                eprintln!("Expanding FOR /D loop: {}", pattern);
                let mut iterations = Vec::new();

                // Execute FOR /D to get directory names
                let for_cmd = format!("FOR /D {} IN ({}) DO echo {}", variable, pattern, variable);

                match self.run_command(&for_cmd) {
                    Ok((output, _)) => {
                        for line in output.lines() {
                            let value = line.trim().to_string();
                            if !value.is_empty() {
                                let expanded_command = command.replace(variable, &value);
                                iterations.push((expanded_command, variable.clone(), value));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("WARNING: FOR /D expansion error: {}", e);
                        return Err(e);
                    }
                }

                Ok(iterations)
            }

            ForLoopType::Recursive {
                variable,
                root_path,
                pattern,
                command,
            } => {
                eprintln!("Expanding FOR /R loop");
                let mut iterations = Vec::new();

                // Build FOR /R command
                let for_cmd = if let Some(path) = root_path {
                    format!(
                        "FOR /R {} {} IN ({}) DO echo {}",
                        path, variable, pattern, variable
                    )
                } else {
                    format!("FOR /R {} IN ({}) DO echo {}", variable, pattern, variable)
                };

                match self.run_command(&for_cmd) {
                    Ok((output, _)) => {
                        for line in output.lines() {
                            let value = line.trim().to_string();
                            if !value.is_empty() {
                                let expanded_command = command.replace(variable, &value);
                                iterations.push((expanded_command, variable.clone(), value));
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("WARNING: FOR /R expansion error: {}", e);
                        return Err(e);
                    }
                }

                Ok(iterations)
            }
        }
    }

    /// Set a loop variable value (for tracking during FOR loop execution)
    pub fn set_loop_variable(&mut self, name: &str, value: &str) {
        // Loop variables are tracked in the current scope
        if let Some(frame) = self.call_stack.last_mut() {
            if frame.has_setlocal {
                frame.locals.insert(name.to_string(), value.to_string());
                eprintln!("Loop variable set: {}={} (local scope)", name, value);
                return;
            }
        }
        self.variables.insert(name.to_string(), value.to_string());
        eprintln!("Loop variable set: {}={}", name, value);
    }

    /// Handle PUSHD command - push current directory onto stack
    pub fn handle_pushd(&mut self, path: Option<&str>) -> io::Result<()> {
        use std::env;

        // Get current directory from Rust's process
        let current_dir = env::current_dir()?;
        let current_dir_str = current_dir.to_string_lossy().to_string();

        // Push onto stack
        self.directory_stack.push(current_dir_str.clone());
        eprintln!(
            "PUSHD: pushed '{}' onto stack (depth: {})",
            current_dir_str,
            self.directory_stack.len()
        );

        // Change to new directory if provided
        if let Some(new_path) = path {
            // Change Rust's process directory
            env::set_current_dir(new_path)?;

            // Also sync CMD session
            let (_, exit_code) = self.run_command(&format!("cd /d {}", new_path))?;
            self.last_exit_code = exit_code;
        }

        Ok(())
    }

    /// Handle POPD command - pop directory from stack and change to it
    pub fn handle_popd(&mut self) -> io::Result<()> {
        use std::env;

        if let Some(dir) = self.directory_stack.pop() {
            eprintln!(
                "POPD: popped '{}' from stack (depth: {})",
                dir,
                self.directory_stack.len()
            );

            // Change Rust's process directory
            env::set_current_dir(&dir)?;

            // Also sync CMD session
            let (_, exit_code) = self.run_command(&format!("cd /d {}", dir))?;
            self.last_exit_code = exit_code;

            Ok(())
        } else {
            eprintln!("WARNING: POPD: directory stack is empty");
            self.last_exit_code = 1;
            Err(io::Error::new(
                io::ErrorKind::Other,
                "Directory stack is empty",
            ))
        }
    }

    /// Get the directory stack for display
    pub fn get_directory_stack(&self) -> &[String] {
        &self.directory_stack
    }

    /// Handle SHIFT command - shift parameters in current call frame
    pub fn handle_shift(&mut self, count: usize) {
        if let Some(frame) = self.call_stack.last_mut() {
            if let Some(ref mut args) = frame.args {
                if count > 0 {
                    let actual_shift = count.min(args.len());
                    args.drain(0..actual_shift);

                    if count > actual_shift {
                        eprintln!(
                            "WARNING: SHIFT: requested {} but only {} parameters available",
                            count, actual_shift
                        );
                    }

                    eprintln!(
                        "SHIFT: shifted {} parameter(s), {} remaining",
                        actual_shift,
                        args.len()
                    );
                }
            } else {
                eprintln!("WARNING: SHIFT: no parameters to shift");
            }
        } else {
            eprintln!("WARNING: SHIFT: not in a subroutine");
        }
    }
}
