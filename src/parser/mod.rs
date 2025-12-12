mod commands;
mod labels;
mod preprocessor;
mod types;

pub use commands::{
    is_comment, normalize_whitespace, parse_for_statement, parse_if_statement, parse_redirections,
    split_composite_command, CommandOp, CommandWithRedirections, ForFileSource, ForLoopType,
    ForStatement, IfCondition, IfStatement, Redirection,
};
pub use labels::build_label_map;
pub use preprocessor::preprocess_lines;
pub use types::{LogicalLine, PreprocessResult};
