#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourceLocation {
    pub line: u32,
    pub column: u32,
    /// Line of the closing `}` for blocks, or the statement line for statements.
    pub end_line: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IscStatement {
    pub text: String,
    pub location: SourceLocation,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IscBlock {
    pub header: String,
    pub location: SourceLocation,
    pub children: Vec<IscNode>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IscNode {
    Statement(IscStatement),
    Block(IscBlock),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct IscDocument {
    pub nodes: Vec<IscNode>,
}

impl IscDocument {
    pub fn walk<'a>(&'a self) -> impl Iterator<Item = &'a IscNode> {
        self.nodes.iter()
    }
}
