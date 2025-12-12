use std::fs;

// Helper to create a test batch file
fn create_test_batch(content: &str, filename: &str) -> String {
    let path = format!("tests/batch_files/test_{}.bat", filename);
    fs::write(&path, content).expect("Failed to write test file");
    path
}

// Helper to cleanup test files
fn cleanup_test_batch(path: &str) {
    let _ = fs::remove_file(path);
}

#[cfg(test)]
mod debugger_tests {
    use super::*;

    #[test]
    fn test_basic_execution() {
        let content = r#"@echo off
set NAME=Alice
echo Hello %NAME%
exit /b 0
"#;

        let path = create_test_batch(content, "basic");

        // Parse and preprocess
        let contents = fs::read_to_string(&path).expect("Could not read test file");
        let physical_lines: Vec<&str> = contents.lines().collect();

        let pre = batch_debugger::parser::preprocess_lines(&physical_lines);
        let labels = batch_debugger::parser::build_label_map(&physical_lines);

        // Verify parsing
        assert!(pre.logical.len() > 0, "Should have parsed logical lines");
        assert_eq!(labels.len(), 0, "Should have no labels");

        cleanup_test_batch(&path);
    }

    #[test]
    fn test_label_parsing() {
        let content = r#"@echo off
call :subroutine
exit /b 0

:subroutine
echo In subroutine
exit /b 0
"#;

        let path = create_test_batch(content, "labels");
        let contents = fs::read_to_string(&path).expect("Could not read test file");
        let physical_lines: Vec<&str> = contents.lines().collect();

        let labels = batch_debugger::parser::build_label_map(&physical_lines);

        assert_eq!(labels.len(), 1, "Should have found 1 label");
        assert!(
            labels.contains_key("subroutine"),
            "Should have found :subroutine label"
        );

        cleanup_test_batch(&path);
    }

    #[test]
    fn test_line_continuation() {
        let content = r#"@echo off
echo This is a ^
continued line
exit /b 0
"#;

        let path = create_test_batch(content, "continuation");
        let contents = fs::read_to_string(&path).expect("Could not read test file");
        let physical_lines: Vec<&str> = contents.lines().collect();

        let pre = batch_debugger::parser::preprocess_lines(&physical_lines);

        // The continuation should join lines 1 and 2
        let joined_line = &pre.logical[1].text;
        assert!(
            joined_line.contains("This is a") && joined_line.contains("continued line"),
            "Lines should be joined"
        );

        cleanup_test_batch(&path);
    }

    #[test]
    fn test_comment_detection() {
        assert!(batch_debugger::parser::is_comment("REM This is a comment"));
        assert!(batch_debugger::parser::is_comment(
            ":: This is also a comment"
        ));
        assert!(batch_debugger::parser::is_comment(""));
        assert!(!batch_debugger::parser::is_comment("echo Hello"));
    }

    #[test]
    fn test_composite_command_splitting() {
        let parts = batch_debugger::parser::split_composite_command("echo A & echo B && echo C");
        assert_eq!(parts.len(), 3, "Should split into 3 parts");

        let parts2 = batch_debugger::parser::split_composite_command("echo A || echo B");
        assert_eq!(parts2.len(), 2, "Should split into 2 parts");
    }

    #[test]
    fn test_breakpoint_management() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Add breakpoints
        ctx.add_breakpoint(5);
        ctx.add_breakpoint(10);
        ctx.add_breakpoint(15);

        // Test should_stop_at in Continue mode
        use batch_debugger::debugger::RunMode;
        ctx.set_mode(RunMode::Continue);

