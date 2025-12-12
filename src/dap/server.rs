use super::protocol::{DapMessage, DapMessageContent};
use crate::debugger::{CmdSession, DebugContext, RunMode};
use crate::executor;
use crate::parser::{self, PreprocessResult};
use serde_json::{json, Value};
use std::collections::HashMap;
use std::io::{self, BufRead, Read};
use std::sync::mpsc::{channel, Receiver, TryRecvError};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;
struct MessageReader {
    receiver: Option<Receiver<Option<DapMessage>>>,
}

impl MessageReader {
    fn new() -> Self {
        Self { receiver: None }
    }

    fn start_read(&mut self) {
        let (tx, rx) = channel();
        self.receiver = Some(rx);

        thread::spawn(move || {
            let stdin = io::stdin();
            let mut handle = stdin.lock();

            let mut content_length = 0;
            let mut lines = handle.by_ref().lines();

            loop {
                match lines.next() {
                    Some(Ok(line)) => {
                        if line.is_empty() || line == "\r" {
                            break;
                        }
                        if line.starts_with("Content-Length:") {
                            content_length = line[15..].trim().parse().unwrap_or(0);
                        }
                    }
                    _ => {
                        let _ = tx.send(None);
                        return;
                    }
                }
            }

            if content_length > 0 {
                let mut buffer = vec![0u8; content_length];
                drop(lines);
                if handle.read_exact(&mut buffer).is_ok() {
                    if let Ok(msg) = serde_json::from_slice(&buffer) {
                        let _ = tx.send(Some(msg));
                        return;
                    }
                }
            }

            let _ = tx.send(None);
        });
    }

    fn try_receive(&mut self) -> Option<Option<DapMessage>> {
        if let Some(ref rx) = self.receiver {
            match rx.try_recv() {
                Ok(msg) => {
                    self.receiver = None;
                    Some(msg)
                }
                Err(TryRecvError::Empty) => None,
                Err(TryRecvError::Disconnected) => {
                    self.receiver = None;
                    Some(None)
                }
            }
        } else {
            None
        }
    }
}

pub struct DapServer {
    seq: u64,
    context: Option<Arc<Mutex<DebugContext>>>,
    preprocessed: Option<PreprocessResult>,
    labels: Option<HashMap<String, usize>>,
    breakpoints: HashMap<String, Vec<usize>>,
    program_path: Option<String>,
    pub event_receiver: Option<Receiver<(String, usize)>>,
    pub output_receiver: Option<Receiver<String>>,
    message_reader: MessageReader,
    watch_expressions: Vec<String>,
}

impl DapServer {
    pub fn new() -> Self {
        Self {
            seq: 0,
            context: None,
            preprocessed: None,
            labels: None,
            breakpoints: HashMap::new(),
            program_path: None,
            event_receiver: None,
            watch_expressions: Vec::new(),
            output_receiver: None,
            message_reader: MessageReader::new(),
        }
    }

    fn next_seq(&mut self) -> u64 {
        self.seq += 1;
        self.seq
    }

    /// Add a watch expression
    pub fn add_watch(&mut self, expression: String) {
        if !self.watch_expressions.contains(&expression) {
            self.watch_expressions.push(expression);
        }
    }

    /// Remove a watch expression
    pub fn remove_watch(&mut self, expression: &str) {
        self.watch_expressions.retain(|e| e != expression);
    }

    /// Get all watch expressions
    pub fn get_watches(&self) -> &[String] {
        &self.watch_expressions
    }

    /// Set the debug context (for testing)
    pub fn set_context(&mut self, context: Arc<Mutex<DebugContext>>) {
        self.context = Some(context);
    }

    /// Get a reference to the context (for testing)
    pub fn get_context(&self) -> Option<&Arc<Mutex<DebugContext>>> {
        self.context.as_ref()
    }

    pub fn send_response(
        &mut self,
        request_seq: u64,
        command: String,
        success: bool,
        body: Option<Value>,
    ) {
        let msg = DapMessage {
            seq: self.next_seq(),
            msg_type: "response".to_string(),
            content: DapMessageContent::Response {
                request_seq,
                success,
                command,
                message: None,
                body,
            },
        };
        self.send_message(&msg);
    }

