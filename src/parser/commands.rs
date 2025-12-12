/// Represents a command operator for composite commands
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CommandOp {
    Unconditional, // &
    And,           // &&
    Or,            // ||
}

/// A single command part in a composite command line
#[derive(Debug, Clone)]
pub struct CommandPart {
    pub text: String,
    pub op: Option<CommandOp>,
}

/// Normalize whitespace in command
pub fn normalize_whitespace(line: &str) -> String {
    line.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Split a command line by composite operators (&, &&, ||)
pub fn split_composite_command(line: &str) -> Vec<CommandPart> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if escaped {
            current.push(ch);
            escaped = false;
            continue;
        }

        if ch == '^' {
            escaped = true;
            current.push(ch);
            continue;
        }

        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
            continue;
        }

        if !in_quotes && ch == '&' {
            let op = if chars.peek() == Some(&'&') {
                chars.next();
                CommandOp::And
            } else {
                CommandOp::Unconditional
            };

            parts.push(CommandPart {
                text: current.trim().to_string(),
                op: Some(op),
            });
            current.clear();
            continue;
        }

        if !in_quotes && ch == '|' {
            if chars.peek() == Some(&'|') {
                chars.next();
                parts.push(CommandPart {
                    text: current.trim().to_string(),
                    op: Some(CommandOp::Or),
                });
                current.clear();
                continue;
            }
        }

        current.push(ch);
    }

    if !current.trim().is_empty() {
        parts.push(CommandPart {
            text: current.trim().to_string(),
            op: None,
        });
    }

    parts
}

/// Check if line is a comment
pub fn is_comment(line: &str) -> bool {
    let trimmed = line.trim();
    trimmed.is_empty()
        || trimmed.to_uppercase().starts_with("REM ")
        || trimmed.starts_with("::")
        || trimmed.to_uppercase().starts_with("REM\t")
}

/// Represents a redirection operator and its target
#[derive(Debug, Clone, PartialEq)]
pub struct Redirection {
    pub operator: String, // ">", ">>", "<", "2>", "2>&1", "|"
    pub target: String,   // filename or empty for pipes
}

/// Represents a command with its redirections
#[derive(Debug, Clone)]
pub struct CommandWithRedirections {
    pub base_command: String,
    pub redirections: Vec<Redirection>,
}

