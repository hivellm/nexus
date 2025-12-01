#!/usr/bin/env python3
import sys

with open('nexus-core/src/lib.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old_str = '''                executor::parser::SetItem::Property {
                    target,
                    property,
                    value,
                } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;

                    let json_value = self.expression_to_json_value(value)?;
                    for node_id in node_ids {
                        let state = self.ensure_node_state(*node_id, &mut state_map)?;
                        state
                            .properties
                            .insert(property.clone(), json_value.clone());
                    }
                }'''

new_str = '''                executor::parser::SetItem::Property {
                    target,
                    property,
                    value,
                } => {
                    let node_ids = context.get(target).ok_or_else(|| {
                        Error::CypherExecution(format!(
                            "Unknown variable '{}' in SET clause",
                            target
                        ))
                    })?;

                    // Evaluate expression per-node to support expressions like n.value * 2
                    for node_id in node_ids.clone() {
                        let state = self.ensure_node_state(node_id, &mut state_map)?;
                        let json_value = self.evaluate_set_expression(value, target, &state.properties)?;
                        state.properties.insert(property.clone(), json_value);
                    }
                }'''

if old_str in content:
    content = content.replace(old_str, new_str, 1)
    with open('nexus-core/src/lib.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print('File updated successfully')
else:
    print('Pattern not found')
    sys.exit(1)