    pub fn send_event(&mut self, event: String, body: Option<Value>) {
        let msg = DapMessage {
            seq: self.next_seq(),
            msg_type: "event".to_string(),
            content: DapMessageContent::Event { event, body },
        };
        self.send_message(&msg);
    }

    pub fn send_output(&mut self, output: &str, category: &str) {
        if output.is_empty() {
            return;
        }
        self.send_event(
            "output".to_string(),
            Some(json!({
                "category": category,
                "output": output
            })),
        );
    }

    fn send_message(&self, msg: &DapMessage) {
        let json = serde_json::to_string(msg).unwrap();
        let content_length = json.len();

        let output = format!("Content-Length: {}\r\n\r\n{}", content_length, json);
        print!("{}", output);

        use std::io::Write;
        let _ = std::io::stdout().flush();

        eprintln!("SENT: {} bytes", content_length);
    }

    // BESKED TIL MIG SELV:
    // Read message virker ikke af en eller anden grund, jeg har rewrited den til try read message, men der er nogle ting jeg gerne vil debugge ved den her funktion i fremtiden:)
    pub fn read_message(&self) -> Option<DapMessage> {
        let stdin = io::stdin();
        let mut handle = stdin.lock();

        let mut content_length = 0;
        let mut lines = handle.by_ref().lines();

        loop {
            if let Some(Ok(line)) = lines.next() {
                if line.is_empty() || line == "\r" {
                    break;
                }
                if line.starts_with("Content-Length:") {
                    content_length = line[15..].trim().parse().unwrap_or(0);
                }
            } else {
                return None;
            }
        }

        if content_length > 0 {
            let mut buffer = vec![0u8; content_length];
            drop(lines);
            if handle.read_exact(&mut buffer).is_ok() {
                if let Ok(msg) = serde_json::from_slice(&buffer) {
                    return Some(msg);
                }
            }
        }

        None
    }

    pub fn try_read_message(&mut self) -> Option<DapMessage> {
        if let Some(result) = self.message_reader.try_receive() {
            return result;
        }
        if self.message_reader.receiver.is_none() {
            self.message_reader.start_read();
        }

        None
    }

    pub fn handle_initialize(&mut self, seq: u64, command: String) {
        let body = json!({
            "supportsConfigurationDoneRequest": true,
            "supportsStepBack": false,
            "supportsStepInTargetsRequest": false,
            "supportsFunctionBreakpoints": false,
            "supportsConditionalBreakpoints": true,
            "supportsSetVariable": true,
            "supportsDataBreakpoints": true,
            "supportsEvaluateForHovers": true,
        });
        self.send_response(seq, command, true, Some(body));

        eprintln!("📋 Sending initialized event");
        self.send_event("initialized".to_string(), None);
    }

