use crate::debugger::{leave_context, DebugContext, Frame, RunMode};
use crate::parser::{
    normalize_whitespace, parse_for_statement, parse_if_statement, parse_redirections,
    PreprocessResult,
};
use std::collections::HashMap;
use std::io::{self, Write};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Duration;

/// Check if a command is a CMD built-in command
fn is_builtin_command(cmd: &str) -> bool {
    let cmd_upper = cmd.to_uppercase();

    // Extract just the command name (before first space or special char)
    let cmd_name = cmd_upper.split_whitespace().next().unwrap_or(&cmd_upper);

    // List of CMD built-in commands
    matches!(
        cmd_name,
        "ASSOC"
            | "BREAK"
            | "CALL"
            | "CD"
            | "CHDIR"
            | "CLS"
            | "COLOR"
            | "COPY"
            | "DATE"
            | "DEL"
            | "DIR"
            | "ECHO"
            | "ENDLOCAL"
            | "ERASE"
            | "EXIT"
            | "FOR"
            | "FTYPE"
            | "GOTO"
            | "IF"
            | "MD"
            | "MKDIR"
            | "MKLINK"
            | "MOVE"
            | "PATH"
            | "PAUSE"
            | "POPD"
            | "PROMPT"
            | "PUSHD"
            | "RD"
            | "REM"
            | "REN"
            | "RENAME"
            | "RMDIR"
            | "SET"
            | "SETLOCAL"
            | "SHIFT"
            | "START"
            | "TIME"
            | "TITLE"
            | "TYPE"
            | "VER"
            | "VERIFY"
            | "VOL"
    )
}

