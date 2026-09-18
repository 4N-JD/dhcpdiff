use super::ast::{IscBlock, IscDocument, IscNode, IscStatement, SourceLocation};
use super::lexer::{Lexer, Token};

pub fn parse_isc(input: &str) -> anyhow::Result<IscDocument> {
    let mut lexer = Lexer::new(input);
    let nodes = parse_nodes(&mut lexer)?;
    Ok(IscDocument { nodes })
}

fn parse_nodes(lexer: &mut Lexer<'_>) -> anyhow::Result<Vec<IscNode>> {
    let mut nodes = Vec::new();
    loop {
        match lexer.peek() {
            Token::Eof | Token::RBrace => break,
            Token::LBrace => anyhow::bail!("unexpected '{{' at line {}", lexer.line()),
            _ => {
                if let Some(node) = parse_node(lexer)? {
                    nodes.push(node);
                }
            }
        }
    }
    Ok(nodes)
}

fn parse_node(lexer: &mut Lexer<'_>) -> anyhow::Result<Option<IscNode>> {
    let line = lexer.line();
    let column = lexer.column();
    let mut parts = Vec::new();

    loop {
        match lexer.next() {
            Token::Text(t) => parts.push(t),
            Token::Semicolon => {
                let text = parts.join(" ").trim().to_string();
                if text.is_empty() {
                    return Ok(None);
                }
                return Ok(Some(IscNode::Statement(IscStatement {
                    text,
                    location: SourceLocation {
                        line,
                        column,
                        end_line: Some(line),
                    },
                })));
            }
            Token::LBrace => {
                let header = parts.join(" ").trim().to_string();
                let children = parse_nodes(lexer)?;
                match lexer.next() {
                    Token::RBrace => {}
                    other => {
                        anyhow::bail!("expected '}}', got {:?} at line {}", other, lexer.line())
                    }
                }
                let end_line = lexer.line();
                return Ok(Some(IscNode::Block(IscBlock {
                    header,
                    location: SourceLocation {
                        line,
                        column,
                        end_line: Some(end_line),
                    },
                    children,
                })));
            }
            Token::RBrace | Token::Eof => {
                if parts.is_empty() {
                    return Ok(None);
                }
                let text = parts.join(" ").trim().to_string();
                return Ok(Some(IscNode::Statement(IscStatement {
                    text,
                    location: SourceLocation {
                        line,
                        column,
                        end_line: Some(line),
                    },
                })));
            }
        }
    }
}