/// Parse redirections from a command line
pub fn parse_redirections(line: &str) -> CommandWithRedirections {
    let mut base_command = String::new();
    let mut redirections = Vec::new();
    let mut chars = line.chars().peekable();
    let mut in_quotes = false;
    let mut current = String::new();

    while let Some(ch) = chars.next() {
        if ch == '"' {
            in_quotes = !in_quotes;
            current.push(ch);
            continue;
        }

        if in_quotes {
            current.push(ch);
            continue;
        }

        // Check for redirection operators
        if ch == '>' {
            // Check for >> or >
            if chars.peek() == Some(&'>') {
                chars.next();
                // >> append redirection
                base_command.push_str(&current);
                current.clear();

                // Skip whitespace
                while chars.peek() == Some(&' ') {
                    chars.next();
                }

                // Get target filename
                let mut target = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ' '
                        || next_ch == '>'
                        || next_ch == '<'
                        || next_ch == '|'
                        || next_ch == '&'
                    {
                        break;
                    }
                    target.push(chars.next().unwrap());
                }

                redirections.push(Redirection {
                    operator: ">>".to_string(),
                    target: target.trim().to_string(),
                });
            } else {
                // > overwrite redirection
                base_command.push_str(&current);
                current.clear();

                // Skip whitespace
                while chars.peek() == Some(&' ') {
                    chars.next();
                }

                // Get target filename
                let mut target = String::new();
                while let Some(&next_ch) = chars.peek() {
                    if next_ch == ' '
                        || next_ch == '>'
                        || next_ch == '<'
                        || next_ch == '|'
                        || next_ch == '&'
                    {
                        break;
                    }
                    target.push(chars.next().unwrap());
                }

                redirections.push(Redirection {
                    operator: ">".to_string(),
                    target: target.trim().to_string(),
                });
            }
        } else if ch == '<' {
            // < input redirection
            base_command.push_str(&current);
            current.clear();

            // Skip whitespace
            while chars.peek() == Some(&' ') {
                chars.next();
            }

            // Get target filename
            let mut target = String::new();
            while let Some(&next_ch) = chars.peek() {
                if next_ch == ' '
                    || next_ch == '>'
                    || next_ch == '<'
                    || next_ch == '|'
                    || next_ch == '&'
                {
                    break;
                }
                target.push(chars.next().unwrap());
            }

            redirections.push(Redirection {
                operator: "<".to_string(),
                target: target.trim().to_string(),
            });
        } else if ch == '2' {
            // Check for 2> or 2>&1
            if chars.peek() == Some(&'>') {
                chars.next();
                if chars.peek() == Some(&'&') {
                    chars.next();
                    if chars.peek() == Some(&'1') {
                        chars.next();
                        // 2>&1 redirect stderr to stdout
                        base_command.push_str(&current);
                        current.clear();

                        redirections.push(Redirection {
                            operator: "2>&1".to_string(),
                            target: String::new(),
                        });
                    } else {
                        current.push('2');
                        current.push('>');
                        current.push('&');
                    }
                } else {
                    // 2> redirect stderr
                    base_command.push_str(&current);
                    current.clear();

                    // Skip whitespace
                    while chars.peek() == Some(&' ') {
                        chars.next();
                    }

                    // Get target filename
                    let mut target = String::new();
                    while let Some(&next_ch) = chars.peek() {
                        if next_ch == ' '
                            || next_ch == '>'
                            || next_ch == '<'
                            || next_ch == '|'
                            || next_ch == '&'
                        {
                            break;
                        }
                        target.push(chars.next().unwrap());
                    }

                    redirections.push(Redirection {
                        operator: "2>".to_string(),
                        target: target.trim().to_string(),
                    });
                }
            } else {
                current.push(ch);
            }
        } else if ch == '|' {
            // Check if it's || (OR operator)
            if chars.peek() == Some(&'|') {
                // It's ||, keep it as part of the command
                current.push(ch);
                current.push(chars.next().unwrap());
            } else {
                // | pipe redirection
                base_command.push_str(&current);
                current.clear();

                // Skip whitespace
                while chars.peek() == Some(&' ') {
                    chars.next();
                }

                // Rest is the piped command
                let mut target = String::new();
                while let Some(next_ch) = chars.next() {
                    target.push(next_ch);
                }

                redirections.push(Redirection {
                    operator: "|".to_string(),
                    target: target.trim().to_string(),
                });
                break;
            }
        } else {
            current.push(ch);
        }
    }

    base_command.push_str(&current);

    CommandWithRedirections {
        base_command: base_command.trim().to_string(),
        redirections,
    }
}

/// Represents different types of IF conditions
#[derive(Debug, Clone, PartialEq)]
pub enum IfCondition {
    /// IF [NOT] ERRORLEVEL number
    ErrorLevel { not: bool, level: i32 },
    /// IF [NOT] string1==string2
    StringEqual {
        not: bool,
        left: String,
        right: String,
    },
    /// IF [NOT] EXIST filename
    Exist { not: bool, path: String },
    /// IF [NOT] DEFINED variable
    Defined { not: bool, variable: String },
    /// IF [NOT] string1 comparison string2 (EQU, NEQ, LSS, LEQ, GTR, GEQ)
    Compare {
        not: bool,
        left: String,
        op: String,
        right: String,
    },
}

/// Represents an IF statement with its condition and branches
#[derive(Debug, Clone)]
pub struct IfStatement {
    pub condition: IfCondition,
    pub then_command: String,
    pub else_command: Option<String>,
}

