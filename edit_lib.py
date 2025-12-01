#!/usr/bin/env python3
import sys

with open('nexus-core/src/lib.rs', 'r', encoding='utf-8') as f:
    content = f.read()

old_str = '''        }
    }

    /// Refresh the executor to ensure it sees the latest storage state
    /// This is necessary because the executor uses a cloned RecordStore
    /// which has its own PropertyStore instance
    pub fn refresh_executor(&mut self) -> Result<()> {
        // Recreate executor with current storage state
        self.executor = executor::Executor::new('''

new_functions = '''        }
    }

    /// Evaluate expression for SET clause with node context
    fn evaluate_set_expression(
        &self,
        expr: &executor::parser::Expression,
        target_var: &str,
        node_props: &serde_json::Map<String, serde_json::Value>,
    ) -> Result<serde_json::Value> {
        match expr {
            executor::parser::Expression::Literal(lit) => match lit {
                executor::parser::Literal::String(s) => Ok(serde_json::Value::String(s.clone())),
                executor::parser::Literal::Integer(i) => Ok(serde_json::Value::Number((*i).into())),
                executor::parser::Literal::Float(f) => serde_json::Number::from_f64(*f)
                    .map(serde_json::Value::Number)
                    .ok_or_else(|| Error::CypherExecution(format!("Invalid float: {}", f))),
                executor::parser::Literal::Boolean(b) => Ok(serde_json::Value::Bool(*b)),
                executor::parser::Literal::Null => Ok(serde_json::Value::Null),
                executor::parser::Literal::Point(p) => Ok(p.to_json_value()),
            },
            executor::parser::Expression::PropertyAccess { variable, property } => {
                if variable == target_var {
                    Ok(node_props.get(property).cloned().unwrap_or(serde_json::Value::Null))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            executor::parser::Expression::BinaryOp { left, op, right } => {
                let left_val = self.evaluate_set_expression(left, target_var, node_props)?;
                let right_val = self.evaluate_set_expression(right, target_var, node_props)?;
                match op {
                    executor::parser::BinaryOperator::Add => self.json_add_values(&left_val, &right_val),
                    executor::parser::BinaryOperator::Subtract => self.json_subtract_values(&left_val, &right_val),
                    executor::parser::BinaryOperator::Multiply => self.json_multiply_values(&left_val, &right_val),
                    executor::parser::BinaryOperator::Divide => self.json_divide_values(&left_val, &right_val),
                    executor::parser::BinaryOperator::Modulo => self.json_modulo_values(&left_val, &right_val),
                    _ => Err(Error::CypherExecution(format!("Unsupported binary operator in SET: {:?}", op))),
                }
            }
            executor::parser::Expression::UnaryOp { op, operand } => {
                let val = self.evaluate_set_expression(operand, target_var, node_props)?;
                match op {
                    executor::parser::UnaryOperator::Minus => {
                        if let Some(n) = val.as_i64() {
                            Ok(serde_json::Value::Number((-n).into()))
                        } else if let Some(n) = val.as_f64() {
                            serde_json::Number::from_f64(-n)
                                .map(serde_json::Value::Number)
                                .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                        } else {
                            Ok(serde_json::Value::Null)
                        }
                    }
                    executor::parser::UnaryOperator::Not => {
                        val.as_bool()
                            .map(|b| serde_json::Value::Bool(!b))
                            .ok_or_else(|| Error::CypherExecution("Invalid bool".to_string()))
                    }
                    _ => Ok(serde_json::Value::Null),
                }
            }
            _ => Err(Error::CypherExecution("Unsupported expression type in SET clause".to_string())),
        }
    }

    fn json_add_values(&self, left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li + ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf + rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            (serde_json::Value::String(l), serde_json::Value::String(r)) => {
                Ok(serde_json::Value::String(format!("{}{}", l, r)))
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_subtract_values(&self, left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li - ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf - rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_multiply_values(&self, left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    Ok(serde_json::Value::Number((li * ri).into()))
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    serde_json::Number::from_f64(lf * rf)
                        .map(serde_json::Value::Number)
                        .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_divide_values(&self, left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    if rf == 0.0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        serde_json::Number::from_f64(lf / rf)
                            .map(serde_json::Value::Number)
                            .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    fn json_modulo_values(&self, left: &serde_json::Value, right: &serde_json::Value) -> Result<serde_json::Value> {
        match (left, right) {
            (serde_json::Value::Number(l), serde_json::Value::Number(r)) => {
                if let (Some(li), Some(ri)) = (l.as_i64(), r.as_i64()) {
                    if ri == 0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        Ok(serde_json::Value::Number((li % ri).into()))
                    }
                } else if let (Some(lf), Some(rf)) = (l.as_f64(), r.as_f64()) {
                    if rf == 0.0 {
                        Ok(serde_json::Value::Null)
                    } else {
                        serde_json::Number::from_f64(lf % rf)
                            .map(serde_json::Value::Number)
                            .ok_or_else(|| Error::CypherExecution("Invalid float".to_string()))
                    }
                } else {
                    Ok(serde_json::Value::Null)
                }
            }
            _ => Ok(serde_json::Value::Null),
        }
    }

    /// Refresh the executor to ensure it sees the latest storage state
    /// This is necessary because the executor uses a cloned RecordStore
    /// which has its own PropertyStore instance
    pub fn refresh_executor(&mut self) -> Result<()> {
        // Recreate executor with current storage state
        self.executor = executor::Executor::new('''

if old_str in content:
    content = content.replace(old_str, new_functions, 1)
    with open('nexus-core/src/lib.rs', 'w', encoding='utf-8') as f:
        f.write(content)
    print('File updated successfully')
else:
    print('Pattern not found')
    sys.exit(1)
