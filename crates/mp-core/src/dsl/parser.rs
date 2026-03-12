use super::ast::*;
use super::lexer::{Kw, Spanned, Token};

#[derive(Debug, Clone, thiserror::Error)]
#[error("parse error at position {position}: expected {expected:?}, got {got}")]
pub struct ParseError {
    pub position: usize,
    pub expected: Vec<String>,
    pub got: String,
    pub hint: Option<String>,
}

struct Parser {
    tokens: Vec<Spanned>,
    pos: usize,
}

impl Parser {
    fn new(tokens: Vec<Spanned>) -> Self {
        Self { tokens, pos: 0 }
    }

    fn peek(&self) -> Option<&Token> {
        self.tokens.get(self.pos).map(|s| &s.token)
    }

    fn current_pos(&self) -> usize {
        self.tokens
            .get(self.pos)
            .map(|s| s.pos)
            .unwrap_or_else(|| {
                self.tokens.last().map(|s| s.pos + 1).unwrap_or(0)
            })
    }

    fn advance(&mut self) -> Option<&Spanned> {
        let t = self.tokens.get(self.pos);
        if t.is_some() {
            self.pos += 1;
        }
        t
    }

    fn at_end(&self) -> bool {
        self.pos >= self.tokens.len()
    }

    fn expect_keyword(&mut self, kw: Kw) -> Result<(), ParseError> {
        match self.peek() {
            Some(Token::Keyword(k)) if *k == kw => {
                self.advance();
                Ok(())
            }
            other => Err(self.error(vec![kw.to_string()], other)),
        }
    }

    fn expect_string(&mut self) -> Result<String, ParseError> {
        match self.peek().cloned() {
            Some(Token::StringLit(s)) => {
                self.advance();
                Ok(s)
            }
            other => Err(self.error(vec!["string".into()], other.as_ref())),
        }
    }

    fn expect_ident_or_string(&mut self) -> Result<String, ParseError> {
        match self.peek().cloned() {
            Some(Token::StringLit(s)) => {
                self.advance();
                Ok(s)
            }
            Some(Token::Ident(s)) => {
                self.advance();
                Ok(s)
            }
            other => Err(self.error(vec!["identifier or string".into()], other.as_ref())),
        }
    }

    fn expect_ident(&mut self) -> Result<String, ParseError> {
        match self.peek().cloned() {
            Some(Token::Ident(s)) => {
                self.advance();
                Ok(s)
            }
            Some(Token::Keyword(kw)) => {
                self.advance();
                Ok(kw.to_string().to_lowercase())
            }
            other => Err(self.error(vec!["identifier".into()], other.as_ref())),
        }
    }

    fn expect_int(&mut self) -> Result<i64, ParseError> {
        match self.peek().cloned() {
            Some(Token::IntLit(n)) => {
                self.advance();
                Ok(n)
            }
            other => Err(self.error(vec!["integer".into()], other.as_ref())),
        }
    }

