use super::ast::*;
use super::parser::ParseError;

const MAX_TAKE: usize = 500;
const DEFAULT_TAKE: usize = 50;

#[derive(Debug)]
pub struct ValidationResult {
    pub program: Program,
    pub applied_defaults: Vec<String>,
}

pub fn validate(mut program: Program) -> Result<ValidationResult, ParseError> {
    let mut defaults = Vec::new();

    for (i, stmt) in program.statements.iter_mut().enumerate() {
        validate_head(&stmt.head, i)?;
        validate_pipeline(&mut stmt.pipeline, &stmt.head, i, &mut defaults)?;
    }

    Ok(ValidationResult {
        program,
        applied_defaults: defaults,
    })
}

fn validate_head(head: &Head, stmt_idx: usize) -> Result<(), ParseError> {
    match head {
        Head::Search(s) => validate_search_conditions(&s.conditions, stmt_idx),
        Head::Insert(ins) => validate_insert(ins, stmt_idx),
        Head::Update(u) => validate_update(u, stmt_idx),
        Head::Delete(d) => validate_delete(d, stmt_idx),
        _ => Ok(()),
    }
}

fn validate_search_conditions(conditions: &[Condition], _stmt_idx: usize) -> Result<(), ParseError> {
    for cond in conditions {
        if let Condition::Cmp { field, op, value } = cond {
            validate_field_type_compat(field, op, value)?;
        }
    }
    Ok(())
}

fn validate_insert(ins: &InsertHead, _stmt_idx: usize) -> Result<(), ParseError> {
    if ins.content.is_empty() {
        return Err(ParseError {
            position: 0,
            expected: vec!["non-empty content string".into()],
            got: "empty string".into(),
            hint: Some("INSERT INTO facts requires a non-empty content string".into()),
        });
    }
    if !matches!(ins.store, Store::Facts) {
        return Err(ParseError {
            position: 0,
            expected: vec!["facts".into()],
            got: ins.store.as_str().into(),
            hint: Some("INSERT currently only supports the facts store".into()),
        });
    }
    for (key, _) in &ins.fields {
        validate_fact_field(key)?;
    }
    Ok(())
}

fn validate_update(u: &UpdateHead, _stmt_idx: usize) -> Result<(), ParseError> {
    if !matches!(u.store, Store::Facts) {
        return Err(ParseError {
            position: 0,
            expected: vec!["facts".into()],
            got: u.store.as_str().into(),
            hint: Some("UPDATE currently only supports the facts store".into()),
        });
    }
    if u.assignments.is_empty() {
        return Err(ParseError {
            position: 0,
            expected: vec!["at least one SET assignment".into()],
            got: "empty SET".into(),
            hint: None,
        });
    }
    for (key, _) in &u.assignments {
        validate_fact_field(key)?;
    }
    Ok(())
}

fn validate_delete(d: &DeleteHead, _stmt_idx: usize) -> Result<(), ParseError> {
    if !matches!(d.store, Store::Facts) {
        return Err(ParseError {
            position: 0,
            expected: vec!["facts".into()],
            got: d.store.as_str().into(),
            hint: Some("DELETE currently only supports the facts store".into()),
        });
    }
    if d.conditions.is_empty() {
        return Err(ParseError {
            position: 0,
            expected: vec!["at least one WHERE condition".into()],
            got: "no conditions".into(),
            hint: Some("DELETE requires conditions to prevent accidental bulk deletion".into()),
        });
    }
    Ok(())
}