/// Parse an IF statement and extract its condition and branches
pub fn parse_if_statement(line: &str) -> Option<IfStatement> {
    let trimmed = line.trim();
    let upper = trimmed.to_uppercase();

    if !upper.starts_with("IF ") {
        return None;
    }

    // Skip "IF "
    let rest = &trimmed[3..].trim();

    // Check for NOT modifier
    let (not, rest) = if rest.to_uppercase().starts_with("NOT ") {
        (true, &rest[4..].trim())
    } else {
        (false, rest)
    };

    // Parse condition type
    let upper_rest = rest.to_uppercase();

    // Check for ERRORLEVEL
    if upper_rest.starts_with("ERRORLEVEL ") {
        let after_keyword = &rest[11..].trim();
        // Find where the command starts (after the number)
        if let Some(space_pos) = after_keyword.find(' ') {
            let level_str = &after_keyword[..space_pos].trim();
            let command = &after_keyword[space_pos..].trim();

            if let Ok(level) = level_str.parse::<i32>() {
                return Some(IfStatement {
                    condition: IfCondition::ErrorLevel { not, level },
                    then_command: command.to_string(),
                    else_command: None,
                });
            }
        }
    }

    // Check for EXIST
    if upper_rest.starts_with("EXIST ") {
        let after_keyword = &rest[6..].trim();
        // Find where the command starts
        if let Some(command_start) = find_command_start(after_keyword) {
            let path = after_keyword[..command_start].trim().to_string();
            let command = after_keyword[command_start..].trim().to_string();

            return Some(IfStatement {
                condition: IfCondition::Exist { not, path },
                then_command: command,
                else_command: None,
            });
        }
    }

    // Check for DEFINED
    if upper_rest.starts_with("DEFINED ") {
        let after_keyword = &rest[8..].trim();
        // Find where the command starts
        if let Some(command_start) = find_command_start(after_keyword) {
            let variable = after_keyword[..command_start].trim().to_string();
            let command = after_keyword[command_start..].trim().to_string();

            return Some(IfStatement {
                condition: IfCondition::Defined { not, variable },
                then_command: command,
                else_command: None,
            });
        }
    }

    // Check for comparison operators (EQU, NEQ, LSS, LEQ, GTR, GEQ)
    let comparison_ops = ["EQU", "NEQ", "LSS", "LEQ", "GTR", "GEQ"];
    for op in &comparison_ops {
        if let Some(op_pos) = upper_rest.find(&format!(" {} ", op)) {
            let left = rest[..op_pos].trim().to_string();
            let after_op = &rest[op_pos + op.len() + 2..]; // +2 for spaces

            if let Some(command_start) = find_command_start(after_op) {
                let right = after_op[..command_start].trim().to_string();
                let command = after_op[command_start..].trim().to_string();

                return Some(IfStatement {
                    condition: IfCondition::Compare {
                        not,
                        left,
                        op: op.to_string(),
                        right,
                    },
                    then_command: command,
                    else_command: None,
                });
            }
        }
    }

    // Check for string equality (==)
    if let Some(eq_pos) = rest.find("==") {
        let left = rest[..eq_pos].trim().to_string();
        let after_eq = &rest[eq_pos + 2..].trim();

        if let Some(command_start) = find_command_start(after_eq) {
            let right = after_eq[..command_start].trim().to_string();
            let command = after_eq[command_start..].trim().to_string();

            return Some(IfStatement {
                condition: IfCondition::StringEqual { not, left, right },
                then_command: command,
                else_command: None,
            });
        }
    }

    None
}

/// Find where the command starts after a condition value
/// This is tricky because the value might be quoted and contain spaces
fn find_command_start(text: &str) -> Option<usize> {
    let text = text.trim();

    // If starts with quote, find matching quote
    if text.starts_with('"') {
        if let Some(end_quote) = text[1..].find('"') {
            // Command starts after the closing quote and any spaces
            let after_quote = end_quote + 2;
            return Some(after_quote);
        }
    }

    // Otherwise, find first space
    if let Some(space_pos) = text.find(' ') {
        return Some(space_pos + 1);
    }

    None
}

/// Represents different types of FOR loop variants
#[derive(Debug, Clone, PartialEq)]
pub enum ForLoopType {
    /// FOR %%i IN (item1 item2 item3) DO command
    Basic {
        variable: String,
        items: Vec<String>,
        command: String,
    },
    /// FOR /L %%i IN (start,step,end) DO command
    Numeric {
        variable: String,
        start: i32,
        step: i32,
        end: i32,
        command: String,
    },
    /// FOR /F "options" %%i IN (file/command/'string') DO command
    FileParser {
        variable: String,
        options: String,
        source: ForFileSource,
        command: String,
    },
    /// FOR /D %%i IN (directory pattern) DO command
    Directory {
        variable: String,
        pattern: String,
        command: String,
    },
    /// FOR /R [[drive:]path] %%i IN (pattern) DO command
    Recursive {
        variable: String,
        root_path: Option<String>,
        pattern: String,
        command: String,
    },
}

/// Represents the source for FOR /F parsing
#[derive(Debug, Clone, PartialEq)]
pub enum ForFileSource {
    File(String),    // File path
    Command(String), // Command in single quotes
    String(String),  // String in single quotes
}

/// Represents a parsed FOR loop statement
#[derive(Debug, Clone)]
pub struct ForStatement {
    pub loop_type: ForLoopType,
}