    fn match_keyword(&mut self, kw: Kw) -> bool {
        if matches!(self.peek(), Some(Token::Keyword(k)) if *k == kw) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn match_token(&mut self, expected: &Token) -> bool {
        if self.peek() == Some(expected) {
            self.advance();
            true
        } else {
            false
        }
    }

    fn error(&self, expected: Vec<String>, got: Option<&Token>) -> ParseError {
        ParseError {
            position: self.current_pos(),
            expected,
            got: got.map(|t| format!("{t}")).unwrap_or_else(|| "end of input".into()),
            hint: None,
        }
    }

    fn error_with_hint(
        &self,
        expected: Vec<String>,
        got: Option<&Token>,
        hint: &str,
    ) -> ParseError {
        ParseError {
            position: self.current_pos(),
            expected,
            got: got.map(|t| format!("{t}")).unwrap_or_else(|| "end of input".into()),
            hint: Some(hint.into()),
        }
    }
}

// ── Public entry point ──

pub fn parse(tokens: Vec<Spanned>, raw_input: &str) -> Result<Program, ParseError> {
    let groups = split_on_semicolons(&tokens);
    let mut statements = Vec::new();

    for group in groups {
        if group.is_empty() {
            continue;
        }
        let raw = group
            .iter()
            .map(|s| format!("{}", s.token))
            .collect::<Vec<_>>()
            .join(" ");
        let stmt = parse_statement(group, &raw)?;
        statements.push(stmt);
    }

    Ok(Program {
        statements,
        raw: raw_input.to_string(),
    })
}

fn split_on_semicolons(tokens: &[Spanned]) -> Vec<Vec<Spanned>> {
    let mut groups: Vec<Vec<Spanned>> = vec![vec![]];
    for t in tokens {
        if matches!(t.token, Token::Semicolon) {
            groups.push(vec![]);
        } else {
            groups.last_mut().unwrap().push(t.clone());
        }
    }
    groups
}

fn split_on_pipes(tokens: &[Spanned]) -> Vec<Vec<Spanned>> {
    let mut groups: Vec<Vec<Spanned>> = vec![vec![]];
    for t in tokens {
        if matches!(t.token, Token::Pipe) {
            groups.push(vec![]);
        } else {
            groups.last_mut().unwrap().push(t.clone());
        }
    }
    groups
}

fn parse_statement(tokens: Vec<Spanned>, raw: &str) -> Result<Statement, ParseError> {
    let pipe_groups = split_on_pipes(&tokens);
    if pipe_groups.is_empty() || pipe_groups[0].is_empty() {
        return Err(ParseError {
            position: 0,
            expected: vec!["a verb (SEARCH, INSERT, DELETE, ...)".into()],
            got: "empty statement".into(),
            hint: None,
        });
    }

    let head_tokens = pipe_groups[0].clone();
    let mut p = Parser::new(head_tokens);
    let head = parse_head(&mut p)?;

    // Remaining tokens after head parsing that weren't consumed
    // belong to conditions that follow on the head line.
    // But our design splits on pipes first, so the head parser
    // gets everything before the first pipe.
    // Any unconsumed tokens in the head group are an error.
    if !p.at_end() {
        let got = p.peek().cloned();
        return Err(p.error_with_hint(
            vec!["|".into(), ";".into(), "end of expression".into()],
            got.as_ref(),
            "unexpected tokens after statement head; use | to chain pipeline stages",
        ));
    }

    let mut pipeline = Vec::new();
    for group in &pipe_groups[1..] {
        if group.is_empty() {
            continue;
        }
        let stage = parse_pipe_stage(group)?;
        pipeline.push(stage);
    }

    Ok(Statement {
        head,
        pipeline,
        raw: raw.to_string(),
    })
}

// ── Head dispatch ──

fn parse_head(p: &mut Parser) -> Result<Head, ParseError> {
    match p.peek().cloned() {
        Some(Token::Keyword(Kw::Search)) => {
            p.advance();
            parse_search_head(p)
        }
        Some(Token::Keyword(Kw::Insert)) => {
            p.advance();
            parse_insert_head(p)
        }
        Some(Token::Keyword(Kw::Update)) => {
            p.advance();
            parse_update_head(p)
        }
        Some(Token::Keyword(Kw::Delete)) => {
            p.advance();
            parse_delete_head(p)
        }
        Some(Token::Keyword(Kw::Ingest)) => {
            p.advance();
            parse_ingest_head(p)
        }
        Some(Token::Keyword(Kw::Create)) => {
            p.advance();
            parse_create_head(p)
        }
        Some(Token::Keyword(Kw::Evaluate)) => {
            p.advance();
            p.expect_keyword(Kw::Policy)?;
            parse_eval_policy(p, false)
        }
        Some(Token::Keyword(Kw::Explain)) => {
            p.advance();
            p.expect_keyword(Kw::Policy)?;
            parse_eval_policy(p, true)
        }
        Some(Token::Keyword(Kw::Run)) => {
            p.advance();
            p.expect_keyword(Kw::Job)?;
            let name = p.expect_string()?;
            Ok(Head::RunJob(StringArg { value: name }))
        }
        Some(Token::Keyword(Kw::Pause)) => {
            p.advance();
            p.expect_keyword(Kw::Job)?;
            let name = p.expect_string()?;
            Ok(Head::PauseJob(StringArg { value: name }))
        }
        Some(Token::Keyword(Kw::Resume)) => {
            p.advance();
            p.expect_keyword(Kw::Job)?;
            let name = p.expect_string()?;
            Ok(Head::ResumeJob(StringArg { value: name }))
        }
        Some(Token::Keyword(Kw::List)) => {
            p.advance();
            parse_list_head(p)
        }
        Some(Token::Keyword(Kw::History)) => {
            p.advance();
            p.expect_keyword(Kw::Job)?;
            let name = p.expect_string()?;
            Ok(Head::HistoryJob(StringArg { value: name }))
        }
        Some(Token::Keyword(Kw::Config)) => {
            p.advance();
            p.expect_keyword(Kw::Agent)?;
            parse_config_agent(p)
        }
        Some(Token::Keyword(Kw::Resolve)) => {
            p.advance();
            p.expect_keyword(Kw::Session)?;
            let id = if let Some(Token::StringLit(_)) = p.peek() {
                Some(p.expect_string()?)
            } else {
                None
            };
            Ok(Head::ResolveSession(OptionalStringArg { value: id }))
        }
        Some(Token::Keyword(Kw::Promote)) => {
            p.advance();
            p.expect_keyword(Kw::Skill)?;
            let id = p.expect_string()?;
            Ok(Head::PromoteSkill(StringArg { value: id }))
        }
        Some(Token::Keyword(Kw::Embedding)) => {
            p.advance();
            parse_embedding_head(p)
        }
        Some(Token::Keyword(Kw::Exec)) => {
            p.advance();
            parse_exec_head(p)
        }
        other => Err(ParseError {
            position: p.current_pos(),
            expected: vec![
                "SEARCH", "INSERT", "UPDATE", "DELETE", "INGEST", "CREATE",
                "EVALUATE", "EXPLAIN", "RUN", "PAUSE", "RESUME", "LIST",
                "HISTORY", "CONFIG", "RESOLVE", "PROMOTE", "EMBEDDING", "EXEC",
            ]
            .into_iter()
            .map(String::from)
            .collect(),
            got: other.map(|t| format!("{t}")).unwrap_or("end of input".into()),
            hint: Some("every MPQ expression starts with a verb".into()),
        }),
    }
}

// ── SEARCH ──

fn parse_search_head(p: &mut Parser) -> Result<Head, ParseError> {
    let store_name = p.expect_ident()?;
    let store = Store::from_str(&store_name).ok_or_else(|| {
        p.error_with_hint(
            vec!["facts".into(), "knowledge".into(), "log".into(), "audit".into(), "activity".into()],
            Some(&Token::Ident(store_name.clone())),
            "valid stores: facts, knowledge, log, audit, activity",
        )
    })?;

    let mut query = None;
    let mut conditions = Vec::new();
    let mut mode = SearchMode::default();

    // Optional bare query string: SEARCH facts "some query"
    if let Some(Token::StringLit(_)) = p.peek() {
        query = Some(p.expect_string()?);
    }

    // Optional WHERE clause
    if p.match_keyword(Kw::Where) {
        conditions = parse_conditions(p)?;
    }

    // Grab trailing SINCE / BEFORE / MODE / SCOPE / AGENT that may appear
    // outside WHERE (or after WHERE conditions).
    loop {
        match p.peek() {
            Some(Token::Keyword(Kw::Since)) => {
                p.advance();
                let dur = parse_duration(p)?;
                conditions.push(Condition::Since(dur));
            }
            Some(Token::Keyword(Kw::Before)) => {
                p.advance();
                let dur = parse_duration(p)?;
                conditions.push(Condition::Before(dur));
            }
            Some(Token::Keyword(Kw::Mode)) => {
                p.advance();
                let mode_str = p.expect_ident()?;
                mode = SearchMode::from_str(&mode_str).ok_or_else(|| {
                    p.error_with_hint(
                        vec!["fts".into(), "vector".into(), "hybrid".into()],
                        Some(&Token::Ident(mode_str.clone())),
                        "valid search modes: fts, vector, hybrid",
                    )
                })?;
            }
            Some(Token::Keyword(Kw::Scope)) => {
                p.advance();
                let s = p.expect_ident()?;
                conditions.push(Condition::Scope(s));
            }
            Some(Token::Keyword(Kw::Agent)) => {
                p.advance();
                let a = p.expect_ident_or_string()?;
                conditions.push(Condition::Agent(a));
            }
            _ => break,
        }
    }

    Ok(Head::Search(SearchHead {
        store,
        query,
        conditions,
        mode,
    }))
}

// ── INSERT ──

fn parse_insert_head(p: &mut Parser) -> Result<Head, ParseError> {
    p.expect_keyword(Kw::Into)?;
    let store_name = p.expect_ident()?;
    let store = Store::from_str(&store_name).ok_or_else(|| {
        p.error_with_hint(
            vec!["facts".into()],
            Some(&Token::Ident(store_name.clone())),
            "INSERT currently supports: facts",
        )
    })?;

    p.match_token(&Token::LParen);

    let content = p.expect_string()?;
    let mut fields = Vec::new();

    while p.match_token(&Token::Comma) {
        let key = p.expect_ident()?;
        if !p.match_token(&Token::Eq) {
            return Err(p.error_with_hint(
                vec!["=".into()],
                p.peek(),
                "key=value pairs after content string: topic=\"value\"",
            ));
        }
        let val = parse_literal(p)?;
        fields.push((key, val));
    }

    // Optional closing paren
    p.match_token(&Token::RParen);

    Ok(Head::Insert(InsertHead {
        store,
        content,
        fields,
    }))
}

// ── UPDATE ──

fn parse_update_head(p: &mut Parser) -> Result<Head, ParseError> {
    let store_name = p.expect_ident()?;
    let store = Store::from_str(&store_name).ok_or_else(|| {
        p.error_with_hint(
            vec!["facts".into()],
            Some(&Token::Ident(store_name.clone())),
            "UPDATE currently supports: facts",
        )
    })?;

    p.expect_keyword(Kw::Set)?;
    let mut assignments = Vec::new();
    loop {
        let key = p.expect_ident()?;
        if !p.match_token(&Token::Eq) {
            return Err(p.error(vec!["=".into()], p.peek()));
        }
        let val = parse_literal(p)?;
        assignments.push((key, val));
        if !p.match_token(&Token::Comma) {
            break;
        }
    }

    let mut conditions = Vec::new();
    if p.match_keyword(Kw::Where) {
        conditions = parse_conditions(p)?;
    }

    Ok(Head::Update(UpdateHead {
        store,
        assignments,
        conditions,
    }))
}

// ── DELETE ──

fn parse_delete_head(p: &mut Parser) -> Result<Head, ParseError> {
    p.expect_keyword(Kw::From)?;
    let store_name = p.expect_ident()?;
    let store = Store::from_str(&store_name).ok_or_else(|| {
        p.error_with_hint(
            vec!["facts".into()],
            Some(&Token::Ident(store_name.clone())),
            "DELETE currently supports: facts",
        )
    })?;

    p.expect_keyword(Kw::Where)?;
    let conditions = parse_conditions(p)?;
    if conditions.is_empty() {
        return Err(p.error_with_hint(
            vec!["at least one condition".into()],
            p.peek(),
            "DELETE requires a WHERE clause with conditions to prevent accidental bulk deletion",
        ));
    }

    Ok(Head::Delete(DeleteHead { store, conditions }))
}

// ── INGEST ──

fn parse_ingest_head(p: &mut Parser) -> Result<Head, ParseError> {
    // INGEST EVENTS "source" [FROM "path"]
    if p.match_keyword(Kw::Events) {
        let source = p.expect_string()?;
        let file_path = if p.match_keyword(Kw::From) {
            Some(p.expect_string()?)
        } else {
            None
        };
        return Ok(Head::IngestEvents(IngestEventsHead { source, file_path }));
    }

    let url = p.expect_string()?;
    let name = if p.match_keyword(Kw::As) {
        Some(p.expect_string()?)
    } else {
        None
    };
    Ok(Head::Ingest(IngestHead { url, name }))
}

// ── EXEC ──

fn parse_exec_head(p: &mut Parser) -> Result<Head, ParseError> {
    let op = p.expect_string()?;
    let args = match p.peek().cloned() {
        Some(Token::JsonBlob(blob)) => {
            p.advance();
            serde_json::from_str(&blob).map_err(|e| ParseError {
                position: p.current_pos(),
                expected: vec!["valid JSON object".into()],
                got: format!("invalid JSON: {e}"),
                hint: Some("EXEC args must be a valid JSON object, e.g. {\"key\": \"value\"}".into()),
            })?
        }
        _ => serde_json::json!({}),
    };
    Ok(Head::Exec(ExecHead { op, args }))
}

// ── CREATE dispatch ──

fn parse_create_head(p: &mut Parser) -> Result<Head, ParseError> {
    match p.peek().cloned() {
        Some(Token::Keyword(Kw::Policy)) => {
            p.advance();
            parse_create_policy(p)
        }
        Some(Token::Keyword(Kw::Job)) => {
            p.advance();
            parse_create_job(p)
        }
        Some(Token::Keyword(Kw::Agent)) => {
            p.advance();
            parse_create_agent(p)
        }
        Some(Token::Keyword(Kw::Skill)) => {
            p.advance();
            let content = p.expect_string()?;
            Ok(Head::CreateSkill(StringArg { value: content }))
        }
        Some(Token::Keyword(Kw::Tool)) => {
            p.advance();
            parse_create_tool(p)
        }
        other => Err(ParseError {
            position: p.current_pos(),
            expected: vec!["POLICY", "JOB", "AGENT", "SKILL", "TOOL"]
                .into_iter()
                .map(String::from)
                .collect(),
            got: other.map(|t| format!("{t}")).unwrap_or("end of input".into()),
            hint: Some("CREATE must be followed by a resource type".into()),
        }),
    }
}

fn parse_create_policy(p: &mut Parser) -> Result<Head, ParseError> {
    let name = p.expect_string()?;
    let effect_str = p.expect_ident()?;
    let effect = PolicyEffect::from_str(&effect_str).ok_or_else(|| {
        p.error_with_hint(
            vec!["allow".into(), "deny".into(), "audit".into()],
            Some(&Token::Ident(effect_str.clone())),
            "policy effect must be allow, deny, or audit",
        )
    })?;

    let action = p.expect_ident()?;
    p.expect_keyword(Kw::On)?;
    let resource = p.expect_ident()?;

    let agent = if p.match_keyword(Kw::For) {
        p.expect_keyword(Kw::Agent)?;
        Some(p.expect_string()?)
    } else {
        None
    };

    let message = if p.match_keyword(Kw::Message) {
        Some(p.expect_string()?)
    } else {
        None
    };

    Ok(Head::CreatePolicy(CreatePolicyHead {
        name,
        effect,
        action,
        resource,
        agent,
        message,
    }))
}

fn parse_create_job(p: &mut Parser) -> Result<Head, ParseError> {
    let name = p.expect_string()?;
    p.expect_keyword(Kw::Schedule)?;
    let schedule = p.expect_string()?;

    let job_type = if p.match_keyword(Kw::Type) {
        Some(p.expect_ident()?)
    } else {
        None
    };

    let payload = if p.match_keyword(Kw::Payload) {
        match p.peek().cloned() {
            Some(Token::JsonBlob(s)) => {
                p.advance();
                Some(s)
            }
            Some(Token::StringLit(s)) => {
                p.advance();
                Some(s)
            }
            _ => None,
        }
    } else {
        None
    };

    Ok(Head::CreateJob(CreateJobHead {
        name,
        schedule,
        job_type,
        payload,
    }))
}

fn parse_create_agent(p: &mut Parser) -> Result<Head, ParseError> {
    let name = p.expect_string()?;
    let mut config = Vec::new();
    if p.match_keyword(Kw::Config) {
        config = parse_key_value_list(p)?;
    }
    Ok(Head::CreateAgent(CreateAgentHead { name, config }))
}

fn parse_config_agent(p: &mut Parser) -> Result<Head, ParseError> {
    let name = p.expect_string()?;
    p.expect_keyword(Kw::Set)?;
    let assignments = parse_key_value_list(p)?;
    Ok(Head::ConfigAgent(ConfigAgentHead { name, assignments }))
}

fn parse_create_tool(p: &mut Parser) -> Result<Head, ParseError> {
    let name = p.expect_string()?;
    p.expect_keyword(Kw::Language)?;
    let language = p.expect_ident()?;
    p.expect_keyword(Kw::Body)?;
    let body = p.expect_string()?;
    Ok(Head::CreateTool(CreateToolHead {
        name,
        language,
        body,
    }))
}

// ── EVALUATE / EXPLAIN POLICY ──

fn parse_eval_policy(p: &mut Parser, is_explain: bool) -> Result<Head, ParseError> {
    let kw = if is_explain { Kw::For } else { Kw::On };
    p.expect_keyword(kw)?;
    p.match_token(&Token::LParen);

    let actor = p.expect_string()?;
    p.match_token(&Token::Comma);
    let action = p.expect_string()?;
    p.match_token(&Token::Comma);
    let resource = p.expect_string()?;

    p.match_token(&Token::RParen);

    let head = EvalPolicyHead {
        actor,
        action,
        resource,
    };
    if is_explain {
        Ok(Head::ExplainPolicy(head))
    } else {
        Ok(Head::EvaluatePolicy(head))
    }
}

// ── LIST dispatch ──

fn parse_list_head(p: &mut Parser) -> Result<Head, ParseError> {
    match p.peek().cloned() {
        Some(Token::Keyword(Kw::Job)) => {
            p.advance();
            // LIST JOBS — the 'S' might be absent, that's fine
            Ok(Head::ListJobs)
        }
        Some(Token::Ident(ref s)) if s.eq_ignore_ascii_case("jobs") => {
            p.advance();
            Ok(Head::ListJobs)
        }
        Some(Token::Keyword(Kw::Session)) => {
            p.advance();
            Ok(Head::ListSessions)
        }
        Some(Token::Ident(ref s)) if s.eq_ignore_ascii_case("sessions") => {
            p.advance();
            Ok(Head::ListSessions)
        }
        Some(Token::Keyword(Kw::Tool)) => {
            p.advance();
            Ok(Head::ListTools)
        }
        Some(Token::Ident(ref s)) if s.eq_ignore_ascii_case("tools") => {
            p.advance();
            Ok(Head::ListTools)
        }
        other => Err(ParseError {
            position: p.current_pos(),
            expected: vec!["JOBS", "SESSIONS", "TOOLS"]
                .into_iter()
                .map(String::from)
                .collect(),
            got: other.map(|t| format!("{t}")).unwrap_or("end of input".into()),
            hint: Some("LIST must be followed by a resource type: JOBS, SESSIONS, TOOLS".into()),
        }),
    }
}

// ── EMBEDDING dispatch ──

fn parse_embedding_head(p: &mut Parser) -> Result<Head, ParseError> {
    match p.peek().cloned() {
        Some(Token::Keyword(Kw::Status)) => {
            p.advance();
            Ok(Head::EmbeddingStatus)
        }
        Some(Token::Keyword(Kw::Retry)) => {
            p.advance();
            p.expect_keyword(Kw::Dead)?;
            Ok(Head::EmbeddingRetryDead)
        }
        Some(Token::Keyword(Kw::Backfill)) => {
            p.advance();
            Ok(Head::EmbeddingBackfill)
        }
        other => Err(ParseError {
            position: p.current_pos(),
            expected: vec!["STATUS", "RETRY", "BACKFILL"]
                .into_iter()
                .map(String::from)
                .collect(),
            got: other.map(|t| format!("{t}")).unwrap_or("end of input".into()),
            hint: Some("EMBEDDING STATUS, EMBEDDING RETRY DEAD, or EMBEDDING BACKFILL".into()),
        }),
    }
}

// ── Pipeline stages ──

fn parse_pipe_stage(tokens: &[Spanned]) -> Result<PipeStage, ParseError> {
    if tokens.is_empty() {
        return Err(ParseError {
            position: 0,
            expected: vec!["SORT, TAKE, OFFSET, COUNT, PROCESS".into()],
            got: "empty pipeline stage".into(),
            hint: None,
        });
    }
    let mut p = Parser::new(tokens.to_vec());
    let stage = match p.peek().cloned() {
        Some(Token::Keyword(Kw::Sort)) => {
            p.advance();
            let field = p.expect_ident()?;
            let order = if p.match_keyword(Kw::Desc) {
                SortOrder::Desc
            } else {
                p.match_keyword(Kw::Asc);
                SortOrder::Asc
            };
            PipeStage::Sort { field, order }
        }
        Some(Token::Keyword(Kw::Take)) => {
            p.advance();
            let n = p.expect_int()? as usize;
            PipeStage::Take(n)
        }
        Some(Token::Keyword(Kw::Offset)) => {
            p.advance();
            let n = p.expect_int()? as usize;
            PipeStage::Offset(n)
        }
        Some(Token::Keyword(Kw::Count)) => {
            p.advance();
            PipeStage::Count
        }
        Some(Token::Keyword(Kw::Process)) => {
            p.advance();
            PipeStage::Process
        }
        other => {
            return Err(ParseError {
                position: tokens.first().map(|t| t.pos).unwrap_or(0),
                expected: vec!["SORT", "TAKE", "OFFSET", "COUNT", "PROCESS"]
                    .into_iter()
                    .map(String::from)
                    .collect(),
                got: other.map(|t| format!("{t}")).unwrap_or("end of input".into()),
                hint: Some("pipeline stages after | must be SORT, TAKE, OFFSET, COUNT, or PROCESS".into()),
            });
        }
    };

    if !p.at_end() {
        let got = p.peek().cloned();
        return Err(p.error_with_hint(
            vec!["|".into(), ";".into()],
            got.as_ref(),
            "unexpected tokens after pipeline stage",
        ));
    }

    Ok(stage)
}

// ── Conditions (flat AND-joined list) ──

fn parse_conditions(p: &mut Parser) -> Result<Vec<Condition>, ParseError> {
    let mut conditions = Vec::new();
    conditions.push(parse_single_condition(p)?);

    while p.match_keyword(Kw::And) {
        conditions.push(parse_single_condition(p)?);
    }

    Ok(conditions)
}

fn parse_single_condition(p: &mut Parser) -> Result<Condition, ParseError> {
    // Check for special conditions first
    match p.peek() {
        Some(Token::Keyword(Kw::Scope)) => {
            p.advance();
            let s = p.expect_ident()?;
            return Ok(Condition::Scope(s));
        }
        Some(Token::Keyword(Kw::Agent)) => {
            p.advance();
            let a = p.expect_ident_or_string()?;
            return Ok(Condition::Agent(a));
        }
        Some(Token::Keyword(Kw::Since)) => {
            p.advance();
            let dur = parse_duration(p)?;
            return Ok(Condition::Since(dur));
        }
        Some(Token::Keyword(Kw::Before)) => {
            p.advance();
            let dur = parse_duration(p)?;
            return Ok(Condition::Before(dur));
        }
        _ => {}
    }

    // field op value
    let field = p.expect_ident()?;

    // LIKE special case
    if p.match_keyword(Kw::Like) {
        let pattern = p.expect_string()?;
        return Ok(Condition::Like { field, pattern });
    }

    let op = match p.peek().cloned() {
        Some(Token::Eq) => CmpOp::Eq,
        Some(Token::Ne) => CmpOp::Ne,
        Some(Token::Gt) => CmpOp::Gt,
        Some(Token::Lt) => CmpOp::Lt,
        Some(Token::Ge) => CmpOp::Ge,
        Some(Token::Le) => CmpOp::Le,
        other => {
            return Err(p.error_with_hint(
                vec!["=".into(), "!=".into(), ">".into(), "<".into(), ">=".into(), "<=".into(), "LIKE".into()],
                other.as_ref(),
                "comparison operator expected after field name",
            ));
        }
    };
    p.advance();

    let value = parse_literal(p)?;
    Ok(Condition::Cmp { field, op, value })
}

// ── Shared helpers ──

fn parse_literal(p: &mut Parser) -> Result<Literal, ParseError> {
    match p.peek().cloned() {
        Some(Token::StringLit(s)) => {
            p.advance();
            Ok(Literal::Str(s))
        }
        Some(Token::IntLit(n)) => {
            p.advance();
            Ok(Literal::Int(n))
        }
        Some(Token::FloatLit(n)) => {
            p.advance();
            Ok(Literal::Float(n))
        }
        Some(Token::BoolLit(b)) => {
            p.advance();
            Ok(Literal::Bool(b))
        }
        other => Err(p.error(
            vec!["string, number, or boolean".into()],
            other.as_ref(),
        )),
    }
}

fn parse_duration(p: &mut Parser) -> Result<DurationLit, ParseError> {
    match p.peek().cloned() {
        Some(Token::DurationLit(amount, unit_ch)) => {
            p.advance();
            let unit = match unit_ch {
                'd' => DurationUnit::Days,
                'h' => DurationUnit::Hours,
                'm' => DurationUnit::Minutes,
                's' => DurationUnit::Seconds,
                _ => {
                    return Err(p.error_with_hint(
                        vec!["duration (e.g. 7d, 24h, 30m, 90s)".into()],
                        Some(&Token::DurationLit(amount, unit_ch)),
                        "valid duration units: d (days), h (hours), m (minutes), s (seconds)",
                    ));
                }
            };
            Ok(DurationLit { amount, unit })
        }
        other => Err(p.error_with_hint(
            vec!["duration (e.g. 7d, 24h, 30m)".into()],
            other.as_ref(),
            "durations are a number followed by d/h/m/s, e.g. 7d, 24h, 30m",
        )),
    }
}

fn parse_key_value_list(p: &mut Parser) -> Result<Vec<(String, Literal)>, ParseError> {
    let mut pairs = Vec::new();
    loop {
        if p.at_end() {
            break;
        }
        match p.peek() {
            Some(Token::Ident(_)) | Some(Token::Keyword(_)) => {}
            _ => break,
        }
        let key = p.expect_ident()?;
        if !p.match_token(&Token::Eq) {
            return Err(p.error(vec!["=".into()], p.peek()));
        }
        let val = parse_literal(p)?;
        pairs.push((key, val));
        if !p.match_token(&Token::Comma) {
            break;
        }
    }
    Ok(pairs)
}

// ── Tests ──

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dsl::lexer::lex;