    pub fn handle_launch(&mut self, seq: u64, command: String, args: Option<Value>) {
        let program = args
            .as_ref()
            .and_then(|v| v.get("program"))
            .and_then(|v| v.as_str())
            .unwrap_or("test.bat");

        let stop_on_entry = args
            .as_ref()
            .and_then(|v| v.get("stopOnEntry"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true);

        self.program_path = Some(program.to_string());

        eprintln!("🚀 Launching batch file: {}", program);
        eprintln!("   Stop on entry: {}", stop_on_entry);

        let mut log = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open("C:\\temp\\batch-debugger-vscode.log")
            .ok();

        if let Some(ref mut f) = log {
            use std::io::Write;
            writeln!(f, "handle_launch called for: {}", program).ok();
            writeln!(f, "stop_on_entry: {}", stop_on_entry).ok();
            f.flush().ok();
        }

        match std::fs::read_to_string(program) {
            Ok(contents) => {
                let physical_lines: Vec<&str> = contents.lines().collect();
                let pre = parser::preprocess_lines(&physical_lines);
                let labels_phys = parser::build_label_map(&physical_lines);

                eprintln!("📝 Parsed {} logical lines", pre.logical.len());
                if let Some(ref mut f) = log {
                    use std::io::Write;
                    writeln!(f, "Parsed {} logical lines", pre.logical.len()).ok();
                    f.flush().ok();
                }

                match CmdSession::start() {
                    Ok(session) => {
                        eprintln!("CMD session started");
                        if let Some(ref mut f) = log {
                            use std::io::Write;
                            writeln!(f, "CMD session started successfully").ok();
                            f.flush().ok();
                        }

                        let mut ctx = DebugContext::new(session);

                        if stop_on_entry {
                            ctx.set_mode(RunMode::StepInto);
                            eprintln!("   Mode: StepInto (will stop at first line)");
                        } else {
                            ctx.set_mode(RunMode::Continue);
                            eprintln!("   Mode: Continue (will run until breakpoint)");
                        }
                        ctx.continue_requested = false;

                        let ctx_arc = Arc::new(Mutex::new(ctx));
                        self.context = Some(ctx_arc.clone());
                        self.preprocessed = Some(pre.clone());
                        self.labels = Some(labels_phys.clone());

                        self.send_response(seq, command, true, None);
                        eprintln!("SENT: Launch response");

                        let mut thread_log = std::fs::OpenOptions::new()
                            .create(true)
                            .append(true)
                            .open("C:\\temp\\batch-debugger-vscode.log")
                            .ok();

                        if let Some(ref mut f) = thread_log {
                            use std::io::Write;
                            writeln!(f, "About to spawn execution thread").ok();
                            f.flush().ok();
                        }

                        let (tx, rx) = channel::<(String, usize)>();
                        let (output_tx, output_rx) = channel::<String>();

                        self.event_receiver = Some(rx);
                        self.output_receiver = Some(output_rx);

                        let exec_ctx = ctx_arc.clone();
                        let exec_pre = pre.clone();
                        let exec_labels = labels_phys.clone();

                        thread::spawn(move || {
                            let mut tlog = std::fs::OpenOptions::new()
                                .create(true)
                                .append(true)
                                .open("C:\\temp\\batch-debugger-vscode.log")
                                .ok();

                            if let Some(ref mut f) = tlog {
                                use std::io::Write;
                                writeln!(f, "🧵 Execution thread STARTED").ok();
                                f.flush().ok();
                            }

                            eprintln!("🧵 Execution thread started");

                            match executor::run_debugger_dap(
                                exec_ctx,
                                &exec_pre,
                                &exec_labels,
                                tx,
                                output_tx,
                            ) {
                                Ok(_) => {
                                    eprintln!("✅ Execution completed successfully");
                                    if let Some(ref mut f) = tlog {
                                        use std::io::Write;
                                        writeln!(f, "✅ Execution completed successfully").ok();
                                        f.flush().ok();
                                    }
                                }
                                Err(e) => {
                                    eprintln!("ERROR: Execution error: {}", e);
                                    if let Some(ref mut f) = tlog {
                                        use std::io::Write;
                                        writeln!(f, "ERROR: Execution error: {}", e).ok();
                                        f.flush().ok();
                                    }
                                }
                            }

                            if let Some(ref mut f) = tlog {
                                use std::io::Write;
                                writeln!(f, "🧵 Execution thread EXITING").ok();
                                f.flush().ok();
                            }
                            eprintln!("🧵 Execution thread exiting");
                        });

                        if let Some(ref mut f) = log {
                            use std::io::Write;
                            writeln!(f, "Execution thread spawned, waiting for first stop").ok();
                            f.flush().ok();
                        }
                        if let Some(ref output_rx) = self.output_receiver {
                            let mut outputs = Vec::new();
                            while let Ok(output) = output_rx.try_recv() {
                                outputs.push(output);
                            }
                            for output in outputs {
                                self.send_output(&output, "stdout");
                            }
                        }
                        if let Some(ref rx) = self.event_receiver {
                            if let Ok((reason, line)) = rx.recv_timeout(Duration::from_secs(2)) {
                                if let Some(ref mut f) = log {
                                    use std::io::Write;
                                    writeln!(f, "Received first stop: {} at line {}", reason, line)
                                        .ok();
                                    f.flush().ok();
                                }

                                if reason != "terminated" {
                                    self.send_event(
                                        "stopped".to_string(),
                                        Some(json!({
                                            "reason": reason,
                                            "threadId": 1,
                                            "allThreadsStopped": true
                                        })),
                                    );
                                    eprintln!("SENT: Initial stopped event: {}", reason);
                                } else {
                                    eprintln!("WARNING: Script completed before first stop");
                                    self.send_event("terminated".to_string(), None);
                                }
                            } else {
                                if let Some(ref mut f) = log {
                                    use std::io::Write;
                                    writeln!(f, "WARNING: Timeout waiting for first stop event")
                                        .ok();
                                    f.flush().ok();
                                }
                                eprintln!("WARNING: Timeout waiting for first stop event");
                            }
                        }
                    }
                    Err(e) => {
                        eprintln!("ERROR: Failed to start CMD session: {}", e);
                        if let Some(ref mut f) = log {
                            use std::io::Write;
                            writeln!(f, "ERROR: Failed to start CMD session: {}", e).ok();
                            f.flush().ok();
                        }
                        self.send_response(seq, command, false, None);
                    }
                }
            }
            Err(e) => {
                eprintln!("ERROR: Failed to read batch file: {}", e);
                if let Some(ref mut f) = log {
                    use std::io::Write;
                    writeln!(f, "ERROR: Failed to read batch file: {}", e).ok();
                    f.flush().ok();
                }
                self.send_response(seq, command, false, None);
            }
        }
    }

    pub fn handle_set_breakpoints(&mut self, seq: u64, command: String, args: Option<Value>) {
        let source_path = args
            .as_ref()
            .and_then(|v| v.get("source"))
            .and_then(|v| v.get("path"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let breakpoints_array = args
            .as_ref()
            .and_then(|v| v.get("breakpoints"))
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();

        let mut verified_breakpoints = Vec::new();
        let mut logical_lines = Vec::new();

        eprintln!("BREAKPOINT: Setting breakpoints for: {}", source_path);

        if let Some(pre) = &self.preprocessed {
            for bp in breakpoints_array {
                if let Some(line) = bp.get("line").and_then(|v| v.as_u64()) {
                    let phys_line = (line as usize).saturating_sub(1);

                    // Extract condition if present
                    let condition = bp
                        .get("condition")
                        .and_then(|v| v.as_str())
                        .map(|s| s.to_string());

                    eprintln!(
                        "   Breakpoint request: physical line {} (0-indexed: {})",
                        line, phys_line
                    );

                    if let Some(ref cond) = condition {
                        eprintln!("   Condition: {}", cond);
                    }

                    if phys_line < pre.phys_to_logical.len() {
                        let logical_line = pre.phys_to_logical[phys_line];
                        logical_lines.push((logical_line, condition.clone()));

                        eprintln!("   Mapped to logical line {}", logical_line);
                        eprintln!("   Line content: {}", pre.logical[logical_line].text);

                        verified_breakpoints.push(json!({
                            "verified": true,
                            "line": line
                        }));
                    } else {
                        eprintln!("   Physical line {} out of range", phys_line);
                    }
                }
            }
        }

        self.breakpoints.insert(
            source_path.to_string(),
            logical_lines.iter().map(|(l, _)| *l).collect(),
        );

        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                eprintln!("   Adding {} breakpoints to context", logical_lines.len());
                for (logical_line, condition) in &logical_lines {
                    ctx.add_breakpoint_with_condition(*logical_line, condition.clone());
                    if let Some(cond) = condition {
                        eprintln!(
                            "   Added conditional breakpoint at logical line {}: {}",
                            logical_line, cond
                        );
                    } else {
                        eprintln!("   Added breakpoint at logical line {}", logical_line);
                    }
                }
            }
        }

        self.send_response(
            seq,
            command,
            true,
            Some(json!({
                "breakpoints": verified_breakpoints
            })),
        );
    }

    pub fn handle_threads(&mut self, seq: u64, command: String) {
        self.send_response(
            seq,
            command,
            true,
            Some(json!({
                "threads": [
                    {
                        "id": 1,
                        "name": "Batch Script"
                    }
                ]
            })),
        );
    }

    pub fn handle_stack_trace(&mut self, seq: u64, command: String) {
        let mut frames = Vec::new();

        let program_path = self.program_path.as_deref().unwrap_or("test.bat");
        let program_name = std::path::Path::new(program_path)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("test.bat");

        if let Some(ctx_arc) = &self.context {
            if let Ok(ctx) = ctx_arc.lock() {
                if let Some(pre) = &self.preprocessed {
                    let current_pc = ctx.current_line.unwrap_or(0);

                    let physical_line = if current_pc < pre.logical.len() {
                        pre.logical[current_pc].phys_start + 1
                    } else {
                        1
                    };

                    eprintln!(
                        "📊 Stack trace: logical PC={}, physical line={}",
                        current_pc, physical_line
                    );

                    frames.push(json!({
                        "id": 0,
                        "name": "main",
                        "line": physical_line,
                        "column": 1,
                        "source": {
                            "name": program_name,
                            "path": program_path
                        }
                    }));

                    for (i, frame) in ctx.call_stack.iter().enumerate() {
                        let return_line = frame.return_pc.saturating_sub(1);
                        if return_line < pre.logical.len() {
                            let logical = &pre.logical[return_line];
                            frames.push(json!({
                                "id": i + 1,
                                "name": format!("frame_{}", i + 1),
                                "line": logical.phys_start + 1,
                                "column": 1,
                                "source": {
                                    "name": program_name,
                                    "path": program_path
                                }
                            }));
                        }
                    }
                }
            }
        }

        self.send_response(
            seq,
            command,
            true,
            Some(json!({
                "stackFrames": frames,
                "totalFrames": frames.len()
            })),
        );
    }

    pub fn handle_scopes(&mut self, seq: u64, command: String) {
        self.send_response(
            seq,
            command,
            true,
            Some(json!({
                "scopes": [
                    {
                        "name": "Local",
                        "variablesReference": 1,
                        "expensive": false
                    },
                    {
                        "name": "Global",
                        "variablesReference": 2,
                        "expensive": false
                    },
                    {
                        "name": "Watch",
                        "variablesReference": 3,
                        "expensive": false
                    }
                ]
            })),
        );
    }

    pub fn handle_variables(&mut self, seq: u64, command: String, args: Option<Value>) {
        let var_ref = args
            .as_ref()
            .and_then(|v| v.get("variablesReference"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let mut variables = Vec::new();

        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                match var_ref {
                    1 => {
                        // Add ERRORLEVEL as a special variable
                        variables.push(json!({
                            "name": "ERRORLEVEL",
                            "value": ctx.last_exit_code.to_string(),
                            "variablesReference": 0,
                            "presentationHint": {
                                "kind": "property",
                                "attributes": ["readOnly"]
                            }
                        }));

                        let visible = ctx.get_visible_variables();
                        for (key, val) in visible {
                            variables.push(json!({
                                "name": key,
                                "value": val,
                                "variablesReference": 0
                            }));
                        }
                    }
                    2 => {
                        // Add ERRORLEVEL as a special variable
                        variables.push(json!({
                            "name": "ERRORLEVEL",
                            "value": ctx.last_exit_code.to_string(),
                            "variablesReference": 0,
                            "presentationHint": {
                                "kind": "property",
                                "attributes": ["readOnly"]
                            }
                        }));

                        for (key, val) in &ctx.variables {
                            variables.push(json!({
                                "name": key,
                                "value": val,
                                "variablesReference": 0
                            }));
                        }
                    }
                    3 => {
                        // Watch expressions
                        for watch_expr in &self.watch_expressions {
                            let value = match ctx.evaluate_expression(watch_expr) {
                                Ok(result) => result,
                                Err(e) => format!("<error: {}>", e),
                            };
                            variables.push(json!({
                                "name": watch_expr,
                                "value": value,
                                "variablesReference": 0,
                                "presentationHint": {
                                    "kind": "property"
                                }
                            }));
                        }
                    }
                    _ => {}
                }
            }
        }

        self.send_response(
            seq,
            command,
            true,
            Some(json!({
                "variables": variables
            })),
        );
    }

    pub fn handle_continue(&mut self, seq: u64, command: String) {
        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.set_mode(RunMode::Continue);
                ctx.continue_requested = true;
            }
        }
        self.send_response(
            seq,
            command,
            true,
            Some(json!({"allThreadsContinued": true})),
        );
    }

    pub fn handle_next(&mut self, seq: u64, command: String) {
        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.set_mode(RunMode::StepOver);
                ctx.continue_requested = true;
            }
        }
        self.send_response(seq, command, true, None);
    }

    pub fn handle_step_in(&mut self, seq: u64, command: String) {
        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.set_mode(RunMode::StepInto);
                ctx.continue_requested = true;
            }
        }
        self.send_response(seq, command, true, None);
    }

    pub fn handle_step_out(&mut self, seq: u64, command: String) {
        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.set_mode(RunMode::StepOut);
                ctx.continue_requested = true;
            }
        }
        self.send_response(seq, command, true, None);
    }

    pub fn handle_pause(&mut self, seq: u64, command: String) {
        if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.set_mode(RunMode::StepInto);
            }
        }

        self.send_response(seq, command, true, None);

        self.send_event(
            "stopped".to_string(),
            Some(json!({
                "reason": "pause",
                "threadId": 1,
                "allThreadsStopped": true
            })),
        );
    }

    pub fn handle_set_variable(&mut self, seq: u64, command: String, args: Option<Value>) {
        eprintln!("📝 Handling setVariable request");

        // Extract variable name and value from arguments
        let var_name = args
            .as_ref()
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let var_value = args
            .as_ref()
            .and_then(|v| v.get("value"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        // Don't allow setting ERRORLEVEL (it's read-only)
        if var_name.eq_ignore_ascii_case("ERRORLEVEL") {
            eprintln!("ERROR: Cannot set ERRORLEVEL (read-only)");
            self.send_response(
                seq,
                command,
                false,
                Some(json!({
                    "error": {
                        "id": 1,
                        "format": "ERRORLEVEL is read-only and cannot be modified"
                    }
                })),
            );
            return;
        }

        if var_name.is_empty() {
            eprintln!("ERROR: Variable name is empty");
            self.send_response(seq, command, false, None);
            return;
        }

        eprintln!("   Setting: {}={}", var_name, var_value);

        // Set the variable in the context
        let result = if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.set_variable(var_name, var_value)
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to lock context",
                ))
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No context available",
            ))
        };

        // Send response after releasing the lock
        match result {
            Ok(_) => {
                eprintln!("Variable set successfully");
                self.send_response(
                    seq,
                    command,
                    true,
                    Some(json!({
                        "value": var_value,
                        "variablesReference": 0
                    })),
                );
            }
            Err(e) => {
                eprintln!("ERROR: Failed to set variable: {}", e);
                self.send_response(seq, command, false, None);
            }
        }
    }

    pub fn handle_evaluate(&mut self, seq: u64, command: String, args: Option<Value>) {
        eprintln!("EVAL: Handling evaluate request");

        // Extract expression and context from arguments
        let expression = args
            .as_ref()
            .and_then(|v| v.get("expression"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let context = args
            .as_ref()
            .and_then(|v| v.get("context"))
            .and_then(|v| v.as_str())
            .unwrap_or("hover");

        if expression.is_empty() {
            eprintln!("ERROR: Expression is empty");
            self.send_response(seq, command, false, None);
            return;
        }

        eprintln!("   Expression: '{}', Context: {}", expression, context);

        // If context is "watch", add to watch expressions list
        if context == "watch" {
            if !self.watch_expressions.contains(&expression.to_string()) {
                self.watch_expressions.push(expression.to_string());
                eprintln!("WATCH: Added watch expression: '{}'", expression);
            }
        }

        // Evaluate the expression in the context
        let result = if let Some(ctx_arc) = &self.context {
            if let Ok(mut ctx) = ctx_arc.lock() {
                ctx.evaluate_expression(expression)
            } else {
                Err(std::io::Error::new(
                    std::io::ErrorKind::Other,
                    "Failed to lock context",
                ))
            }
        } else {
            Err(std::io::Error::new(
                std::io::ErrorKind::Other,
                "No context available",
            ))
        };

        // Send response after releasing the lock
        match result {
            Ok(value) => {
                eprintln!("Evaluation successful: '{}'", value);
                self.send_response(
                    seq,
                    command,
                    true,
                    Some(json!({
                        "result": value,
                        "variablesReference": 0
                    })),
                );
            }
            Err(e) => {
                eprintln!("ERROR: Evaluation failed: {}", e);
                self.send_response(
                    seq,
                    command,
                    false,
                    Some(json!({
                        "error": {
                            "id": 1,
                            "format": format!("Evaluation failed: {}", e)
                        }
                    })),
                );
            }
        }
    }

    pub fn handle_data_breakpoint_info(&mut self, seq: u64, command: String, args: Option<Value>) {
        eprintln!("DATA_BP: Handling dataBreakpointInfo request");

        // Extract variable name from arguments
        let variable_name = args
            .as_ref()
            .and_then(|v| v.get("name"))
            .and_then(|v| v.as_str())
            .unwrap_or("");

        if variable_name.is_empty() {
            eprintln!("ERROR: Variable name is empty");
            self.send_response(seq, command, false, None);
            return;
        }

        eprintln!("   Variable: '{}'", variable_name);

        // Return data breakpoint info
        // dataId is the variable name itself
        self.send_response(
            seq,
            command,
            true,
            Some(json!({
                "dataId": variable_name,
                "description": format!("Break when '{}' changes", variable_name),
                "accessTypes": ["write"],
                "canPersist": false
            })),
        );
    }

    pub fn handle_set_data_breakpoints(&mut self, seq: u64, command: String, args: Option<Value>) {
        eprintln!("DATA_BP: Handling setDataBreakpoints request");

        // Extract breakpoints array from arguments
        let breakpoints = args
            .as_ref()
            .and_then(|v| v.get("breakpoints"))
            .and_then(|v| v.as_array());

        let mut result_breakpoints = Vec::new();
        let success = if let Some(ctx_arc) = self.context.clone() {
            if let Ok(mut ctx) = ctx_arc.lock() {
                // Clear existing data breakpoints
                let existing: Vec<String> = ctx.get_data_breakpoints().keys().cloned().collect();
                for var_name in existing {
                    ctx.remove_data_breakpoint(&var_name);
                }

                // Add new data breakpoints
                if let Some(bps) = breakpoints {
                    for bp in bps {
                        if let Some(data_id) = bp.get("dataId").and_then(|v| v.as_str()) {
                            eprintln!("   Adding data breakpoint on: {}", data_id);
                            ctx.add_data_breakpoint(data_id.to_string());

                            result_breakpoints.push(json!({
                                "verified": true
                            }));
                        }
                    }
                }
                true
            } else {
                false
            }
        } else {
            false
        };

        if success {
            self.send_response(
                seq,
                command,
                true,
                Some(json!({
                    "breakpoints": result_breakpoints
                })),
            );
        } else {
            eprintln!("ERROR: Failed to set data breakpoints");
            self.send_response(seq, command, false, None);
        }
    }

    pub fn check_and_send_output(&mut self) {
        let mut outputs = Vec::new();
        if let Some(ref output_rx) = self.output_receiver {
            while let Ok(output) = output_rx.try_recv() {
                outputs.push(output);
            }
        }
        for output in outputs {
            self.send_output(&output, "stdout");
        }
    }
}