/// Parse a FOR loop statement
pub fn parse_for_statement(line: &str) -> Option<ForStatement> {
    let trimmed = line.trim();
    let upper = trimmed.to_uppercase();

    if !upper.starts_with("FOR ") {
        return None;
    }

    // Skip "FOR "
    let rest = &trimmed[4..].trim();
    let upper_rest = rest.to_uppercase();

    // Check for /L (numeric loop)
    if upper_rest.starts_with("/L ") {
        return parse_for_numeric(&rest[3..].trim());
    }

    // Check for /F (file parser)
    if upper_rest.starts_with("/F ") {
        return parse_for_file_parser(&rest[3..].trim());
    }

    // Check for /D (directory)
    if upper_rest.starts_with("/D ") {
        return parse_for_directory(&rest[3..].trim());
    }

    // Check for /R (recursive)
    if upper_rest.starts_with("/R ") {
        return parse_for_recursive(&rest[3..].trim());
    }

    // Default: basic FOR loop
    parse_for_basic(rest)
}

/// Parse basic FOR loop: FOR %%i IN (items) DO command
fn parse_for_basic(text: &str) -> Option<ForStatement> {
    // Extract variable (%%i or %i)
    let text = text.trim();
    if !text.starts_with('%') {
        return None;
    }

    let var_end = if text.starts_with("%%") {
        3 // %%i
    } else {
        2 // %i
    };

    if text.len() < var_end {
        return None;
    }

    let variable = text[..var_end].to_string();
    let rest = text[var_end..].trim();

    // Find IN keyword
    let upper_rest = rest.to_uppercase();
    if !upper_rest.starts_with("IN ") {
        return None;
    }

    let after_in = &rest[3..].trim();

    // Find opening parenthesis
    if !after_in.starts_with('(') {
        return None;
    }

    // Find matching closing parenthesis
    let close_paren = after_in.find(')')?;
    let items_str = &after_in[1..close_paren];

    // Parse items (space-separated)
    let items: Vec<String> = items_str
        .split_whitespace()
        .map(|s| s.to_string())
        .collect();

    // Find DO keyword
    let after_paren = &after_in[close_paren + 1..].trim();
    let upper_after = after_paren.to_uppercase();
    if !upper_after.starts_with("DO ") {
        return None;
    }

    let command = after_paren[3..].trim().to_string();

    Some(ForStatement {
        loop_type: ForLoopType::Basic {
            variable,
            items,
            command,
        },
    })
}

/// Parse FOR /L numeric loop: FOR /L %%i IN (start,step,end) DO command
fn parse_for_numeric(text: &str) -> Option<ForStatement> {
    let text = text.trim();
    if !text.starts_with('%') {
        return None;
    }

    let var_end = if text.starts_with("%%") { 3 } else { 2 };
    if text.len() < var_end {
        return None;
    }

    let variable = text[..var_end].to_string();
    let rest = text[var_end..].trim();

    // Find IN keyword
    let upper_rest = rest.to_uppercase();
    if !upper_rest.starts_with("IN ") {
        return None;
    }

    let after_in = &rest[3..].trim();

    // Find opening parenthesis
    if !after_in.starts_with('(') {
        return None;
    }

    // Find matching closing parenthesis
    let close_paren = after_in.find(')')?;
    let range_str = &after_in[1..close_paren];

    // Parse range (start,step,end)
    let parts: Vec<&str> = range_str.split(',').collect();
    if parts.len() != 3 {
        return None;
    }

    let start = parts[0].trim().parse::<i32>().ok()?;
    let step = parts[1].trim().parse::<i32>().ok()?;
    let end = parts[2].trim().parse::<i32>().ok()?;

    // Find DO keyword
    let after_paren = &after_in[close_paren + 1..].trim();
    let upper_after = after_paren.to_uppercase();
    if !upper_after.starts_with("DO ") {
        return None;
    }

    let command = after_paren[3..].trim().to_string();

    Some(ForStatement {
        loop_type: ForLoopType::Numeric {
            variable,
            start,
            step,
            end,
            command,
        },
    })
}