    fn parse_expr(input: &str) -> Result<Program, ParseError> {
        let tokens = lex(input).map_err(|e| ParseError {
            position: e.pos,
            expected: vec![],
            got: e.message,
            hint: None,
        })?;
        parse(tokens, input)
    }

    #[test]
    fn parse_simple_search() {
        let prog = parse_expr(r#"SEARCH facts WHERE topic = "auth" SINCE 7d"#).unwrap();
        assert_eq!(prog.statements.len(), 1);
        match &prog.statements[0].head {
            Head::Search(s) => {
                assert_eq!(s.store, Store::Facts);
                assert_eq!(s.conditions.len(), 2); // topic = "auth" + SINCE 7d
            }
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn parse_search_with_pipeline() {
        let prog = parse_expr(
            r#"SEARCH facts WHERE confidence > 0.5 | SORT confidence DESC | TAKE 10"#,
        )
        .unwrap();
        assert_eq!(prog.statements[0].pipeline.len(), 2);
        assert!(matches!(
            prog.statements[0].pipeline[0],
            PipeStage::Sort { ref field, order: SortOrder::Desc } if field == "confidence"
        ));
        assert!(matches!(prog.statements[0].pipeline[1], PipeStage::Take(10)));
    }

    #[test]
    fn parse_bare_search_query() {
        let prog = parse_expr(r#"SEARCH facts "authentication patterns""#).unwrap();
        match &prog.statements[0].head {
            Head::Search(s) => {
                assert_eq!(s.query.as_deref(), Some("authentication patterns"));
            }
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn parse_insert() {
        let prog =
            parse_expr(r#"INSERT INTO facts ("Redis is great", topic="infra", confidence=0.9)"#)
                .unwrap();
        match &prog.statements[0].head {
            Head::Insert(ins) => {
                assert_eq!(ins.content, "Redis is great");
                assert_eq!(ins.fields.len(), 2);
                assert_eq!(ins.fields[0].0, "topic");
            }
            _ => panic!("expected Insert"),
        }
    }

    #[test]
    fn parse_update() {
        let prog =
            parse_expr(r#"UPDATE facts SET confidence = 0.5 WHERE id = "abc""#).unwrap();
        match &prog.statements[0].head {
            Head::Update(u) => {
                assert_eq!(u.assignments.len(), 1);
                assert_eq!(u.conditions.len(), 1);
            }
            _ => panic!("expected Update"),
        }
    }

    #[test]
    fn parse_delete() {
        let prog = parse_expr(r#"DELETE FROM facts WHERE confidence < 0.3"#).unwrap();
        assert!(matches!(prog.statements[0].head, Head::Delete(_)));
    }

    #[test]
    fn parse_create_policy() {
        let prog = parse_expr(
            r#"CREATE POLICY "no-deletes" deny DELETE ON facts FOR AGENT "junior" MESSAGE "no deletes""#,
        )
        .unwrap();
        match &prog.statements[0].head {
            Head::CreatePolicy(cp) => {
                assert_eq!(cp.name, "no-deletes");
                assert_eq!(cp.effect, PolicyEffect::Deny);
                assert_eq!(cp.action, "delete");
                assert_eq!(cp.agent.as_deref(), Some("junior"));
                assert_eq!(cp.message.as_deref(), Some("no deletes"));
            }
            _ => panic!("expected CreatePolicy"),
        }
    }

    #[test]
    fn parse_evaluate_policy() {
        let prog = parse_expr(r#"EVALUATE POLICY ON ("bot", "delete", "facts")"#).unwrap();
        assert!(matches!(prog.statements[0].head, Head::EvaluatePolicy(_)));
    }

    #[test]
    fn parse_create_job() {
        let prog =
            parse_expr(r#"CREATE JOB "digest" SCHEDULE "0 9 * * *" TYPE prompt"#).unwrap();
        match &prog.statements[0].head {
            Head::CreateJob(j) => {
                assert_eq!(j.name, "digest");
                assert_eq!(j.schedule, "0 9 * * *");
                assert_eq!(j.job_type.as_deref(), Some("prompt"));
            }
            _ => panic!("expected CreateJob"),
        }
    }

    #[test]
    fn parse_run_pause_resume() {
        assert!(parse_expr(r#"RUN JOB "daily""#).is_ok());
        assert!(parse_expr(r#"PAUSE JOB "daily""#).is_ok());
        assert!(parse_expr(r#"RESUME JOB "daily""#).is_ok());
    }

    #[test]
    fn parse_multi_statement() {
        let prog = parse_expr(
            r#"DELETE FROM facts WHERE id = "old"; SEARCH facts WHERE topic = "auth" | TAKE 5"#,
        )
        .unwrap();
        assert_eq!(prog.statements.len(), 2);
        assert!(matches!(prog.statements[0].head, Head::Delete(_)));
        assert!(matches!(prog.statements[1].head, Head::Search(_)));
        assert_eq!(prog.statements[1].pipeline.len(), 1);
    }

    #[test]
    fn parse_list_jobs() {
        assert!(parse_expr("LIST JOBS").is_ok());
        assert!(parse_expr("LIST TOOLS").is_ok());
        assert!(parse_expr("LIST SESSIONS").is_ok());
    }

    #[test]
    fn parse_embedding_ops() {
        assert!(parse_expr("EMBEDDING STATUS").is_ok());
        assert!(parse_expr("EMBEDDING RETRY DEAD").is_ok());
        assert!(parse_expr("EMBEDDING BACKFILL").is_ok());
        let prog = parse_expr("EMBEDDING BACKFILL | PROCESS").unwrap();
        assert_eq!(prog.statements[0].pipeline.len(), 1);
    }

    #[test]
    fn parse_ingest() {
        let prog = parse_expr(r#"INGEST "https://example.com/doc" AS "my-doc""#).unwrap();
        match &prog.statements[0].head {
            Head::Ingest(i) => {
                assert_eq!(i.url, "https://example.com/doc");
                assert_eq!(i.name.as_deref(), Some("my-doc"));
            }
            _ => panic!("expected Ingest"),
        }
    }

    #[test]
    fn parse_create_agent() {
        let prog =
            parse_expr(r#"CREATE AGENT "reviewer" CONFIG model="claude-3", temperature=0.2"#)
                .unwrap();
        match &prog.statements[0].head {
            Head::CreateAgent(a) => {
                assert_eq!(a.name, "reviewer");
                assert_eq!(a.config.len(), 2);
            }
            _ => panic!("expected CreateAgent"),
        }
    }

    #[test]
    fn parse_count_pipeline() {
        let prog = parse_expr("SEARCH facts | COUNT").unwrap();
        assert!(matches!(prog.statements[0].pipeline[0], PipeStage::Count));
    }

    #[test]
    fn parse_error_invalid_verb() {
        let err = parse_expr("YEET facts").unwrap_err();
        assert!(err.got.contains("YEET") || err.got.contains("yeet"));
    }

    #[test]
    fn parse_error_missing_where_on_delete() {
        let err = parse_expr("DELETE FROM facts").unwrap_err();
        assert!(err.expected.iter().any(|e| e.contains("WHERE")));
    }

    #[test]
    fn parse_search_mode() {
        let prog = parse_expr(r#"SEARCH facts "query" MODE fts | TAKE 5"#).unwrap();
        match &prog.statements[0].head {
            Head::Search(s) => assert_eq!(s.mode, SearchMode::Fts),
            _ => panic!("expected Search"),
        }
    }

    #[test]
    fn parse_exec_with_args() {
        let prog = parse_expr(r#"EXEC "ingest.events" {"source": "cursor", "file_path": "/tmp/test.jsonl"}"#).unwrap();
        match &prog.statements[0].head {
            Head::Exec(e) => {
                assert_eq!(e.op, "ingest.events");
                assert_eq!(e.args["source"], "cursor");
                assert_eq!(e.args["file_path"], "/tmp/test.jsonl");
            }
            _ => panic!("expected Exec"),
        }
    }

    #[test]
    fn parse_exec_no_args() {
        let prog = parse_expr(r#"EXEC "embedding.status""#).unwrap();
        match &prog.statements[0].head {
            Head::Exec(e) => {
                assert_eq!(e.op, "embedding.status");
                assert!(e.args.as_object().unwrap().is_empty());
            }
            _ => panic!("expected Exec"),
        }
    }

    #[test]
    fn parse_exec_in_multi_statement() {
        let prog = parse_expr(r#"EXEC "ingest.status" {"limit": 5}; SEARCH facts | TAKE 3"#).unwrap();
        assert_eq!(prog.statements.len(), 2);
        assert!(matches!(prog.statements[0].head, Head::Exec(_)));
        assert!(matches!(prog.statements[1].head, Head::Search(_)));
    }

    #[test]
    fn parse_ingest_events_basic() {
        let prog = parse_expr(r#"INGEST EVENTS "cursor""#).unwrap();
        match &prog.statements[0].head {
            Head::IngestEvents(ie) => {
                assert_eq!(ie.source, "cursor");
                assert!(ie.file_path.is_none());
            }
            _ => panic!("expected IngestEvents"),
        }
    }

    #[test]
    fn parse_ingest_events_with_path() {
        let prog = parse_expr(r#"INGEST EVENTS "cursor" FROM "/tmp/events.jsonl""#).unwrap();
        match &prog.statements[0].head {
            Head::IngestEvents(ie) => {
                assert_eq!(ie.source, "cursor");
                assert_eq!(ie.file_path.as_deref(), Some("/tmp/events.jsonl"));
            }
            _ => panic!("expected IngestEvents"),
        }
    }

    #[test]
    fn parse_ingest_content_inline() {
        let prog = parse_expr(r##"INGEST "My Document content here" AS "my-doc""##).unwrap();
        match &prog.statements[0].head {
            Head::Ingest(i) => {
                assert!(i.url.starts_with("My Document"));
                assert_eq!(i.name.as_deref(), Some("my-doc"));
            }
            _ => panic!("expected Ingest"),
        }
    }

    #[test]
    fn parse_ingest_file_uri() {
        let prog = parse_expr(r#"INGEST "file:///tmp/doc.md""#).unwrap();
        match &prog.statements[0].head {
            Head::Ingest(i) => {
                assert_eq!(i.url, "file:///tmp/doc.md");
            }
            _ => panic!("expected Ingest"),
        }
    }
}