pub fn run_debugger_dap(
    ctx_arc: Arc<Mutex<DebugContext>>,
    pre: &PreprocessResult,
    labels_phys: &HashMap<String, usize>,
    event_tx: Sender<(String, usize)>,
    output_tx: Sender<String>,
) -> io::Result<()> {
    let mut log = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open("C:\\temp\\batch-debugger-vscode.log")
        .ok();

    if let Some(ref mut f) = log {
        writeln!(f, "run_debugger_dap: ENTRY").ok();
        writeln!(f, "  Logical lines: {}", pre.logical.len()).ok();
        f.flush().ok();
    }

    let mut pc: usize = 0;
    let mut step_depth: Option<usize> = None;

    'run: loop {
        if let Some(ref mut f) = log {
            writeln!(f, "Main loop: pc={}", pc).ok();
            f.flush().ok();
        }
        while pc >= pre.logical.len() {
            if let Some(ref mut f) = log {
                writeln!(f, "EOF reached, unwinding").ok();
                f.flush().ok();
            }

            let mut ctx = match ctx_arc.lock() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("ERROR: Failed to lock context: {}", e);
                    if let Some(ref mut f) = log {
                        writeln!(f, "ERROR: Failed to lock context: {}", e).ok();
                        f.flush().ok();
                    }
                    break 'run;
                }
            };
            match leave_context(&mut ctx.call_stack) {
                Some(next_pc) => pc = next_pc,
                None => break 'run,
            }
        }

        let ll = &pre.logical[pc];
        let raw = ll.text.as_str();
        let line = normalize_whitespace(raw.trim());
        let line_upper = line.to_uppercase();

        if let Some(ref mut f) = log {
            writeln!(f, "Processing line {}: '{}'", pc, raw).ok();
            f.flush().ok();
        }
        if line.trim().starts_with(':') {
            if let Some(ref mut f) = log {
                writeln!(f, "  Skipping label line").ok();
                f.flush().ok();
            }
            pc += 1;
            continue;
        }
        if line_upper.starts_with("REM ") || line.trim().starts_with("::") {
            if let Some(ref mut f) = log {
                writeln!(f, "  Skipping comment line").ok();
                f.flush().ok();
            }
            pc += 1;
            continue;
        }
        let should_stop = {
            if let Some(ref mut f) = log {
                writeln!(f, "  Checking if should stop...").ok();
                f.flush().ok();
            }

            let mut ctx = match ctx_arc.lock() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("ERROR: Failed to lock context: {}", e);
                    if let Some(ref mut f) = log {
                        writeln!(f, "ERROR: Failed to lock context: {}", e).ok();
                        f.flush().ok();
                    }
                    break 'run;
                }
            };

            let stop = match ctx.mode() {
                RunMode::Continue => ctx.should_stop_at(pc),
                RunMode::StepInto => true,
                RunMode::StepOver => {
                    if let Some(target_depth) = step_depth {
                        ctx.call_stack.len() <= target_depth
                    } else {
                        true
                    }
                }
                RunMode::StepOut => ctx.should_stop_at(pc),
            };

            if let Some(ref mut f) = log {
                writeln!(f, "  Should stop: {}, mode: {:?}", stop, ctx.mode()).ok();
                f.flush().ok();
            }

            stop
        };
        if should_stop {
            eprintln!(
                "DAP: Stopped at line {} (phys {}): {}",
                pc,
                ll.phys_start + 1,
                raw
            );

            if let Some(ref mut f) = log {
                writeln!(
                    f,
                    "STOPPED at line {} (phys {}): {}",
                    pc,
                    ll.phys_start + 1,
                    raw
                )
                .ok();
                f.flush().ok();
            }
            let stop_reason = {
                let ctx = match ctx_arc.lock() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("ERROR: Failed to lock context: {}", e);
                        break 'run;
                    }
                };

                match ctx.mode() {
                    RunMode::Continue => "breakpoint",
                    RunMode::StepInto | RunMode::StepOver | RunMode::StepOut => "step",
                }
            };
            if let Err(e) = event_tx.send((stop_reason.to_string(), pc)) {
                eprintln!("ERROR: Failed to send stopped event: {}", e);
                if let Some(ref mut f) = log {
                    writeln!(f, "ERROR: Failed to send stopped event: {}", e).ok();
                    f.flush().ok();
                }
                break 'run;
            }

            eprintln!("Sent stopped event: {}", stop_reason);
            if let Some(ref mut f) = log {
                writeln!(f, "Sent stopped event: {}", stop_reason).ok();
                f.flush().ok();
            }
            {
                let mut ctx = match ctx_arc.lock() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("ERROR: Failed to lock context: {}", e);
                        if let Some(ref mut f) = log {
                            writeln!(f, "ERROR: Failed to lock context: {}", e).ok();
                            f.flush().ok();
                        }
                        break 'run;
                    }
                };
                ctx.continue_requested = false;
                ctx.current_line = Some(pc);

                if let Some(ref mut f) = log {
                    writeln!(
                        f,
                        "  Reset continue_requested to false, set current_line to {}",
                        pc
                    )
                    .ok();
                    f.flush().ok();
                }
            }
            let mut wait_count = 0;
            if let Some(ref mut f) = log {
                writeln!(f, "  Entering wait loop...").ok();
                f.flush().ok();
            }

            loop {
                std::thread::sleep(Duration::from_millis(50));
                wait_count += 1;

                if wait_count % 20 == 0 {
                    if let Some(ref mut f) = log {
                        writeln!(f, "  Still waiting... ({} iterations)", wait_count).ok();
                        f.flush().ok();
                    }
                }
                if wait_count > 6000 {
                    eprintln!("Timeout waiting for step command");
                    if let Some(ref mut f) = log {
                        writeln!(f, "Timeout waiting for step command").ok();
                        f.flush().ok();
                    }
                    break 'run;
                }

                let ctx = match ctx_arc.lock() {
                    Ok(c) => c,
                    Err(e) => {
                        eprintln!("ERROR: Failed to lock context during wait: {}", e);
                        if let Some(ref mut f) = log {
                            writeln!(f, "ERROR: Failed to lock context during wait: {}", e).ok();
                            f.flush().ok();
                        }
                        break 'run;
                    }
                };

                if ctx.continue_requested {
                    eprintln!("Continue requested, mode: {:?}", ctx.mode());
                    if let Some(ref mut f) = log {
                        writeln!(f, "Continue requested, mode: {:?}", ctx.mode()).ok();
                        f.flush().ok();
                    }
                    match ctx.mode() {
                        RunMode::Continue => {
                            step_depth = None;
                        }
                        RunMode::StepOver => {
                            step_depth = Some(ctx.call_stack.len());
                        }
                        RunMode::StepInto => {
                            step_depth = None;
                        }
                        RunMode::StepOut => {
                            step_depth = None;
                        }
                    }
                    break;
                }
            }

            if let Some(ref mut f) = log {
                writeln!(f, "  Exited wait loop, continuing execution").ok();
                f.flush().ok();
            }
        }
        {
            if let Some(ref mut f) = log {
                writeln!(f, "  Executing line: '{}'", line).ok();
                f.flush().ok();
            }

            let mut ctx = match ctx_arc.lock() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("ERROR: Failed to lock context for execution: {}", e);
                    if let Some(ref mut f) = log {
                        writeln!(f, "ERROR: Failed to lock context for execution: {}", e).ok();
                        f.flush().ok();
                    }
                    break 'run;
                }
            };
            if line_upper.starts_with("SETLOCAL") {
                ctx.handle_setlocal();
                let (out, code) = ctx.run_command(&line)?;
                if !out.trim().is_empty() {
                    if let Err(e) = output_tx.send(out.clone()) {
                        eprintln!("ERROR: Failed to send output: {}", e);
                    }
                }
                ctx.last_exit_code = code;
                pc += 1;
                continue;
            }
            if line_upper.starts_with("ENDLOCAL") {
                ctx.handle_endlocal();
                let (out, code) = ctx.run_command(&line)?;
                if !out.trim().is_empty() {
                    if let Err(e) = output_tx.send(out.clone()) {
                        eprintln!("ERROR: Failed to send output: {}", e);
                    }
                }
                ctx.last_exit_code = code;
                pc += 1;
                continue;
            }
            if line_upper.starts_with("CALL ") {
                let rest = &line[5..].trim();
                let mut lexer = shlex::Shlex::new(rest);
                let first = lexer.next().unwrap_or_default();
                let label_key = first.trim_start_matches(':').to_lowercase();
                let args: Vec<String> = lexer.collect();

                if let Some(&phys_target) = labels_phys.get(&label_key) {
                    let logical_target = pre.phys_to_logical[phys_target];
                    ctx.call_stack.push(Frame::new(pc + 1, Some(args)));
                    pc = logical_target;
                } else {
                    eprintln!("ERROR: CALL to unknown label: {}", label_key);
                    break 'run;
                }
                continue;
            }
            if line_upper.starts_with("EXIT /B") {
                let rest = &line[7..].trim();
                let code: i32 = rest.parse::<i32>().unwrap_or(0);
                ctx.last_exit_code = code;

                match leave_context(&mut ctx.call_stack) {
                    Some(next_pc) => pc = next_pc,
                    None => break 'run,
                }
                continue;
            }
            if line_upper.starts_with("GOTO ") {
                let rest = &line[5..].trim();
                let label_key = rest
                    .trim_start_matches(':')
                    .split_whitespace()
                    .next()
                    .unwrap_or("")
                    .to_lowercase();

                if label_key == "eof" {
                    match leave_context(&mut ctx.call_stack) {
                        Some(next_pc) => pc = next_pc,
                        None => break 'run,
                    }
                    continue;
                }

                if let Some(&phys_target) = labels_phys.get(&label_key) {
                    let logical_target = pre.phys_to_logical[phys_target];
                    pc = logical_target;
                } else {
                    eprintln!("ERROR: GOTO to unknown label: {}", label_key);
                    break 'run;
                }
                continue;
            }
            if line_upper.starts_with("PUSHD") {
                let rest = line[5..].trim();
                let path = if rest.is_empty() { None } else { Some(rest) };
                if let Err(e) = ctx.handle_pushd(path) {
                    eprintln!("ERROR: PUSHD error: {}", e);
                }
                pc += 1;
                continue;
            }
            if line_upper.starts_with("POPD") {
                if let Err(e) = ctx.handle_popd() {
                    eprintln!("ERROR: POPD error: {}", e);
                }
                pc += 1;
                continue;
            }
            if line_upper.starts_with("SHIFT") {
                let rest = line[5..].trim();
                let count = if rest.is_empty() {
                    1
                } else {
                    rest.split_whitespace()
                        .next()
                        .and_then(|s| s.trim_start_matches('/').parse().ok())
                        .unwrap_or(1)
                };
                ctx.handle_shift(count);
                pc += 1;
                continue;
            }
            // Check if this is a FOR loop and expand it for stepping
            if line_upper.starts_with("FOR ") {
                if let Some(for_stmt) = parse_for_statement(&line) {
                    eprintln!("FOR: Loop detected, expanding iterations...");

                    match ctx.expand_for_loop(&for_stmt.loop_type) {
                        Ok(iterations) => {
                            eprintln!("FOR: Loop expanded into {} iterations", iterations.len());

                            if let Err(e) = output_tx
                                .send(format!("FOR: Loop: {} iterations\r\n", iterations.len()))
                            {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }

                            // Execute each iteration
                            for (idx, (command, var_name, var_value)) in
                                iterations.iter().enumerate()
                            {
                                eprintln!("  Iteration {}: {}={}", idx + 1, var_name, var_value);

                                // Update loop variable
                                ctx.set_loop_variable(var_name, var_value);

                                // Send iteration info to debug console
                                if let Err(e) = output_tx.send(format!(
                                    "  [{}] {}={}\r\n",
                                    idx + 1,
                                    var_name,
                                    var_value
                                )) {
                                    eprintln!("ERROR: Failed to send output: {}", e);
                                }

                                // Track SET commands in the iteration
                                ctx.track_set_command(command);

                                // Execute the command
                                match ctx.run_command(command) {
                                    Ok((out, code)) => {
                                        if !out.trim().is_empty() {
                                            if let Err(e) = output_tx.send(out.clone()) {
                                                eprintln!("ERROR: Failed to send output: {}", e);
                                            }
                                        }
                                        ctx.last_exit_code = code;
                                    }
                                    Err(e) => {
                                        eprintln!(
                                            "ERROR: Command execution error in FOR loop: {}",
                                            e
                                        );
                                        if let Err(e) = output_tx.send(format!(
                                            "ERROR: Error in iteration {}: {}\r\n",
                                            idx + 1,
                                            e
                                        )) {
                                            eprintln!("ERROR: Failed to send error output: {}", e);
                                        }
                                        // Continue to next iteration instead of breaking
                                    }
                                }
                            }

                            // Skip the FOR loop line itself and continue
                            pc += 1;
                            continue;
                        }
                        Err(e) => {
                            eprintln!("ERROR: FOR loop expansion error: {}", e);
                            if let Err(e) = output_tx
                                .send(format!("ERROR: FOR loop expansion error: {}\r\n", e))
                            {
                                eprintln!("ERROR: Failed to send error output: {}", e);
                            }
                        }
                    }
                }
            }

            // Check if this is an IF statement and pre-evaluate the condition
            if line_upper.starts_with("IF ") {
                if let Some(if_stmt) = parse_if_statement(&line) {
                    match ctx.evaluate_if_condition(&if_stmt.condition) {
                        Ok(condition_result) => {
                            if condition_result {
                                eprintln!("IF: Condition is TRUE -> will execute THEN branch");
                                if let Err(e) = output_tx.send(
                                    "IF: Condition is TRUE -> executing THEN branch\r\n"
                                        .to_string(),
                                ) {
                                    eprintln!("ERROR: Failed to send output: {}", e);
                                }
                            } else {
                                eprintln!("IF: Condition is FALSE -> will skip THEN branch");
                                if let Err(e) = output_tx.send(
                                    "IF: Condition is FALSE -> skipping THEN branch\r\n"
                                        .to_string(),
                                ) {
                                    eprintln!("ERROR: Failed to send output: {}", e);
                                }
                            }
                        }
                        Err(e) => {
                            eprintln!("WARNING: Failed to evaluate IF condition: {}", e);
                        }
                    }
                }
            }

            // Parse and display redirections
            let cmd_with_redirections = parse_redirections(&line);

            // Detect if command is built-in or external
            let base_cmd = cmd_with_redirections.base_command.trim();
            let is_builtin = is_builtin_command(base_cmd);
            let cmd_type = if is_builtin { "built-in" } else { "external" };

            if !cmd_with_redirections.redirections.is_empty() {
                eprintln!("Executing {} command: {}", cmd_type, line);
                for redir in &cmd_with_redirections.redirections {
                    match redir.operator.as_str() {
                        ">" => {
                            eprintln!("  |-- Output redirected to: {} (overwrite)", redir.target);
                            if let Err(e) = output_tx.send(format!(
                                "  |-- Output redirected to: {} (overwrite)\r\n",
                                redir.target
                            )) {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }
                        }
                        ">>" => {
                            eprintln!("  |-- Output redirected to: {} (append)", redir.target);
                            if let Err(e) = output_tx.send(format!(
                                "  |-- Output redirected to: {} (append)\r\n",
                                redir.target
                            )) {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }
                        }
                        "<" => {
                            eprintln!("  |-- Input redirected from: {}", redir.target);
                            if let Err(e) = output_tx
                                .send(format!("  |-- Input redirected from: {}\r\n", redir.target))
                            {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }
                        }
                        "2>" => {
                            eprintln!("  |-- Error output redirected to: {}", redir.target);
                            if let Err(e) = output_tx.send(format!(
                                "  |-- Error output redirected to: {}\r\n",
                                redir.target
                            )) {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }
                        }
                        "2>&1" => {
                            eprintln!("  |-- Error output redirected to stdout");
                            if let Err(e) = output_tx
                                .send("  |-- Error output redirected to stdout\r\n".to_string())
                            {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }
                        }
                        "|" => {
                            eprintln!("  |-- Piped to: {}", redir.target);
                            if let Err(e) =
                                output_tx.send(format!("  |-- Piped to: {}\r\n", redir.target))
                            {
                                eprintln!("ERROR: Failed to send output: {}", e);
                            }
                        }
                        _ => {}
                    }
                }
            } else {
                eprintln!("Executing {} command: {}", cmd_type, line);
            }

            ctx.track_set_command(&line);

            if let Some(ref mut f) = log {
                writeln!(f, "  About to run_command: '{}'", line).ok();
                f.flush().ok();
            }

            match ctx.run_command(&line) {
                Ok((out, code)) => {
                    if let Some(ref mut f) = log {
                        writeln!(f, "  Command executed, exit code: {}", code).ok();
                        f.flush().ok();
                    }

                    if !out.trim().is_empty() {
                        if let Err(e) = output_tx.send(out.clone()) {
                            eprintln!("ERROR: Failed to send output: {}", e);
                            if let Some(ref mut f) = log {
                                writeln!(f, "ERROR: Failed to send output: {}", e).ok();
                                f.flush().ok();
                            }
                        }
                    }
                    ctx.last_exit_code = code;

                    // Check for data breakpoint hits after command execution
                    if ctx.check_data_breakpoints() {
                        eprintln!("BREAK: Data breakpoint triggered, pausing execution");
                        if let Some(ref mut f) = log {
                            writeln!(f, "BREAK: Data breakpoint triggered").ok();
                            f.flush().ok();
                        }
                        // Send stopped event
                        let _ = event_tx.send(("stopped".to_string(), pc));
                        // Update data breakpoint values for next iteration
                        ctx.update_data_breakpoints();
                        // Wait for continue
                        ctx.continue_requested = false;
                        ctx.set_mode(crate::debugger::RunMode::Continue);
                        // Continue to next iteration
                    }
                }
                Err(e) => {
                    eprintln!("ERROR: Command execution error: {}", e);
                    if let Some(ref mut f) = log {
                        writeln!(f, "ERROR: Command execution error: {}", e).ok();
                        f.flush().ok();
                    }
                    break 'run;
                }
            }
        }

        pc += 1;
    }

    eprintln!("DAP: Script execution completed");
    if let Some(ref mut f) = log {
        writeln!(f, "DAP: Script execution completed").ok();
        f.flush().ok();
    }
    let _ = event_tx.send(("terminated".to_string(), 0));

    Ok(())
}
