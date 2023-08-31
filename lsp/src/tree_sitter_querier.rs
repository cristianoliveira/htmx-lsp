// See: https://tree-sitter.github.io/tree-sitter/using-parsers#query-syntax

use std::collections::HashMap;

use log::{debug, error};
use tree_sitter::{Node, Point, Query, QueryCursor, Range};

use crate::tree_sitter::Position;

// If error char is "=" means the key name is completed and the cursor is
// at the "=" but no quote, so we shouldn't suggest yet eg <div hx-foo=|>
const KEY_VALUE_SEPARATOR: &str = "=";

#[derive(Debug)]
struct CaptureDetails {
    value: String,
    end: Point,
}

fn query_props(
    query_string: &str,
    node: Node<'_>,
    source: &str,
    trigger_point: Point,
) -> Option<HashMap<String, CaptureDetails>> {
    let query = Query::new(tree_sitter_html::language(), query_string).expect(&format!(
        "get_position_by_query invalid query {query_string}"
    ));
    let mut cursor_qry = QueryCursor::new();

    let capture_names = query.capture_names();

    let matches = cursor_qry.matches(&query, node, source.as_bytes());

    let mut props = HashMap::new();
    matches.into_iter().for_each(|match_| {
        match_
            .captures
            .iter()
            .filter(|capture| capture.node.start_position() <= trigger_point)
            .for_each(|capture| {
                let key = capture_names[capture.index as usize].to_owned();
                let value = if let Ok(capture_value) = capture.node.utf8_text(source.as_bytes()) {
                    capture_value.to_owned()
                } else {
                    error!("query_props capture.node.utf8_text failed {key}");
                    "".to_owned()
                };

                let details = CaptureDetails {
                    value,
                    end: capture.node.end_position(),
                };

                props.insert(key, details);
            });
    });

    Some(props)
}

pub fn query_attr_keys_for_completion(
    node: Node<'_>,
    source: &str,
    trigger_point: Point,
) -> Option<Position> {
    // [ means match any of the following
    let query_string = r#"
    (
        [
            (_ 
                (tag_name) 

                (_)*

                (attribute (attribute_name) @attr_name) @complete_match

                (#eq? @attr_name @complete_match)
            )

            (_ 
              (tag_name) 

              (attribute (attribute_name)) 

              (ERROR) @error_char
            )
        ]
    )"#;

    let attr_completion = query_props(query_string, node, source, trigger_point);
    let props = attr_completion?;
    let attr_name = props.get("attr_name")?;

    if props.get("error_char").is_some() {
        return None;
    }

    return Some(Position::AttributeName(attr_name.value.to_owned()));
}

pub fn query_attr_values_for_completion(
    node: Node<'_>,
    source: &str,
    trigger_point: Point,
) -> Option<Position> {
    // [ means match any of the following
    let query_string = r#"(
        [
          (ERROR 
            (tag_name) 

            (attribute_name) @attr_name 
            (_)
          ) @open_quote_err

          (_ 
            (tag_name)

            (attribute 
              (attribute_name) @attr_name
              (_)
            ) @last_item

            (ERROR) @error_char
          )

          (_
            (tag_name)

            (attribute 
              (attribute_name) @attr_name
              (quoted_attribute_value) @quoted_attr_value

              (#eq? @quoted_attr_value "\"\"")
            ) @empty_attribute
          )

          (_
            (tag_name) 

            (attribute 
              (attribute_name) @attr_name
              (quoted_attribute_value (attribute_value) @attr_value)

              ) @non_empty_attribute 
          )
        ]

        (#match? @attr_name "hx-.*")
    )"#;

    let value_completion = query_props(query_string, node, source, trigger_point);
    let props = value_completion?;

    let attr_name = props.get("attr_name")?;

    debug!("query_attr_values_for_completion attr_name {:?}", attr_name);

    if props.get("open_quote_err").is_some() || props.get("empty_attribute").is_some() {
        return Some(Position::AttributeValue {
            name: attr_name.value.to_owned(),
            value: "".to_string(),
        });
    }

    if let Some(error_char) = props.get("error_char") {
        if error_char.value == KEY_VALUE_SEPARATOR {
            return None;
        }
    };

    if let Some(capture) = props.get("non_empty_attribute") {
        // If the editor cursor point is after the attribute value, don't suggest
        if trigger_point >= capture.end {
            return None;
        }
    }

    return Some(Position::AttributeValue {
        name: attr_name.value.to_owned(),
        value: "".to_string(),
    });
}