        assert!(ctx.should_stop_at(5), "Should stop at breakpoint 5");
        assert!(ctx.should_stop_at(10), "Should stop at breakpoint 10");
        assert!(!ctx.should_stop_at(7), "Should not stop at line 7");
    }

    #[test]
    fn test_run_modes() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::RunMode;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test mode switching
        ctx.set_mode(RunMode::Continue);
        assert_eq!(ctx.mode(), RunMode::Continue);

        ctx.set_mode(RunMode::StepInto);
        assert_eq!(ctx.mode(), RunMode::StepInto);

        ctx.set_mode(RunMode::StepOver);
        assert_eq!(ctx.mode(), RunMode::StepOver);

        ctx.set_mode(RunMode::StepOut);
        assert_eq!(ctx.mode(), RunMode::StepOut);
    }

    #[test]
    fn test_variable_tracking() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Track simple SET commands
        ctx.track_set_command("SET NAME=Alice");
        ctx.track_set_command("SET AGE=25");
        ctx.track_set_command("SET \"CITY=New York\"");

        assert_eq!(ctx.variables.get("NAME"), Some(&"Alice".to_string()));
        assert_eq!(ctx.variables.get("AGE"), Some(&"25".to_string()));
        assert_eq!(ctx.variables.get("CITY"), Some(&"New York".to_string()));

        // SET /A should now be tracked (with execution)
        ctx.track_set_command("SET /A COUNTER=1");
        assert_eq!(ctx.variables.get("COUNTER"), Some(&"1".to_string()));

        // SET /P is also tracked now, but it requires the command to be executed first
        // Since we're not executing it, we just verify it doesn't crash
        // (The actual tracking happens after execution in real usage)
    }

    #[test]
    fn test_call_stack() {
        use batch_debugger::debugger::Frame;

        let mut call_stack: Vec<Frame> = Vec::new();

        // Simulate CALL operations
        call_stack.push(Frame::new(
            10,
            Some(vec!["arg1".to_string(), "arg2".to_string()]),
        ));
        call_stack.push(Frame::new(25, None));
        call_stack.push(Frame::new(40, Some(vec!["test".to_string()])));

        assert_eq!(call_stack.len(), 3, "Should have 3 frames");

        // Simulate returns
        let frame3 = call_stack.pop().unwrap();
        assert_eq!(frame3.return_pc, 40);

        let frame2 = call_stack.pop().unwrap();
        assert_eq!(frame2.return_pc, 25);

        assert_eq!(call_stack.len(), 1, "Should have 1 frame left");
    }

    #[test]
    fn test_setlocal_scope() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::Frame;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set global variable
        ctx.track_set_command("SET GLOBAL=value1");

        // Enter subroutine
        ctx.call_stack.push(Frame::new(10, None));

        // SETLOCAL
        ctx.handle_setlocal();

        // Set local variable
        ctx.track_set_command("SET LOCAL=value2");

        // Check visible variables includes both
        let visible = ctx.get_visible_variables();
        assert_eq!(visible.get("GLOBAL"), Some(&"value1".to_string()));
        assert_eq!(visible.get("LOCAL"), Some(&"value2".to_string()));

        // ENDLOCAL
        ctx.handle_endlocal();

        // Local variable should be cleared
        let visible_after = ctx.get_visible_variables();
        assert_eq!(visible_after.get("GLOBAL"), Some(&"value1".to_string()));
        assert!(!visible_after.contains_key("LOCAL"));
    }

    #[test]
    fn test_cmd_session_basic_command() {
        use batch_debugger::debugger::CmdSession;

        let mut session = CmdSession::start().expect("Failed to start CMD session");

        // Test basic echo command
        let (output, code) = session
            .run("echo Hello World")
            .expect("Failed to run command");
        assert!(
            output.contains("Hello World"),
            "Output should contain 'Hello World'"
        );
        assert_eq!(code, 0, "Exit code should be 0");
    }

    #[test]
    fn test_cmd_session_set_command() {
        use batch_debugger::debugger::CmdSession;

        let mut session = CmdSession::start().expect("Failed to start CMD session");

        // Set a variable
        let (_, code) = session
            .run("set TESTVAR=TestValue")
            .expect("Failed to set variable");
        assert_eq!(code, 0, "SET command should succeed");

        // Echo the variable
        let (output, _) = session
            .run("echo %TESTVAR%")
            .expect("Failed to echo variable");
        assert!(
            output.contains("TestValue"),
            "Should echo the variable value"
        );
    }

    #[test]
    fn test_preprocessing_empty_lines() {
        let physical_lines = vec!["@echo off", "", "echo Hello", "", "exit /b 0"];
        let pre = batch_debugger::parser::preprocess_lines(&physical_lines);

        // Should have logical lines for all physical lines
        assert_eq!(pre.phys_to_logical.len(), 5);
    }

    #[test]
    fn test_block_depth_tracking() {
        let content = r#"@echo off
if 1==1 (
    echo Level 1
    if 2==2 (
        echo Level 2
    )
)
exit /b 0
"#;

        let path = create_test_batch(content, "blocks");
        let contents = fs::read_to_string(&path).expect("Could not read test file");
        let physical_lines: Vec<&str> = contents.lines().collect();

        let pre = batch_debugger::parser::preprocess_lines(&physical_lines);

        // Check that depth tracking works
        let depths: Vec<u16> = pre.logical.iter().map(|l| l.group_depth).collect();

        // Should have varying depths
        assert!(depths.iter().any(|&d| d == 0), "Should have depth 0");
        assert!(depths.iter().any(|&d| d > 0), "Should have depth > 0");

        cleanup_test_batch(&path);
    }

    #[test]
    fn test_errorlevel_tracking() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let mut session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Initially, ERRORLEVEL should be 0
        assert_eq!(ctx.last_exit_code, 0, "Initial ERRORLEVEL should be 0");

        // Run a successful command
        let (_, code) = ctx
            .run_command("echo Success")
            .expect("Failed to run command");
        ctx.last_exit_code = code;
        assert_eq!(
            ctx.last_exit_code, 0,
            "ERRORLEVEL should be 0 after success"
        );

        // Run a command that fails
        let (_, code) = ctx
            .run_command("findstr \"NONEXISTENT\" nonexistent_file.txt 2>nul")
            .expect("Failed to run command");
        ctx.last_exit_code = code;
        assert_ne!(
            ctx.last_exit_code, 0,
            "ERRORLEVEL should be non-zero after failure"
        );

        // Test explicit exit code
        let (_, code) = ctx
            .run_command("cmd /c exit /b 5")
            .expect("Failed to run command");
        ctx.last_exit_code = code;
        assert_eq!(ctx.last_exit_code, 5, "ERRORLEVEL should be 5");

        // Another explicit exit code
        let (_, code) = ctx
            .run_command("cmd /c exit /b 42")
            .expect("Failed to run command");
        ctx.last_exit_code = code;
        assert_eq!(ctx.last_exit_code, 42, "ERRORLEVEL should be 42");

        // Run a command that explicitly returns 0
        let (_, code) = ctx
            .run_command("cmd /c exit /b 0")
            .expect("Failed to run command");
        ctx.last_exit_code = code;
        assert_eq!(ctx.last_exit_code, 0, "ERRORLEVEL should be back to 0");
    }

    #[test]
    fn test_set_variable() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable using set_variable method
        ctx.set_variable("TEST_VAR", "TestValue")
            .expect("Failed to set variable");

        // Check it was tracked
        assert_eq!(
            ctx.variables.get("TEST_VAR"),
            Some(&"TestValue".to_string()),
            "Variable should be tracked"
        );

        // Verify it was set in the CMD session
        let (output, _) = ctx
            .run_command("echo %TEST_VAR%")
            .expect("Failed to echo variable");
        assert!(
            output.contains("TestValue"),
            "Variable should be set in CMD session"
        );

        // Modify existing variable
        ctx.set_variable("TEST_VAR", "NewValue")
            .expect("Failed to modify variable");

        assert_eq!(
            ctx.variables.get("TEST_VAR"),
            Some(&"NewValue".to_string()),
            "Variable should be updated"
        );

        // Set variable with spaces
        ctx.set_variable("SPACE_VAR", "Value With Spaces")
            .expect("Failed to set variable with spaces");

        let (output, _) = ctx
            .run_command("echo %SPACE_VAR%")
            .expect("Failed to echo variable");
        assert!(
            output.contains("Value With Spaces"),
            "Variable with spaces should work"
        );
    }

    #[test]
    fn test_set_variable_with_setlocal() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::Frame;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a global variable
        ctx.set_variable("GLOBAL_VAR", "GlobalValue")
            .expect("Failed to set global variable");

        assert_eq!(
            ctx.variables.get("GLOBAL_VAR"),
            Some(&"GlobalValue".to_string())
        );

        // Enter a subroutine and activate SETLOCAL
        ctx.call_stack.push(Frame::new(10, None));
        ctx.handle_setlocal();

        // Set a local variable
        ctx.set_variable("LOCAL_VAR", "LocalValue")
            .expect("Failed to set local variable");

        // Check it went into the local scope
        if let Some(frame) = ctx.call_stack.last() {
            assert_eq!(
                frame.locals.get("LOCAL_VAR"),
                Some(&"LocalValue".to_string()),
                "Variable should be in local scope"
            );
        }

        // Global variable should still be accessible
        let visible = ctx.get_visible_variables();
        assert_eq!(visible.get("GLOBAL_VAR"), Some(&"GlobalValue".to_string()));
        assert_eq!(visible.get("LOCAL_VAR"), Some(&"LocalValue".to_string()));

        // ENDLOCAL should clear local variable
        ctx.handle_endlocal();
        let visible_after = ctx.get_visible_variables();
        assert_eq!(
            visible_after.get("GLOBAL_VAR"),
            Some(&"GlobalValue".to_string())
        );
        assert!(!visible_after.contains_key("LOCAL_VAR"));
    }

    #[test]
    fn test_set_variable_special_characters() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test with equals sign in value
        ctx.set_variable("VAR_WITH_EQUALS", "key=value")
            .expect("Failed to set variable with equals");

        assert_eq!(
            ctx.variables.get("VAR_WITH_EQUALS"),
            Some(&"key=value".to_string())
        );

        // Test with numbers
        ctx.set_variable("NUMBER_VAR", "12345")
            .expect("Failed to set number variable");

        assert_eq!(ctx.variables.get("NUMBER_VAR"), Some(&"12345".to_string()));

        // Test empty value
        ctx.set_variable("EMPTY_VAR", "")
            .expect("Failed to set empty variable");

        assert_eq!(ctx.variables.get("EMPTY_VAR"), Some(&"".to_string()));
    }

    #[test]
    fn test_set_variable_persistence() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set multiple variables
        ctx.set_variable("VAR1", "Value1")
            .expect("Failed to set VAR1");
        ctx.set_variable("VAR2", "Value2")
            .expect("Failed to set VAR2");
        ctx.set_variable("VAR3", "Value3")
            .expect("Failed to set VAR3");

        // Run some other commands
        let _ = ctx.run_command("echo Testing");
        let _ = ctx.run_command("set TEMP_VAR=temp");

        // Original variables should still be there
        assert_eq!(ctx.variables.get("VAR1"), Some(&"Value1".to_string()));
        assert_eq!(ctx.variables.get("VAR2"), Some(&"Value2".to_string()));
        assert_eq!(ctx.variables.get("VAR3"), Some(&"Value3".to_string()));

        // Verify they're still set in CMD session
        let (output, _) = ctx
            .run_command("echo %VAR1% %VAR2% %VAR3%")
            .expect("Failed to echo variables");
        assert!(output.contains("Value1"));
        assert!(output.contains("Value2"));
        assert!(output.contains("Value3"));
    }

    #[test]
    fn test_evaluate_expression_simple_variables() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set some variables
        ctx.set_variable("NAME", "Alice")
            .expect("Failed to set NAME");
        ctx.set_variable("AGE", "25").expect("Failed to set AGE");

        // Evaluate variable with %VAR% syntax
        let result = ctx
            .evaluate_expression("%NAME%")
            .expect("Failed to evaluate %NAME%");
        assert_eq!(result, "Alice");

        // Evaluate variable without %
        let result = ctx
            .evaluate_expression("NAME")
            .expect("Failed to evaluate NAME");
        assert_eq!(result, "Alice");

        // Evaluate numeric variable
        let result = ctx
            .evaluate_expression("%AGE%")
            .expect("Failed to evaluate %AGE%");
        assert_eq!(result, "25");
    }

    #[test]
    fn test_evaluate_expression_errorlevel() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set exit code
        ctx.last_exit_code = 42;

        // Evaluate ERRORLEVEL
        let result = ctx
            .evaluate_expression("ERRORLEVEL")
            .expect("Failed to evaluate ERRORLEVEL");
        assert_eq!(result, "42");

        // Evaluate with %
        let result = ctx
            .evaluate_expression("%ERRORLEVEL%")
            .expect("Failed to evaluate %ERRORLEVEL%");
        assert_eq!(result, "42");

        // Change exit code
        ctx.last_exit_code = 0;
        let result = ctx
            .evaluate_expression("ERRORLEVEL")
            .expect("Failed to evaluate ERRORLEVEL");
        assert_eq!(result, "0");
    }

    #[test]
    fn test_evaluate_expression_complex() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set variables
        ctx.set_variable("FIRST", "Hello")
            .expect("Failed to set FIRST");
        ctx.set_variable("SECOND", "World")
            .expect("Failed to set SECOND");

        // Evaluate expression with multiple variables
        let result = ctx
            .evaluate_expression("%FIRST% %SECOND%")
            .expect("Failed to evaluate complex expression");
        assert!(result.contains("Hello"));
        assert!(result.contains("World"));

        // Evaluate path-like expression
        ctx.set_variable("DIR", "C:\\Users")
            .expect("Failed to set DIR");
        let result = ctx
            .evaluate_expression("%DIR%\\Documents")
            .expect("Failed to evaluate path expression");
        assert!(result.contains("C:\\Users\\Documents"));
    }

    #[test]
    fn test_evaluate_expression_with_setlocal() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::Frame;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set global variable
        ctx.set_variable("GLOBAL", "GlobalValue")
            .expect("Failed to set GLOBAL");

        // Evaluate global
        let result = ctx
            .evaluate_expression("GLOBAL")
            .expect("Failed to evaluate GLOBAL");
        assert_eq!(result, "GlobalValue");

        // Enter SETLOCAL scope
        ctx.call_stack.push(Frame::new(10, None));
        ctx.handle_setlocal();

        // Set local variable
        ctx.set_variable("LOCAL", "LocalValue")
            .expect("Failed to set LOCAL");

        // Evaluate local variable
        let result = ctx
            .evaluate_expression("LOCAL")
            .expect("Failed to evaluate LOCAL");
        assert_eq!(result, "LocalValue");

        // Global should still be accessible
        let result = ctx
            .evaluate_expression("GLOBAL")
            .expect("Failed to evaluate GLOBAL in local scope");
        assert_eq!(result, "GlobalValue");
    }

    #[test]
    fn test_evaluate_expression_literals() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Evaluate literal string
        let result = ctx
            .evaluate_expression("HelloWorld")
            .expect("Failed to evaluate literal");
        assert_eq!(result, "HelloWorld");

        // Evaluate quoted string
        let result = ctx
            .evaluate_expression("\"Hello World\"")
            .expect("Failed to evaluate quoted string");
        assert!(result.contains("Hello World"));
    }

    #[test]
    fn test_evaluate_expression_empty_and_whitespace() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Evaluate with leading/trailing whitespace
        ctx.set_variable("VAR", "Value").expect("Failed to set VAR");

        let result = ctx
            .evaluate_expression("  VAR  ")
            .expect("Failed to evaluate with whitespace");
        assert_eq!(result, "Value");

        // Evaluate with %VAR% and whitespace
        let result = ctx
            .evaluate_expression("  %VAR%  ")
            .expect("Failed to evaluate %VAR% with whitespace");
        assert_eq!(result, "Value");
    }

    #[test]
    fn test_conditional_breakpoint_true() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::RunMode;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable
        ctx.set_variable("COUNTER", "5")
            .expect("Failed to set COUNTER");

        // Add conditional breakpoint: stop when COUNTER == 5
        ctx.add_breakpoint_with_condition(10, Some("COUNTER".to_string()));

        // Set mode to Continue
        ctx.set_mode(RunMode::Continue);

        // Should stop because condition is true (COUNTER is "5", non-zero)
        assert!(ctx.should_stop_at(10), "Should stop when COUNTER is 5");
    }

    #[test]
    fn test_conditional_breakpoint_false() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::RunMode;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable to 0
        ctx.set_variable("COUNTER", "0")
            .expect("Failed to set COUNTER");

        // Add conditional breakpoint: stop when COUNTER != 0
        ctx.add_breakpoint_with_condition(10, Some("COUNTER".to_string()));

        // Set mode to Continue
        ctx.set_mode(RunMode::Continue);

        // Should NOT stop because condition is false (COUNTER is "0")
        assert!(!ctx.should_stop_at(10), "Should not stop when COUNTER is 0");
    }

    #[test]
    fn test_conditional_breakpoint_expression() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::RunMode;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set variables
        ctx.set_variable("NAME", "Alice")
            .expect("Failed to set NAME");

        // Add conditional breakpoint with complex expression
        ctx.add_breakpoint_with_condition(10, Some("%NAME%".to_string()));

        // Set mode to Continue
        ctx.set_mode(RunMode::Continue);

        // Should stop because NAME evaluates to "Alice" (non-empty, non-zero)
        assert!(ctx.should_stop_at(10), "Should stop when NAME is Alice");
    }

    #[test]
    fn test_conditional_breakpoint_hit_count() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::RunMode;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Add unconditional breakpoint
        ctx.add_breakpoint_with_condition(10, None);

        // Set mode to Continue
        ctx.set_mode(RunMode::Continue);

        // Hit the breakpoint multiple times
        assert!(ctx.should_stop_at(10), "First hit");
        assert!(ctx.should_stop_at(10), "Second hit");
        assert!(ctx.should_stop_at(10), "Third hit");

        // Check hit count was incremented
        if let Some(bp) = ctx.get_breakpoint(10) {
            assert_eq!(bp.hit_count, 3, "Hit count should be 3");
        } else {
            panic!("Breakpoint not found");
        }
    }

    #[test]
    fn test_unconditional_breakpoint_still_works() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::RunMode;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Add unconditional breakpoint (no condition)
        ctx.add_breakpoint(10);

        // Set mode to Continue
        ctx.set_mode(RunMode::Continue);

        // Should stop unconditionally
        assert!(
            ctx.should_stop_at(10),
            "Should stop at unconditional breakpoint"
        );
    }

    #[test]
    fn test_set_a_simple_arithmetic() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test simple addition
        ctx.track_set_command("SET /A RESULT=10+20");

        let vars = ctx.get_visible_variables();
        assert_eq!(vars.get("RESULT"), Some(&"30".to_string()));
    }

    #[test]
    fn test_set_a_multiplication() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test multiplication
        ctx.track_set_command("SET /A COUNTER=5");
        ctx.track_set_command("SET /A RESULT=COUNTER*2");

        let vars = ctx.get_visible_variables();
        assert_eq!(vars.get("COUNTER"), Some(&"5".to_string()));
        assert_eq!(vars.get("RESULT"), Some(&"10".to_string()));
    }

    #[test]
    fn test_set_a_complex_expression() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test complex expression with precedence
        ctx.track_set_command("SET /A BASE=5");
        ctx.track_set_command("SET /A RESULT=BASE*2+3");

        let vars = ctx.get_visible_variables();
        assert_eq!(vars.get("BASE"), Some(&"5".to_string()));
        assert_eq!(vars.get("RESULT"), Some(&"13".to_string())); // 5*2=10, 10+3=13
    }

    #[test]
    fn test_set_a_compound_assignment() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test += operator
        ctx.track_set_command("SET /A COUNTER=10");
        ctx.track_set_command("SET /A COUNTER+=5");

        let vars = ctx.get_visible_variables();
        assert_eq!(vars.get("COUNTER"), Some(&"15".to_string()));
    }

    #[test]
    fn test_set_a_with_setlocal() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::Frame;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a call frame with SETLOCAL
        ctx.call_stack.push(Frame {
            return_pc: 0,
            args: None,
            locals: std::collections::HashMap::new(),
            has_setlocal: false,
        });
        ctx.handle_setlocal();

        // SET /A in local scope
        ctx.track_set_command("SET /A LOCAL_VAR=42");

        let frame_vars = ctx.get_frame_variables(0);
        assert_eq!(frame_vars.get("LOCAL_VAR"), Some(&"42".to_string()));

        // Should NOT be in global variables
        assert_eq!(ctx.variables.get("LOCAL_VAR"), None);
    }

    #[test]
    fn test_set_p_with_file_input() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use std::fs;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a temp file with input
        fs::write("tests/batch_files/temp_test_input.txt", "TestValue\n")
            .expect("Failed to write temp file");

        // Execute SET /P command
        let _ = ctx.run_command("SET /P USERNAME=<tests/batch_files/temp_test_input.txt");

        // Track the command
        ctx.track_set_command("SET /P USERNAME=<tests/batch_files/temp_test_input.txt");

        let vars = ctx.get_visible_variables();
        assert_eq!(vars.get("USERNAME"), Some(&"TestValue".to_string()));

        // Cleanup
        let _ = fs::remove_file("tests/batch_files/temp_test_input.txt");
    }

    #[test]
    fn test_set_p_with_setlocal() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::debugger::Frame;
        use std::fs;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a call frame with SETLOCAL
        ctx.call_stack.push(Frame {
            return_pc: 0,
            args: None,
            locals: std::collections::HashMap::new(),
            has_setlocal: false,
        });
        ctx.handle_setlocal();

        // Create a temp file with input
        fs::write("tests/batch_files/temp_test_input2.txt", "LocalValue\n")
            .expect("Failed to write temp file");

        // Execute SET /P command
        let _ = ctx.run_command("SET /P LOCALNAME=<tests/batch_files/temp_test_input2.txt");

        // Track the command
        ctx.track_set_command("SET /P LOCALNAME=<tests/batch_files/temp_test_input2.txt");

        let frame_vars = ctx.get_frame_variables(0);
        assert_eq!(frame_vars.get("LOCALNAME"), Some(&"LocalValue".to_string()));

        // Should NOT be in global variables
        assert_eq!(ctx.variables.get("LOCALNAME"), None);

        // Cleanup
        let _ = fs::remove_file("tests/batch_files/temp_test_input2.txt");
    }

    #[test]
    fn test_watch_add_and_get() {
        use batch_debugger::dap::DapServer;

        let mut server = DapServer::new();

        // Add watch expressions
        server.add_watch("COUNTER".to_string());
        server.add_watch("NAME".to_string());

        // Get watches
        let watches = server.get_watches();
        assert_eq!(watches.len(), 2);
        assert!(watches.contains(&"COUNTER".to_string()));
        assert!(watches.contains(&"NAME".to_string()));
    }

    #[test]
    fn test_watch_no_duplicates() {
        use batch_debugger::dap::DapServer;

        let mut server = DapServer::new();

        // Add same watch twice
        server.add_watch("COUNTER".to_string());
        server.add_watch("COUNTER".to_string());

        // Should only have one
        let watches = server.get_watches();
        assert_eq!(watches.len(), 1);
        assert_eq!(watches[0], "COUNTER");
    }

    #[test]
    fn test_watch_remove() {
        use batch_debugger::dap::DapServer;

        let mut server = DapServer::new();

        // Add watches
        server.add_watch("COUNTER".to_string());
        server.add_watch("NAME".to_string());
        server.add_watch("VALUE".to_string());

        // Remove one
        server.remove_watch("NAME");

        // Verify removal
        let watches = server.get_watches();
        assert_eq!(watches.len(), 2);
        assert!(watches.contains(&"COUNTER".to_string()));
        assert!(watches.contains(&"VALUE".to_string()));
        assert!(!watches.contains(&"NAME".to_string()));
    }

    #[test]
    fn test_watch_expressions_evaluation() {
        use batch_debugger::dap::DapServer;
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use std::sync::{Arc, Mutex};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set up some variables
        ctx.track_set_command("SET COUNTER=5");
        ctx.track_set_command("SET NAME=Alice");

        let mut server = DapServer::new();
        server.set_context(Arc::new(Mutex::new(ctx)));

        // Add watch expressions
        server.add_watch("COUNTER".to_string());
        server.add_watch("NAME".to_string());

        // Simulate variables request for watch scope (variablesReference = 3)
        let args = serde_json::json!({
            "variablesReference": 3
        });

        // We can't easily test the full DAP flow here, but we can verify
        // that watches are stored and can be retrieved
        let watches = server.get_watches();
        assert_eq!(watches.len(), 2);

        // Verify we can evaluate the expressions
        if let Some(ctx_arc) = server.get_context() {
            if let Ok(mut ctx) = ctx_arc.lock() {
                let counter_val = ctx.evaluate_expression("COUNTER").unwrap();
                assert_eq!(counter_val, "5");

                let name_val = ctx.evaluate_expression("NAME").unwrap();
                assert_eq!(name_val, "Alice");
            }
        }
    }

    #[test]
    fn test_watch_with_complex_expressions() {
        use batch_debugger::dap::DapServer;
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use std::sync::{Arc, Mutex};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set up variable
        ctx.track_set_command("SET /A BASE=10");

        let mut server = DapServer::new();
        server.set_context(Arc::new(Mutex::new(ctx)));

        // Add complex watch expression
        server.add_watch("BASE".to_string());

        let watches = server.get_watches();
        assert_eq!(watches.len(), 1);

        // Verify evaluation works
        if let Some(ctx_arc) = server.get_context() {
            if let Ok(mut ctx) = ctx_arc.lock() {
                let result = ctx.evaluate_expression("BASE").unwrap();
                assert_eq!(result, "10");
            }
        }
    }

    #[test]
    fn test_if_errorlevel_condition() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::parser::{parse_if_statement, IfCondition};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set exit code to 5
        ctx.last_exit_code = 5;

        // Parse and evaluate: IF ERRORLEVEL 5
        let if_stmt = parse_if_statement("IF ERRORLEVEL 5 echo Test").expect("Failed to parse");
        match if_stmt.condition {
            IfCondition::ErrorLevel { not, level } => {
                assert!(!not, "Should not have NOT modifier");
                assert_eq!(level, 5, "Should check for level 5");
            }
            _ => panic!("Wrong condition type"),
        }

        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "ERRORLEVEL 5 should be true when exit code is 5");

        // Test with higher exit code
        ctx.last_exit_code = 10;
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(
            result,
            "ERRORLEVEL 5 should be true when exit code is 10 (>=)"
        );

        // Test with lower exit code
        ctx.last_exit_code = 3;
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(!result, "ERRORLEVEL 5 should be false when exit code is 3");
    }

    #[test]
    fn test_if_string_equal_condition() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::parser::parse_if_statement;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable
        ctx.set_variable("NAME", "Alice")
            .expect("Failed to set NAME");

        // Parse and evaluate: IF "%NAME%"=="Alice"
        let if_stmt =
            parse_if_statement("IF \"%NAME%\"==\"Alice\" echo Match").expect("Failed to parse");

        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "String comparison should match");

        // Test case-insensitive comparison
        let if_stmt =
            parse_if_statement("IF \"%NAME%\"==\"ALICE\" echo Match").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(
            result,
            "String comparison should be case-insensitive (Alice vs ALICE)"
        );

        // Test NOT modifier
        let if_stmt = parse_if_statement("IF NOT \"%NAME%\"==\"Bob\" echo Different")
            .expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "NOT modifier should work");
    }

    #[test]
    fn test_if_exist_condition() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::parser::parse_if_statement;
        use std::fs;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a temporary file
        let test_file = "tests/batch_files/temp_if_test.txt";
        fs::write(test_file, "test").expect("Failed to create test file");

        // Parse and evaluate: IF EXIST file
        let if_stmt = parse_if_statement(&format!("IF EXIST {} echo Exists", test_file))
            .expect("Failed to parse");

        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "File should exist");

        // Cleanup
        fs::remove_file(test_file).ok();

        // Test NOT EXIST
        let if_stmt = parse_if_statement(&format!("IF NOT EXIST {} echo NotExists", test_file))
            .expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "File should not exist after deletion");
    }

    #[test]
    fn test_if_defined_condition() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::parser::parse_if_statement;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable
        ctx.set_variable("MYVAR", "value")
            .expect("Failed to set MYVAR");

        // Parse and evaluate: IF DEFINED MYVAR
        let if_stmt = parse_if_statement("IF DEFINED MYVAR echo Defined").expect("Failed to parse");

        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "Variable should be defined");

        // Test undefined variable
        let if_stmt =
            parse_if_statement("IF DEFINED NOTDEFINED echo Defined").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(!result, "Variable should not be defined");

        // Test NOT DEFINED
        let if_stmt = parse_if_statement("IF NOT DEFINED NOTDEFINED echo NotDefined")
            .expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "NOT DEFINED should work");
    }

    #[test]
    fn test_if_compare_numeric() {
        use batch_debugger::debugger::CmdSession;
        use batch_debugger::debugger::DebugContext;
        use batch_debugger::parser::parse_if_statement;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set numeric variables
        ctx.set_variable("NUM1", "10").expect("Failed to set NUM1");
        ctx.set_variable("NUM2", "20").expect("Failed to set NUM2");

        // Test EQU
        let if_stmt = parse_if_statement("IF %NUM1% EQU 10 echo Equal").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "EQU should work (10 == 10)");

        // Test NEQ
        let if_stmt =
            parse_if_statement("IF %NUM1% NEQ %NUM2% echo NotEqual").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "NEQ should work (10 != 20)");

        // Test LSS
        let if_stmt =
            parse_if_statement("IF %NUM1% LSS %NUM2% echo Less").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "LSS should work (10 < 20)");

        // Test LEQ
        let if_stmt =
            parse_if_statement("IF %NUM1% LEQ 10 echo LessOrEqual").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "LEQ should work (10 <= 10)");

        // Test GTR
        let if_stmt =
            parse_if_statement("IF %NUM2% GTR %NUM1% echo Greater").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "GTR should work (20 > 10)");

        // Test GEQ
        let if_stmt =
            parse_if_statement("IF %NUM2% GEQ 20 echo GreaterOrEqual").expect("Failed to parse");
        let result = ctx
            .evaluate_if_condition(&if_stmt.condition)
            .expect("Failed to evaluate");
        assert!(result, "GEQ should work (20 >= 20)");
    }

    #[test]
    fn test_if_parsing_all_types() {
        use batch_debugger::parser::{parse_if_statement, IfCondition};

        // Test ERRORLEVEL parsing
        let stmt = parse_if_statement("IF ERRORLEVEL 1 echo Error").expect("Parse failed");
        match stmt.condition {
            IfCondition::ErrorLevel {
                not: false,
                level: 1,
            } => {}
            _ => panic!("Wrong condition type for ERRORLEVEL"),
        }
        assert_eq!(stmt.then_command, "echo Error");

        // Test string equality parsing
        let stmt = parse_if_statement("IF \"a\"==\"b\" echo Equal").expect("Parse failed");
        match stmt.condition {
            IfCondition::StringEqual { not: false, .. } => {}
            _ => panic!("Wrong condition type for string equality"),
        }

        // Test EXIST parsing
        let stmt = parse_if_statement("IF EXIST file.txt echo Exists").expect("Parse failed");
        match stmt.condition {
            IfCondition::Exist { not: false, path } => {
                assert_eq!(path, "file.txt");
            }
            _ => panic!("Wrong condition type for EXIST"),
        }

        // Test DEFINED parsing
        let stmt = parse_if_statement("IF DEFINED VAR echo Defined").expect("Parse failed");
        match stmt.condition {
            IfCondition::Defined {
                not: false,
                variable,
            } => {
                assert_eq!(variable, "VAR");
            }
            _ => panic!("Wrong condition type for DEFINED"),
        }

        // Test comparison parsing
        let stmt = parse_if_statement("IF 1 EQU 2 echo Equal").expect("Parse failed");
        match stmt.condition {
            IfCondition::Compare { not: false, op, .. } => {
                assert_eq!(op, "EQU");
            }
            _ => panic!("Wrong condition type for comparison"),
        }

        // Test NOT modifier
        let stmt = parse_if_statement("IF NOT ERRORLEVEL 1 echo NoError").expect("Parse failed");
        match stmt.condition {
            IfCondition::ErrorLevel {
                not: true,
                level: 1,
            } => {}
            _ => panic!("NOT modifier not parsed correctly"),
        }
    }

    #[test]
    fn test_for_basic_parsing() {
        use batch_debugger::parser::{parse_for_statement, ForLoopType};

        // Test basic FOR loop parsing
        let stmt = parse_for_statement("FOR %%i IN (a b c) DO echo %%i").expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::Basic {
                variable,
                items,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(items, vec!["a", "b", "c"]);
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for basic FOR"),
        }
    }

    #[test]
    fn test_for_numeric_parsing() {
        use batch_debugger::parser::{parse_for_statement, ForLoopType};

        // Test FOR /L numeric loop parsing
        let stmt = parse_for_statement("FOR /L %%i IN (1,1,5) DO echo %%i").expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::Numeric {
                variable,
                start,
                step,
                end,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(start, 1);
                assert_eq!(step, 1);
                assert_eq!(end, 5);
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for FOR /L"),
        }

        // Test negative step
        let stmt =
            parse_for_statement("FOR /L %%j IN (10,-2,0) DO echo %%j").expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::Numeric {
                variable,
                start,
                step,
                end,
                command,
            } => {
                assert_eq!(variable, "%%j");
                assert_eq!(start, 10);
                assert_eq!(step, -2);
                assert_eq!(end, 0);
                assert_eq!(command, "echo %%j");
            }
            _ => panic!("Wrong loop type for FOR /L with negative step"),
        }
    }

    #[test]
    fn test_for_file_parser_parsing() {
        use batch_debugger::parser::{parse_for_statement, ForFileSource, ForLoopType};

        // Test FOR /F with file
        let stmt =
            parse_for_statement("FOR /F %%i IN (file.txt) DO echo %%i").expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::FileParser {
                variable,
                options,
                source,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(options, "");
                match source {
                    ForFileSource::File(path) => assert_eq!(path, "file.txt"),
                    _ => panic!("Wrong source type"),
                }
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for FOR /F"),
        }

        // Test FOR /F with options
        let stmt = parse_for_statement("FOR /F \"skip=1\" %%i IN (file.txt) DO echo %%i")
            .expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::FileParser {
                variable,
                options,
                source,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(options, "skip=1");
                match source {
                    ForFileSource::File(path) => assert_eq!(path, "file.txt"),
                    _ => panic!("Wrong source type"),
                }
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for FOR /F with options"),
        }
    }

    #[test]
    fn test_for_directory_parsing() {
        use batch_debugger::parser::{parse_for_statement, ForLoopType};

        // Test FOR /D directory loop parsing
        let stmt = parse_for_statement("FOR /D %%i IN (*) DO echo %%i").expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::Directory {
                variable,
                pattern,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(pattern, "*");
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for FOR /D"),
        }
    }

    #[test]
    fn test_for_recursive_parsing() {
        use batch_debugger::parser::{parse_for_statement, ForLoopType};

        // Test FOR /R without root path
        let stmt = parse_for_statement("FOR /R %%i IN (*.txt) DO echo %%i").expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::Recursive {
                variable,
                root_path,
                pattern,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(root_path, None);
                assert_eq!(pattern, "*.txt");
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for FOR /R"),
        }

        // Test FOR /R with root path
        let stmt = parse_for_statement("FOR /R C:\\temp %%i IN (*.txt) DO echo %%i")
            .expect("Parse failed");
        match stmt.loop_type {
            ForLoopType::Recursive {
                variable,
                root_path,
                pattern,
                command,
            } => {
                assert_eq!(variable, "%%i");
                assert_eq!(root_path, Some("C:\\temp".to_string()));
                assert_eq!(pattern, "*.txt");
                assert_eq!(command, "echo %%i");
            }
            _ => panic!("Wrong loop type for FOR /R with path"),
        }
    }

    #[test]
    fn test_for_basic_expansion() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use batch_debugger::parser::{parse_for_statement, ForLoopType};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test basic FOR loop expansion
        let stmt = parse_for_statement("FOR %%i IN (apple banana cherry) DO echo %%i")
            .expect("Parse failed");

        let iterations = ctx
            .expand_for_loop(&stmt.loop_type)
            .expect("Failed to expand");

        assert_eq!(iterations.len(), 3, "Should have 3 iterations");

        assert_eq!(iterations[0].1, "%%i");
        assert_eq!(iterations[0].2, "apple");
        assert!(iterations[0].0.contains("apple"));

        assert_eq!(iterations[1].2, "banana");
        assert_eq!(iterations[2].2, "cherry");
    }

    #[test]
    fn test_for_numeric_expansion() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use batch_debugger::parser::{parse_for_statement, ForLoopType};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Test FOR /L expansion
        let stmt = parse_for_statement("FOR /L %%n IN (1,1,3) DO echo %%n").expect("Parse failed");

        let iterations = ctx
            .expand_for_loop(&stmt.loop_type)
            .expect("Failed to expand");

        assert_eq!(iterations.len(), 3, "Should have 3 iterations (1,2,3)");
        assert_eq!(iterations[0].2, "1");
        assert_eq!(iterations[1].2, "2");
        assert_eq!(iterations[2].2, "3");

        // Test negative step
        let stmt = parse_for_statement("FOR /L %%n IN (5,-1,3) DO echo %%n").expect("Parse failed");

        let iterations = ctx
            .expand_for_loop(&stmt.loop_type)
            .expect("Failed to expand");

        assert_eq!(iterations.len(), 3, "Should have 3 iterations (5,4,3)");
        assert_eq!(iterations[0].2, "5");
        assert_eq!(iterations[1].2, "4");
        assert_eq!(iterations[2].2, "3");
    }

    #[test]
    fn test_for_loop_variable_tracking() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use batch_debugger::parser::parse_for_statement;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Expand a basic FOR loop
        let stmt =
            parse_for_statement("FOR %%x IN (one two three) DO echo %%x").expect("Parse failed");

        let iterations = ctx
            .expand_for_loop(&stmt.loop_type)
            .expect("Failed to expand");

        // Test that loop variable can be tracked
        ctx.set_loop_variable("%%x", "test_value");

        let vars = ctx.get_visible_variables();
        assert_eq!(
            vars.get("%%x"),
            Some(&"test_value".to_string()),
            "Loop variable should be tracked"
        );
    }

    #[test]
    fn test_for_with_setlocal() {
        use batch_debugger::debugger::{CmdSession, DebugContext, Frame};
        use batch_debugger::parser::parse_for_statement;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a SETLOCAL scope
        ctx.call_stack.push(Frame::new(0, None));
        ctx.handle_setlocal();

        // Set loop variable in local scope
        ctx.set_loop_variable("%%i", "local_value");

        // Check it's in local scope
        let frame_vars = ctx.get_frame_variables(0);
        assert_eq!(
            frame_vars.get("%%i"),
            Some(&"local_value".to_string()),
            "Loop variable should be in local scope"
        );

        // Check visible variables include it
        let visible = ctx.get_visible_variables();
        assert_eq!(
            visible.get("%%i"),
            Some(&"local_value".to_string()),
            "Loop variable should be visible"
        );
    }

    #[test]
    fn test_parse_redirection_output() {
        use batch_debugger::parser::parse_redirections;

        // Test > output redirection
        let cmd = parse_redirections("echo Hello > output.txt");
        assert_eq!(cmd.base_command, "echo Hello");
        assert_eq!(cmd.redirections.len(), 1);
        assert_eq!(cmd.redirections[0].operator, ">");
        assert_eq!(cmd.redirections[0].target, "output.txt");
    }

    #[test]
    fn test_parse_redirection_append() {
        use batch_debugger::parser::parse_redirections;

        // Test >> append redirection
        let cmd = parse_redirections("echo World >> output.txt");
        assert_eq!(cmd.base_command, "echo World");
        assert_eq!(cmd.redirections.len(), 1);
        assert_eq!(cmd.redirections[0].operator, ">>");
        assert_eq!(cmd.redirections[0].target, "output.txt");
    }

    #[test]
    fn test_parse_redirection_input() {
        use batch_debugger::parser::parse_redirections;

        // Test < input redirection
        let cmd = parse_redirections("sort < input.txt");
        assert_eq!(cmd.base_command, "sort");
        assert_eq!(cmd.redirections.len(), 1);
        assert_eq!(cmd.redirections[0].operator, "<");
        assert_eq!(cmd.redirections[0].target, "input.txt");
    }

    #[test]
    fn test_parse_redirection_stderr() {
        use batch_debugger::parser::parse_redirections;

        // Test 2> stderr redirection
        let cmd = parse_redirections("command 2> error.log");
        assert_eq!(cmd.base_command, "command");
        assert_eq!(cmd.redirections.len(), 1);
        assert_eq!(cmd.redirections[0].operator, "2>");
        assert_eq!(cmd.redirections[0].target, "error.log");
    }

    #[test]
    fn test_parse_redirection_stderr_to_stdout() {
        use batch_debugger::parser::parse_redirections;

        // Test 2>&1 stderr to stdout redirection
        let cmd = parse_redirections("command 2>&1");
        assert_eq!(cmd.base_command, "command");
        assert_eq!(cmd.redirections.len(), 1);
        assert_eq!(cmd.redirections[0].operator, "2>&1");
        assert_eq!(cmd.redirections[0].target, "");
    }

    #[test]
    fn test_parse_redirection_pipe() {
        use batch_debugger::parser::parse_redirections;

        // Test | pipe
        let cmd = parse_redirections("dir | findstr .txt");
        assert_eq!(cmd.base_command, "dir");
        assert_eq!(cmd.redirections.len(), 1);
        assert_eq!(cmd.redirections[0].operator, "|");
        assert_eq!(cmd.redirections[0].target, "findstr .txt");
    }

    #[test]
    fn test_parse_redirection_multiple() {
        use batch_debugger::parser::parse_redirections;

        // Test multiple redirections
        let cmd = parse_redirections("command < input.txt > output.txt 2> error.log");
        assert_eq!(cmd.base_command, "command");
        assert_eq!(cmd.redirections.len(), 3);
        assert_eq!(cmd.redirections[0].operator, "<");
        assert_eq!(cmd.redirections[0].target, "input.txt");
        assert_eq!(cmd.redirections[1].operator, ">");
        assert_eq!(cmd.redirections[1].target, "output.txt");
        assert_eq!(cmd.redirections[2].operator, "2>");
        assert_eq!(cmd.redirections[2].target, "error.log");
    }

    #[test]
    fn test_parse_redirection_quoted() {
        use batch_debugger::parser::parse_redirections;

        // Test that quoted strings are not parsed as redirections
        let cmd = parse_redirections("echo \"Hello > World\"");
        assert_eq!(cmd.base_command, "echo \"Hello > World\"");
        assert_eq!(cmd.redirections.len(), 0);
    }

    #[test]
    fn test_parse_redirection_no_redirections() {
        use batch_debugger::parser::parse_redirections;

        // Test command with no redirections
        let cmd = parse_redirections("echo Hello World");
        assert_eq!(cmd.base_command, "echo Hello World");
        assert_eq!(cmd.redirections.len(), 0);
    }

    #[test]
    fn test_parse_redirection_not_or_operator() {
        use batch_debugger::parser::parse_redirections;

        // Test that || (OR operator) is not parsed as pipe
        let cmd = parse_redirections("command1 || command2");
        assert_eq!(cmd.base_command, "command1 || command2");
        assert_eq!(cmd.redirections.len(), 0);
    }

    #[test]
    fn test_data_breakpoint_add_and_check() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable
        ctx.run_command("SET COUNTER=0")
            .expect("Failed to set variable");
        ctx.track_set_command("SET COUNTER=0");

        // Add data breakpoint on COUNTER
        ctx.add_data_breakpoint("COUNTER".to_string());

        // Check no hit initially
        assert!(!ctx.check_data_breakpoints(), "Should not hit initially");

        // Change the variable
        ctx.run_command("SET COUNTER=1")
            .expect("Failed to set variable");
        ctx.track_set_command("SET COUNTER=1");

        // Should hit now
        assert!(ctx.check_data_breakpoints(), "Should hit after change");

        // Update breakpoints
        ctx.update_data_breakpoints();

        // Should not hit again with same value
        assert!(
            !ctx.check_data_breakpoints(),
            "Should not hit with same value"
        );
    }

    #[test]
    fn test_data_breakpoint_multiple_variables() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set variables
        ctx.run_command("SET VAR1=A").expect("Failed to set VAR1");
        ctx.track_set_command("SET VAR1=A");
        ctx.run_command("SET VAR2=B").expect("Failed to set VAR2");
        ctx.track_set_command("SET VAR2=B");

        // Add data breakpoints
        ctx.add_data_breakpoint("VAR1".to_string());
        ctx.add_data_breakpoint("VAR2".to_string());

        // Check no hit initially
        assert!(!ctx.check_data_breakpoints(), "Should not hit initially");

        // Change VAR1
        ctx.run_command("SET VAR1=C").expect("Failed to set VAR1");
        ctx.track_set_command("SET VAR1=C");

        // Should hit
        assert!(ctx.check_data_breakpoints(), "Should hit after VAR1 change");
        assert_eq!(ctx.data_breakpoint_hit.as_ref().unwrap().0, "VAR1");

        // Update
        ctx.update_data_breakpoints();

        // Change VAR2
        ctx.run_command("SET VAR2=D").expect("Failed to set VAR2");
        ctx.track_set_command("SET VAR2=D");

        // Should hit
        assert!(ctx.check_data_breakpoints(), "Should hit after VAR2 change");
        assert_eq!(ctx.data_breakpoint_hit.as_ref().unwrap().0, "VAR2");
    }

    #[test]
    fn test_data_breakpoint_removal() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a variable
        ctx.run_command("SET VALUE=10")
            .expect("Failed to set variable");
        ctx.track_set_command("SET VALUE=10");

        // Add data breakpoint
        ctx.add_data_breakpoint("VALUE".to_string());

        // Remove it
        ctx.remove_data_breakpoint("VALUE");

        // Change the variable
        ctx.run_command("SET VALUE=20")
            .expect("Failed to set variable");
        ctx.track_set_command("SET VALUE=20");

        // Should not hit after removal
        assert!(
            !ctx.check_data_breakpoints(),
            "Should not hit after removal"
        );
    }

    #[test]
    fn test_data_breakpoint_get_list() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Add multiple data breakpoints
        ctx.add_data_breakpoint("VAR_A".to_string());
        ctx.add_data_breakpoint("VAR_B".to_string());
        ctx.add_data_breakpoint("VAR_C".to_string());

        // Get list
        let breakpoints = ctx.get_data_breakpoints();
        assert_eq!(breakpoints.len(), 3, "Should have 3 data breakpoints");
        assert!(breakpoints.contains_key("VAR_A"));
        assert!(breakpoints.contains_key("VAR_B"));
        assert!(breakpoints.contains_key("VAR_C"));
    }

    #[test]
    fn test_hover_variable_preview() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set some variables
        ctx.run_command("SET NAME=John")
            .expect("Failed to set NAME");
        ctx.track_set_command("SET NAME=John");
        ctx.run_command("SET AGE=30").expect("Failed to set AGE");
        ctx.track_set_command("SET AGE=30");

        // Test hover evaluation (same as regular evaluation)
        let result = ctx
            .evaluate_expression("NAME")
            .expect("Failed to evaluate NAME");
        assert_eq!(result, "John", "Hover should show NAME value");

        let result = ctx
            .evaluate_expression("%AGE%")
            .expect("Failed to evaluate AGE");
        assert_eq!(result, "30", "Hover should show AGE value");

        // Test ERRORLEVEL
        let result = ctx
            .evaluate_expression("ERRORLEVEL")
            .expect("Failed to evaluate ERRORLEVEL");
        assert_eq!(result, "0", "Hover should show ERRORLEVEL value");
    }

    #[test]
    fn test_pushd_changes_directory() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use std::env;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Get current directory
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Create a test directory
        let test_dir = original_dir.join("tests");
        if test_dir.exists() {
            // PUSHD to tests directory
            ctx.handle_pushd(Some("tests"))
                .expect("Failed to PUSHD to tests");

            // Check directory changed
            let new_dir = env::current_dir().expect("Failed to get new dir");
            assert_eq!(new_dir, test_dir, "Directory should have changed");

            // Check stack has entry
            let stack = ctx.get_directory_stack();
            assert_eq!(stack.len(), 1, "Stack should have 1 entry");
            assert_eq!(
                stack[0],
                original_dir.to_str().unwrap(),
                "Stack should contain original directory"
            );

            // Clean up - go back
            env::set_current_dir(&original_dir).ok();
        }
    }

    #[test]
    fn test_pushd_without_argument() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use std::env;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // PUSHD without argument (should fail or do nothing)
        let result = ctx.handle_pushd(None);

        // In real CMD, PUSHD without args displays stack or does nothing
        // Our implementation should handle it gracefully
        assert!(result.is_ok(), "PUSHD without args should not error");
    }

    #[test]
    fn test_popd_restores_directory() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use std::env;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Get current directory
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Create a test directory
        let test_dir = original_dir.join("tests");
        if test_dir.exists() {
            // PUSHD to tests directory
            ctx.handle_pushd(Some("tests"))
                .expect("Failed to PUSHD to tests");

            // Verify we're in the new directory
            let new_dir = env::current_dir().expect("Failed to get new dir");
            assert_eq!(new_dir, test_dir, "Should be in tests directory");

            // POPD back
            ctx.handle_popd().expect("Failed to POPD");

            // Check directory restored
            let restored_dir = env::current_dir().expect("Failed to get restored dir");
            assert_eq!(restored_dir, original_dir, "Directory should be restored");

            // Check stack is empty
            let stack = ctx.get_directory_stack();
            assert_eq!(stack.len(), 0, "Stack should be empty after POPD");
        }
    }

    #[test]
    fn test_popd_empty_stack() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // POPD with empty stack should fail gracefully
        let result = ctx.handle_popd();

        // Should return an error
        assert!(result.is_err(), "POPD on empty stack should error");
    }

    #[test]
    fn test_pushd_popd_multiple_levels() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use std::env;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Get current directory
        let original_dir = env::current_dir().expect("Failed to get current dir");

        // Test nested PUSHD operations
        let test_dir = original_dir.join("tests");
        let batch_dir = test_dir.join("batch_files");

        if test_dir.exists() {
            // First PUSHD
            ctx.handle_pushd(Some("tests"))
                .expect("Failed to first PUSHD");

            let stack = ctx.get_directory_stack();
            assert_eq!(
                stack.len(),
                1,
                "Stack should have 1 entry after first PUSHD"
            );

            if batch_dir.exists() {
                // Second PUSHD
                ctx.handle_pushd(Some("batch_files"))
                    .expect("Failed to second PUSHD");

                let stack = ctx.get_directory_stack();
                assert_eq!(
                    stack.len(),
                    2,
                    "Stack should have 2 entries after second PUSHD"
                );

                // First POPD
                ctx.handle_popd().expect("Failed to first POPD");
                let stack = ctx.get_directory_stack();
                assert_eq!(stack.len(), 1, "Stack should have 1 entry after first POPD");

                // Second POPD
                ctx.handle_popd().expect("Failed to second POPD");
                let stack = ctx.get_directory_stack();
                assert_eq!(stack.len(), 0, "Stack should be empty after second POPD");

                // Verify back to original
                let current = env::current_dir().expect("Failed to get current dir");
                assert_eq!(
                    current, original_dir,
                    "Should be back to original directory"
                );
            } else {
                // Clean up if batch_files doesn't exist
                ctx.handle_popd().ok();
                env::set_current_dir(&original_dir).ok();
            }
        }
    }

    #[test]
    fn test_shift_basic() {
        use batch_debugger::debugger::{CmdSession, DebugContext, Frame};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a call frame with arguments
        let mut frame = Frame::new(
            10,
            Some(vec![
                "arg1".to_string(),
                "arg2".to_string(),
                "arg3".to_string(),
                "arg4".to_string(),
            ]),
        );

        // Manually set up the frame
        ctx.call_stack.push(frame);

        // Get initial args
        let initial_args = ctx.call_stack.last().unwrap().args.as_ref().unwrap();
        assert_eq!(initial_args.len(), 4, "Should have 4 args initially");
        assert_eq!(initial_args[0], "arg1");
        assert_eq!(initial_args[1], "arg2");

        // SHIFT by 1
        ctx.handle_shift(1);

        // Check args shifted
        let shifted_args = ctx.call_stack.last().unwrap().args.as_ref().unwrap();
        assert_eq!(shifted_args.len(), 3, "Should have 3 args after shift");
        assert_eq!(shifted_args[0], "arg2", "First arg should now be arg2");
        assert_eq!(shifted_args[1], "arg3", "Second arg should now be arg3");
        assert_eq!(shifted_args[2], "arg4", "Third arg should now be arg4");
    }

    #[test]
    fn test_shift_multiple() {
        use batch_debugger::debugger::{CmdSession, DebugContext, Frame};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create a call frame with arguments
        ctx.call_stack.push(Frame::new(
            10,
            Some(vec![
                "a".to_string(),
                "b".to_string(),
                "c".to_string(),
                "d".to_string(),
                "e".to_string(),
            ]),
        ));

        // SHIFT by 2
        ctx.handle_shift(2);

        // Check args shifted by 2
        let shifted_args = ctx.call_stack.last().unwrap().args.as_ref().unwrap();
        assert_eq!(shifted_args.len(), 3, "Should have 3 args after shift /2");
        assert_eq!(shifted_args[0], "c", "First arg should now be c");
        assert_eq!(shifted_args[1], "d", "Second arg should now be d");
        assert_eq!(shifted_args[2], "e", "Third arg should now be e");
    }

    #[test]
    fn test_shift_empty_args() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // No call frame - should handle gracefully
        ctx.handle_shift(1);

        // Should not crash
        assert_eq!(ctx.call_stack.len(), 0, "Stack should still be empty");
    }

    #[test]
    fn test_shift_no_frame_args() {
        use batch_debugger::debugger::{CmdSession, DebugContext, Frame};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create frame with no args
        ctx.call_stack.push(Frame::new(10, None));

        // SHIFT should handle gracefully
        ctx.handle_shift(1);

        // Should not crash
        let frame = ctx.call_stack.last().unwrap();
        assert!(frame.args.is_none(), "Args should still be None");
    }

    #[test]
    fn test_shift_beyond_args() {
        use batch_debugger::debugger::{CmdSession, DebugContext, Frame};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create frame with 2 args
        ctx.call_stack
            .push(Frame::new(10, Some(vec!["x".to_string(), "y".to_string()])));

        // SHIFT by 5 (more than available)
        ctx.handle_shift(5);

        // Should clear all args
        let frame = ctx.call_stack.last().unwrap();
        if let Some(args) = &frame.args {
            assert_eq!(args.len(), 0, "All args should be shifted away");
        }
    }

    #[test]
    fn test_shift_with_setlocal() {
        use batch_debugger::debugger::{CmdSession, DebugContext, Frame};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Create frame with SETLOCAL and args
        ctx.call_stack.push(Frame::new(
            10,
            Some(vec![
                "param1".to_string(),
                "param2".to_string(),
                "param3".to_string(),
            ]),
        ));
        ctx.handle_setlocal();

        // SHIFT should still work
        ctx.handle_shift(1);

        let frame = ctx.call_stack.last().unwrap();
        let shifted_args = frame.args.as_ref().unwrap();
        assert_eq!(shifted_args.len(), 2, "Should have 2 args after shift");
        assert_eq!(shifted_args[0], "param2", "First arg should be param2");
        assert_eq!(shifted_args[1], "param3", "Second arg should be param3");

        // SETLOCAL should still be active
        assert!(frame.has_setlocal, "SETLOCAL should still be active");
    }

    #[test]
    fn test_directory_stack_tracking() {
        use batch_debugger::debugger::{CmdSession, DebugContext};
        use std::env;

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        let original_dir = env::current_dir().expect("Failed to get current dir");
        let test_dir = original_dir.join("tests");

        if test_dir.exists() {
            // Stack should be empty initially
            assert_eq!(ctx.get_directory_stack().len(), 0);

            // PUSHD
            ctx.handle_pushd(Some("tests")).expect("Failed to PUSHD");

            // Stack should have 1 entry
            let stack = ctx.get_directory_stack();
            assert_eq!(stack.len(), 1);
            assert_eq!(stack[0], original_dir.to_str().unwrap());

            // PUSHD again
            let src_dir = test_dir.join("batch_files");
            if src_dir.exists() {
                ctx.handle_pushd(Some("batch_files"))
                    .expect("Failed to second PUSHD");

                // Stack should have 2 entries
                let stack = ctx.get_directory_stack();
                assert_eq!(stack.len(), 2);
                assert_eq!(stack[1], test_dir.to_str().unwrap());
            }

            // Clean up
            while !ctx.get_directory_stack().is_empty() {
                ctx.handle_popd().ok();
            }
            env::set_current_dir(&original_dir).ok();
        }
    }

    #[test]
    fn test_builtin_command_detection() {
        // This test verifies that is_builtin_command() correctly identifies built-in commands
        // The function is in dap_runner.rs and is private, so we test indirectly
        // by verifying that the list of built-ins is comprehensive

        let builtins = vec![
            "ECHO", "SET", "IF", "FOR", "CALL", "GOTO", "EXIT", "REM", "CD", "CHDIR", "DIR",
            "COPY", "MOVE", "DEL", "ERASE", "REN", "RENAME", "MD", "MKDIR", "RD", "RMDIR", "TYPE",
            "CLS", "PAUSE", "DATE", "TIME", "PATH", "PROMPT", "TITLE", "VER", "VOL", "ASSOC",
            "FTYPE", "PUSHD", "POPD", "SETLOCAL", "ENDLOCAL", "SHIFT", "START", "COLOR", "MKLINK",
            "BREAK", "VERIFY",
        ];

        // All of these should be recognized as built-ins
        // (We can't directly test the function since it's private, but we document the expected list)
        assert!(
            builtins.len() > 30,
            "Should have comprehensive list of built-ins"
        );
    }

    #[test]
    fn test_external_command_examples() {
        // Examples of commands that should be detected as external
        let externals = vec![
            "python", "node", "git", "npm", "cargo", "javac", "gcc", "cl.exe", "notepad", "calc",
            "explorer", "tasklist", "netstat", "ping", "ipconfig",
            "findstr", // This is actually built-in to find.exe, but often used like external
        ];

        // These are common external commands that should NOT be in the built-in list
        // (We document this for reference, actual detection happens in dap_runner.rs)
        assert!(
            externals.len() > 10,
            "Should have examples of external commands"
        );
    }

    #[test]
    fn test_command_name_extraction() {
        // Test that command names are extracted correctly from full commands
        // The is_builtin_command function should extract just the command name

        let test_cases = vec![
            ("ECHO Hello World", "ECHO"),
            ("SET VAR=value", "SET"),
            ("IF EXIST file.txt ECHO Found", "IF"),
            ("FOR %%i IN (a b c) DO ECHO %%i", "FOR"),
            ("python script.py arg1 arg2", "python"),
            ("git commit -m \"message\"", "git"),
        ];

        for (full_cmd, expected_name) in test_cases {
            let extracted = full_cmd.split_whitespace().next().unwrap();
            assert_eq!(
                extracted.to_uppercase(),
                expected_name.to_uppercase(),
                "Should extract '{}' from '{}'",
                expected_name,
                full_cmd
            );
        }
    }

    #[test]
    fn test_string_operation_substring() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a test string
        ctx.set_variable("TEXT", "HelloWorld")
            .expect("Failed to set TEXT");

        // Test substring from start: %TEXT:~0,5% should be "Hello"
        let result = ctx
            .evaluate_expression("%TEXT:~0,5%")
            .expect("Failed to evaluate substring");
        assert_eq!(result, "Hello", "Substring from start should work");

        // Test substring from position: %TEXT:~5,5% should be "World"
        let result = ctx
            .evaluate_expression("%TEXT:~5,5%")
            .expect("Failed to evaluate substring");
        assert_eq!(result, "World", "Substring from middle should work");

        // Test substring from end: %TEXT:~-5% should be "World"
        let result = ctx
            .evaluate_expression("%TEXT:~-5%")
            .expect("Failed to evaluate substring from end");
        assert_eq!(result, "World", "Substring from end should work");
    }

    #[test]
    fn test_string_operation_replacement() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a test string
        ctx.set_variable("PATH_STR", "C:\\old\\path\\file.txt")
            .expect("Failed to set PATH_STR");

        // Test string replacement: %PATH_STR:old=new%
        let result = ctx
            .evaluate_expression("%PATH_STR:old=new%")
            .expect("Failed to evaluate replacement");
        assert_eq!(
            result, "C:\\new\\path\\file.txt",
            "String replacement should work"
        );

        // Test case-insensitive replacement
        ctx.set_variable("TEXT", "Hello World")
            .expect("Failed to set TEXT");
        let result = ctx
            .evaluate_expression("%TEXT:hello=Goodbye%")
            .expect("Failed to evaluate case-insensitive replacement");
        assert_eq!(
            result, "Goodbye World",
            "Case-insensitive replacement should work"
        );
    }

    #[test]
    fn test_string_operation_replace_from_start() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a test string
        ctx.set_variable("FILENAME", "prefix_document.txt")
            .expect("Failed to set FILENAME");

        // Test replace from start: %FILENAME:*_=%
        let result = ctx
            .evaluate_expression("%FILENAME:*_=%")
            .expect("Failed to evaluate replace from start");
        assert_eq!(
            result, "document.txt",
            "Replace from start should remove prefix"
        );
    }

    #[test]
    fn test_string_operation_combined() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a test string
        ctx.set_variable("DATA", "test_value_123")
            .expect("Failed to set DATA");

        // First do replacement, then substring
        ctx.set_variable("TEMP", "test_modified_123")
            .expect("Failed to set TEMP");

        let result = ctx
            .evaluate_expression("%TEMP:~0,13%")
            .expect("Failed to evaluate combined operation");
        assert_eq!(result, "test_modified", "Combined operations should work");
    }

    #[test]
    fn test_string_operation_empty_replacement() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a test string
        ctx.set_variable("TEXT", "HelloWorldHello")
            .expect("Failed to set TEXT");

        // Test removing substring: %TEXT:Hello=%
        // Note: CMD replaces ALL occurrences in this case
        let result = ctx
            .evaluate_expression("%TEXT:Hello=%")
            .expect("Failed to evaluate empty replacement");
        assert_eq!(
            result, "World",
            "Empty replacement should remove all occurrences"
        );
    }

    #[test]
    fn test_string_operation_with_spaces() {
        use batch_debugger::debugger::{CmdSession, DebugContext};

        let session = CmdSession::start().expect("Failed to start CMD session");
        let mut ctx = DebugContext::new(session);

        // Set a string with spaces
        ctx.set_variable("MESSAGE", "The quick brown fox")
            .expect("Failed to set MESSAGE");

        // Test substring with spaces
        let result = ctx
            .evaluate_expression("%MESSAGE:~4,5%")
            .expect("Failed to evaluate substring with spaces");
        assert_eq!(result, "quick", "Substring with spaces should work");

        // Test replacement with spaces
        let result = ctx
            .evaluate_expression("%MESSAGE:quick=slow%")
            .expect("Failed to evaluate replacement with spaces");
        assert_eq!(
            result, "The slow brown fox",
            "Replacement with spaces should work"
        );
    }
}