/// Parse FOR /F file parser: FOR /F "options" %%i IN (file) DO command
fn parse_for_file_parser(text: &str) -> Option<ForStatement> {
    let text = text.trim();

    // Extract options (optional, may be quoted)
    let (options, rest) = if text.starts_with('"') {
        // Find closing quote
        let close_quote = text[1..].find('"')?;
        let opts = text[1..close_quote + 1].to_string();
        let remaining = text[close_quote + 2..].trim();
        (opts, remaining)
    } else {
        // No options
        (String::new(), text)
    };

    // Extract variable
    if !rest.starts_with('%') {
        return None;
    }

    let var_end = if rest.starts_with("%%") { 3 } else { 2 };
    if rest.len() < var_end {
        return None;
    }

    let variable = rest[..var_end].to_string();
    let rest = rest[var_end..].trim();

    // Find IN keyword
    let upper_rest = rest.to_uppercase();
    if !upper_rest.starts_with("IN ") {
        return None;
    }

    let after_in = &rest[3..].trim();

    // Find opening parenthesis
    if !after_in.starts_with('(') {
        return None;
    }

    // Find matching closing parenthesis
    let close_paren = after_in.find(')')?;
    let source_str = &after_in[1..close_paren];

    // Determine source type
    let source = if source_str.starts_with('\'') && source_str.ends_with('\'') {
        // Command or string in single quotes
        let content = source_str[1..source_str.len() - 1].to_string();
        if content.contains('|') || content.contains('&') {
            ForFileSource::Command(content)
        } else {
            ForFileSource::String(content)
        }
    } else {
        // File path
        ForFileSource::File(source_str.to_string())
    };

    // Find DO keyword
    let after_paren = &after_in[close_paren + 1..].trim();
    let upper_after = after_paren.to_uppercase();
    if !upper_after.starts_with("DO ") {
        return None;
    }

    let command = after_paren[3..].trim().to_string();

    Some(ForStatement {
        loop_type: ForLoopType::FileParser {
            variable,
            options,
            source,
            command,
        },
    })
}

/// Parse FOR /D directory: FOR /D %%i IN (pattern) DO command
fn parse_for_directory(text: &str) -> Option<ForStatement> {
    let text = text.trim();
    if !text.starts_with('%') {
        return None;
    }

    let var_end = if text.starts_with("%%") { 3 } else { 2 };
    if text.len() < var_end {
        return None;
    }

    let variable = text[..var_end].to_string();
    let rest = text[var_end..].trim();

    // Find IN keyword
    let upper_rest = rest.to_uppercase();
    if !upper_rest.starts_with("IN ") {
        return None;
    }

    let after_in = &rest[3..].trim();

    // Find opening parenthesis
    if !after_in.starts_with('(') {
        return None;
    }

    // Find matching closing parenthesis
    let close_paren = after_in.find(')')?;
    let pattern = after_in[1..close_paren].to_string();

    // Find DO keyword
    let after_paren = &after_in[close_paren + 1..].trim();
    let upper_after = after_paren.to_uppercase();
    if !upper_after.starts_with("DO ") {
        return None;
    }

    let command = after_paren[3..].trim().to_string();

    Some(ForStatement {
        loop_type: ForLoopType::Directory {
            variable,
            pattern,
            command,
        },
    })
}

/// Parse FOR /R recursive: FOR /R [[drive:]path] %%i IN (pattern) DO command
fn parse_for_recursive(text: &str) -> Option<ForStatement> {
    let text = text.trim();

    // Check if there's a root path
    let (root_path, rest) = if text.contains('%') {
        // Find where the variable starts
        let var_pos = text.find('%')?;
        let before_var = text[..var_pos].trim();

        if before_var.is_empty() {
            (None, text)
        } else {
            (Some(before_var.to_string()), &text[var_pos..])
        }
    } else {
        (None, text)
    };

    // Extract variable
    if !rest.starts_with('%') {
        return None;
    }

    let var_end = if rest.starts_with("%%") { 3 } else { 2 };
    if rest.len() < var_end {
        return None;
    }

    let variable = rest[..var_end].to_string();
    let rest = rest[var_end..].trim();

    // Find IN keyword
    let upper_rest = rest.to_uppercase();
    if !upper_rest.starts_with("IN ") {
        return None;
    }

    let after_in = &rest[3..].trim();

    // Find opening parenthesis
    if !after_in.starts_with('(') {
        return None;
    }

    // Find matching closing parenthesis
    let close_paren = after_in.find(')')?;
    let pattern = after_in[1..close_paren].to_string();

    // Find DO keyword
    let after_paren = &after_in[close_paren + 1..].trim();
    let upper_after = after_paren.to_uppercase();
    if !upper_after.starts_with("DO ") {
        return None;
    }

    let command = after_paren[3..].trim().to_string();

    Some(ForStatement {
        loop_type: ForLoopType::Recursive {
            variable,
            root_path,
            pattern,
            command,
        },
    })
}