fn validate_pipeline(
    pipeline: &mut Vec<PipeStage>,
    head: &Head,
    _stmt_idx: usize,
    defaults: &mut Vec<String>,
) -> Result<(), ParseError> {
    let is_search = matches!(head, Head::Search(_));
    let is_list_or_history = matches!(
        head,
        Head::ListJobs | Head::ListSessions | Head::ListTools | Head::HistoryJob(_)
    );

    for stage in pipeline.iter() {
        match stage {
            PipeStage::Sort { .. } if !is_search => {
                return Err(ParseError {
                    position: 0,
                    expected: vec!["SORT only after SEARCH".into()],
                    got: "SORT after non-SEARCH".into(),
                    hint: Some("SORT is only valid as a pipeline stage after SEARCH".into()),
                });
            }
            PipeStage::Take(n) => {
                if *n > MAX_TAKE {
                    return Err(ParseError {
                        position: 0,
                        expected: vec![format!("TAKE value <= {MAX_TAKE}")],
                        got: format!("TAKE {n}"),
                        hint: Some(format!("maximum TAKE is {MAX_TAKE} to prevent oversized results")),
                    });
                }
            }
            PipeStage::Count if !is_search => {
                return Err(ParseError {
                    position: 0,
                    expected: vec!["COUNT only after SEARCH".into()],
                    got: "COUNT after non-SEARCH".into(),
                    hint: Some("COUNT is only valid after SEARCH".into()),
                });
            }
            _ => {}
        }
    }

    // Apply default TAKE if not present on search/list heads (and no COUNT)
    if (is_search || is_list_or_history)
        && !pipeline.iter().any(|s| matches!(s, PipeStage::Take(_)))
        && !pipeline.iter().any(|s| matches!(s, PipeStage::Count))
    {
        pipeline.push(PipeStage::Take(DEFAULT_TAKE));
        defaults.push(format!("applied default TAKE {DEFAULT_TAKE}"));
    }

    Ok(())
}

fn validate_fact_field(field: &str) -> Result<(), ParseError> {
    const VALID_FIELDS: &[&str] = &[
        "content", "summary", "pointer", "keywords", "confidence",
        "topic", "source_message_id", "id",
    ];
    if !VALID_FIELDS.contains(&field) {
        return Err(ParseError {
            position: 0,
            expected: VALID_FIELDS.iter().map(|s| (*s).to_string()).collect(),
            got: field.to_string(),
            hint: Some(format!(
                "valid fact fields: {}",
                VALID_FIELDS.join(", ")
            )),
        });
    }
    Ok(())
}

fn validate_field_type_compat(
    field: &str,
    _op: &CmpOp,
    value: &Literal,
) -> Result<(), ParseError> {
    match field {
        "confidence" => {
            if !matches!(value, Literal::Float(_) | Literal::Int(_)) {
                return Err(ParseError {
                    position: 0,
                    expected: vec!["number".into()],
                    got: format!("{value:?}"),
                    hint: Some("confidence must be compared to a number (e.g. confidence > 0.7)".into()),
                });
            }
        }
        "id" => {
            if !matches!(value, Literal::Str(_)) {
                return Err(ParseError {
                    position: 0,
                    expected: vec!["string".into()],
                    got: format!("{value:?}"),
                    hint: Some("id must be compared to a string".into()),
                });
            }
        }
        _ => {}
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::lexer::lex;
    use crate::dsl::parser::parse;

    fn validated(input: &str) -> Result<ValidationResult, ParseError> {
        let tokens = lex(input).map_err(|e| ParseError {
            position: e.pos,
            expected: vec![],
            got: e.message,
            hint: None,
        })?;
        let prog = parse(tokens, input)?;
        validate(prog)
    }

    #[test]
    fn validate_applies_default_take() {
        let result = validated("SEARCH facts").unwrap();
        let pipeline = &result.program.statements[0].pipeline;
        assert!(pipeline.iter().any(|s| matches!(s, PipeStage::Take(50))));
        assert!(!result.applied_defaults.is_empty());
    }

    #[test]
    fn validate_take_ceiling() {
        let err = validated("SEARCH facts | TAKE 1000").unwrap_err();
        assert!(err.got.contains("1000"));
    }

    #[test]
    fn validate_no_default_take_with_count() {
        let result = validated("SEARCH facts | COUNT").unwrap();
        let pipeline = &result.program.statements[0].pipeline;
        assert!(!pipeline.iter().any(|s| matches!(s, PipeStage::Take(_))));
    }

    #[test]
    fn validate_insert_empty_content() {
        let err = validated(r#"INSERT INTO facts ("", topic="x")"#).unwrap_err();
        assert!(err.hint.unwrap().contains("non-empty"));
    }

    #[test]
    fn validate_delete_requires_conditions() {
        // Parser already enforces this, but belt-and-suspenders
        let err = validated("DELETE FROM facts").unwrap_err();
        assert!(err.expected.iter().any(|e| e.contains("WHERE")));
    }

    #[test]
    fn validate_sort_only_after_search() {
        let err = validated(r#"RUN JOB "test" | SORT name ASC"#).unwrap_err();
        assert!(err.hint.unwrap().contains("SORT"));
    }

    #[test]
    fn validate_confidence_type_check() {
        let err = validated(r#"SEARCH facts WHERE confidence > "high""#).unwrap_err();
        assert!(err.hint.unwrap().contains("number"));
    }
}
