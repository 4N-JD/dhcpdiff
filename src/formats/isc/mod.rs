mod ast;
mod lexer;
mod options;
mod parser;

pub use ast::{IscBlock, IscDocument, IscNode, IscStatement, SourceLocation};
pub use options::{
    collect_definitions, collect_options_from_nodes, collect_options_from_nodes_labeled,
    insert_statement_options, insert_statement_options_labeled, option_key_from_name,
    parse_bootp_statement, parse_lease_time_statement, parse_option_definition,
    parse_option_statement, statement_source, walk_nodes,
};
pub use parser::parse_isc;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{NormalizedValue, OptionKey};

    #[test]
    fn block_location_includes_end_line() {
        let doc = parse_isc(
            r#"
subnet 10.0.0.0 netmask 255.255.255.0 {
  pool {
    range 10.0.0.10 10.0.0.20;
  }
}
"#,
        )
        .unwrap();
        let IscNode::Block(subnet) = &doc.nodes[0] else {
            panic!("expected subnet block");
        };
        assert_eq!(subnet.location.line, 2);
        assert_eq!(subnet.location.end_line, Some(6));

        let IscNode::Block(pool) = &subnet.children[0] else {
            panic!("expected pool block");
        };
        assert_eq!(pool.location.line, 3);
        assert_eq!(pool.location.end_line, Some(5));
    }

    #[test]
    fn lease_time_statements_map_to_isc_space() {
        assert_eq!(
            parse_lease_time_statement("default-lease-time 3600;"),
            Some((OptionKey::isc_default_lease_time(), NormalizedValue::Int(3600)))
        );
        assert_eq!(
            parse_lease_time_statement("min-lease-time 600;"),
            Some((OptionKey::isc_min_lease_time(), NormalizedValue::Int(600)))
        );
        assert_eq!(
            parse_lease_time_statement("max-lease-time 7200;"),
            Some((OptionKey::isc_max_lease_time(), NormalizedValue::Int(7200)))
        );
        assert!(parse_lease_time_statement("option dhcp-lease-time 3600;").is_none());
    }

    #[test]
    fn lexer_preserves_escapes_for_option_value_normalization() {
        let via_double = parse_isc(r#"option bootfile-name "SMSBoot\\x86\\wdsnbp.com";"#).unwrap();
        let via_hex = parse_isc(r#"option bootfile-name "SMSBoot\x5cx86\x5cwdsnbp.com";"#).unwrap();
        let text = |doc: &IscDocument| {
            let IscNode::Statement(stmt) = &doc.nodes[0] else {
                panic!("expected statement");
            };
            stmt.text.clone()
        };
        // Lexer must keep escapes intact (not strip one backslash level).
        assert!(text(&via_double).contains(r"\\x86"), "got {}", text(&via_double));
        assert!(text(&via_hex).contains(r"\x5c"), "got {}", text(&via_hex));

        let (_, v1) = parse_option_statement(&text(&via_double)).unwrap();
        let (_, v2) = parse_option_statement(&text(&via_hex)).unwrap();
        assert_eq!(
            NormalizedValue::from_raw_text(&v1),
            NormalizedValue::from_raw_text(&v2)
        );
        assert_eq!(
            NormalizedValue::from_raw_text(&v1),
            NormalizedValue::String(r"SMSBoot\x86\wdsnbp.com".into())
        );
    }
}

