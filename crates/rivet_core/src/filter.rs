use chrono::{
  DateTime,
  Days,
  Utc
};
use tracing::trace;

use crate::datetime::{
  parse_date_expr,
  to_project_date
};
use crate::task::{
  Status,
  Task
};

#[derive(Debug, Clone)]
pub enum Pred {
  Id(u64),
  Uuid(uuid::Uuid),
  TagInclude(String),
  TagExclude(String),
  VirtualTagInclude(VirtualTag),
  VirtualTagExclude(VirtualTag),
  ProjectEq(String),
  StatusEq(Status),
  Waiting,
  DueBefore(DateTime<Utc>),
  DueAfter(DateTime<Utc>),
  TextContains(String)
}

#[derive(Debug, Clone, Copy)]
pub enum VirtualTag {
  Pending,
  Waiting,
  Completed,
  Deleted,
  Active,
  Ready,
  Blocked,
  Unblocked,
  Due,
  Overdue,
  Today,
  Tomorrow
}

#[derive(Debug, Clone)]
enum Expr {
  True,
  Pred(Pred),
  And(Vec<Expr>),
  Or(Vec<Expr>)
}

#[derive(Debug, Clone)]
pub struct Filter {
  expr: Expr
}

impl Default for Filter {
  fn default() -> Self {
    Self {
      expr: Expr::True
    }
  }
}

impl Filter {
  #[tracing::instrument(skip(
    terms, now
  ))]
  pub fn parse(
    terms: &[String],
    now: DateTime<Utc>
  ) -> anyhow::Result<Self> {
    if terms.is_empty() {
      return Ok(Self::default());
    }

    let tokens = lex_terms(terms);
    let mut parser =
      Parser::new(tokens, now);
    let expr = parser.parse_expr()?;
    parser.ensure_end()?;

    Ok(Self {
      expr
    })
  }

  #[tracing::instrument(skip(
    self, task, now
  ))]
  pub fn matches(
    &self,
    task: &Task,
    now: DateTime<Utc>
  ) -> bool {
    let ok =
      eval_expr(&self.expr, task, now);
    if !ok {
      return false;
    }

    if task.is_waiting(now)
            && !expr_has_explicit_status_filter(&self.expr)
            && !expr_has_identity_selector(&self.expr)
        {
            return false;
        }

    true
  }

  #[tracing::instrument(skip(
    self, task, now
  ))]
  pub fn matches_without_waiting_guard(
    &self,
    task: &Task,
    now: DateTime<Utc>
  ) -> bool {
    eval_expr(&self.expr, task, now)
  }

  pub fn has_explicit_status_filter(
    &self
  ) -> bool {
    expr_has_explicit_status_filter(
      &self.expr
    )
  }

  pub fn has_identity_selector(
    &self
  ) -> bool {
    expr_has_identity_selector(
      &self.expr
    )
  }
}

struct Parser {
  tokens: Vec<String>,
  pos:    usize,
  now:    DateTime<Utc>
}

impl Parser {
  fn new(
    tokens: Vec<String>,
    now: DateTime<Utc>
  ) -> Self {
    Self {
      tokens,
      pos: 0,
      now
    }
  }

  fn parse_expr(
    &mut self
  ) -> anyhow::Result<Expr> {
    self.parse_or()
  }

  fn parse_or(
    &mut self
  ) -> anyhow::Result<Expr> {
    let mut nodes =
      vec![self.parse_and()?];

    while self.match_any(&["or", "||"])
    {
      nodes.push(self.parse_and()?);
    }

    if nodes.len() == 1 {
      Ok(nodes.remove(0))
    } else {
      Ok(Expr::Or(nodes))
    }
  }

  fn parse_and(
    &mut self
  ) -> anyhow::Result<Expr> {
    let mut nodes =
      vec![self.parse_primary()?];

    loop {
      if self.match_any(&["and", "&&"])
      {
        nodes
          .push(self.parse_primary()?);
        continue;
      }

      if self
        .peek_is_implicit_and_boundary()
      {
        nodes
          .push(self.parse_primary()?);
        continue;
      }

      break;
    }

    if nodes.len() == 1 {
      Ok(nodes.remove(0))
    } else {
      Ok(Expr::And(nodes))
    }
  }

  fn parse_primary(
    &mut self
  ) -> anyhow::Result<Expr> {
    if self.match_token("(") {
      let inner = self.parse_expr()?;
      self.expect_token(")")?;
      return Ok(inner);
    }

    let token = self
      .next_token()
      .ok_or_else(|| {
        anyhow::anyhow!(
          "unexpected end of filter \
           expression"
        )
      })?;

    if token == ")" {
      return Err(anyhow::anyhow!(
        "unexpected ')' in filter \
         expression"
      ));
    }

    let pred =
      parse_atom(&token, self.now)?;
    Ok(Expr::Pred(pred))
  }

  fn ensure_end(
    &self
  ) -> anyhow::Result<()> {
    if self.pos < self.tokens.len() {
      Err(anyhow::anyhow!(
        "unexpected token in filter \
         expression: {}",
        self.tokens[self.pos]
      ))
    } else {
      Ok(())
    }
  }

  fn match_token(
    &mut self,
    expected: &str
  ) -> bool {
    let Some(tok) =
      self.tokens.get(self.pos)
    else {
      return false;
    };
    if tok
      .eq_ignore_ascii_case(expected)
    {
      self.pos += 1;
      true
    } else {
      false
    }
  }

  fn match_any(
    &mut self,
    options: &[&str]
  ) -> bool {
    options
      .iter()
      .any(|opt| self.match_token(opt))
  }

  fn expect_token(
    &mut self,
    expected: &str
  ) -> anyhow::Result<()> {
    if self.match_token(expected) {
      Ok(())
    } else {
      Err(anyhow::anyhow!(
        "expected '{expected}' in \
         filter expression"
      ))
    }
  }

  fn next_token(
    &mut self
  ) -> Option<String> {
    let out = self
      .tokens
      .get(self.pos)
      .cloned();
    if out.is_some() {
      self.pos += 1;
    }
    out
  }

  fn peek_is_implicit_and_boundary(
    &self
  ) -> bool {
    let Some(tok) =
      self.tokens.get(self.pos)
    else {
      return false;
    };

    if tok.eq_ignore_ascii_case("and")
      || tok.eq_ignore_ascii_case("&&")
    {
      return false;
    }

    !tok.eq_ignore_ascii_case("or")
      && !tok.eq_ignore_ascii_case("||")
      && !tok.eq_ignore_ascii_case(")")
  }
}

fn lex_terms(
  terms: &[String]
) -> Vec<String> {
  let mut out = Vec::new();

  for term in terms {
    let mut current = String::new();
    for ch in term.chars() {
      if ch == '(' || ch == ')' {
        if !current.is_empty() {
          out.push(current.clone());
          current.clear();
        }
        out.push(ch.to_string());
      } else {
        current.push(ch);
      }
    }

    if !current.is_empty() {
      out.push(current);
    }
  }

  out
}

fn parse_atom(
  term: &str,
  now: DateTime<Utc>
) -> anyhow::Result<Pred> {
  if let Some(tag) =
    term.strip_prefix('+')
  {
    if let Some(virtual_tag) =
      parse_virtual_tag(tag)
    {
      return Ok(
        Pred::VirtualTagInclude(
          virtual_tag
        )
      );
    }
    return Ok(Pred::TagInclude(
      tag.to_string()
    ));
  }
  if let Some(tag) =
    term.strip_prefix('-')
  {
    if let Some(virtual_tag) =
      parse_virtual_tag(tag)
    {
      return Ok(
        Pred::VirtualTagExclude(
          virtual_tag
        )
      );
    }
    return Ok(Pred::TagExclude(
      tag.to_string()
    ));
  }
  if let Ok(id) = term.parse::<u64>() {
    return Ok(Pred::Id(id));
  }
  if let Ok(uuid) =
    uuid::Uuid::parse_str(term)
  {
    return Ok(Pred::Uuid(uuid));
  }

  if let Some(project) =
    term.strip_prefix("project:")
  {
    return Ok(Pred::ProjectEq(
      project.to_string()
    ));
  }

  if let Some(status_text) =
    term.strip_prefix("status:")
  {
    return Ok(
      match status_text
        .to_ascii_lowercase()
        .as_str()
      {
        | "pending" => {
          Pred::StatusEq(
            Status::Pending
          )
        }
        | "completed" => {
          Pred::StatusEq(
            Status::Completed
          )
        }
        | "deleted" => {
          Pred::StatusEq(
            Status::Deleted
          )
        }
        | "waiting" => Pred::Waiting,
        | _ => {
          Pred::TextContains(
            term.to_string()
          )
        }
      }
    );
  }

  if let Some(value) =
    term.strip_prefix("due.before:")
  {
    return Ok(Pred::DueBefore(
      parse_date_expr(value, now)?
    ));
  }

  if let Some(value) =
    term.strip_prefix("due.after:")
  {
    return Ok(Pred::DueAfter(
      parse_date_expr(value, now)?
    ));
  }

  Ok(Pred::TextContains(
    term.to_string()
  ))
}

fn eval_expr(
  expr: &Expr,
  task: &Task,
  now: DateTime<Utc>
) -> bool {
  match expr {
    | Expr::True => true,
    | Expr::Pred(pred) => {
      eval_pred(pred, task, now)
    }
    | Expr::And(nodes) => {
      nodes.iter().all(|node| {
        eval_expr(node, task, now)
      })
    }
    | Expr::Or(nodes) => {
      nodes.iter().any(|node| {
        eval_expr(node, task, now)
      })
    }
  }
}

fn eval_pred(
  pred: &Pred,
  task: &Task,
  now: DateTime<Utc>
) -> bool {
  let ok = match pred {
    | Pred::Id(id) => {
      task.id == Some(*id)
    }
    | Pred::Uuid(uuid) => {
      task.uuid == *uuid
    }
    | Pred::TagInclude(tag) => {
      task.tags.iter().any(|t| t == tag)
    }
    | Pred::TagExclude(tag) => {
      task.tags.iter().all(|t| t != tag)
    }
    | Pred::VirtualTagInclude(
      virtual_tag
    ) => {
      eval_virtual_tag(
        *virtual_tag,
        task,
        now
      )
    }
    | Pred::VirtualTagExclude(
      virtual_tag
    ) => {
      !eval_virtual_tag(
        *virtual_tag,
        task,
        now
      )
    }
    | Pred::ProjectEq(project) => {
      task.project.as_deref()
        == Some(project.as_str())
    }
    | Pred::StatusEq(status) => {
      match status {
        | Status::Pending => {
          task.status == Status::Pending
            && !task.is_waiting(now)
        }
        | _ => &task.status == status
      }
    }
    | Pred::Waiting => {
      task.is_waiting(now)
    }
    | Pred::DueBefore(dt) => {
      task
        .due
        .map(|due| due < *dt)
        .unwrap_or(false)
    }
    | Pred::DueAfter(dt) => {
      task
        .due
        .map(|due| due > *dt)
        .unwrap_or(false)
    }
    | Pred::TextContains(text) => {
      task
        .description
        .to_ascii_lowercase()
        .contains(
          &text.to_ascii_lowercase()
        )
    }
  };

  trace!(pred = ?pred, id = ?task.id, uuid = %task.uuid, ok, "filter predicate evaluation");
  ok
}

fn eval_virtual_tag(
  virtual_tag: VirtualTag,
  task: &Task,
  now: DateTime<Utc>
) -> bool {
  let now_local_date =
    to_project_date(now);

  match virtual_tag {
    | VirtualTag::Pending => {
      task.status == Status::Pending
        && !task.is_waiting(now)
    }
    | VirtualTag::Waiting => {
      task.is_waiting(now)
    }
    | VirtualTag::Completed => {
      task.status == Status::Completed
    }
    | VirtualTag::Deleted => {
      task.status == Status::Deleted
    }
    | VirtualTag::Active => {
      task.status == Status::Pending
        && !task.is_waiting(now)
        && task.start.is_some()
    }
    | VirtualTag::Ready => {
      task.status == Status::Pending
        && !task.is_waiting(now)
        && task.depends.is_empty()
    }
    | VirtualTag::Blocked => {
      !task.depends.is_empty()
    }
    | VirtualTag::Unblocked => {
      task.depends.is_empty()
    }
    | VirtualTag::Due => {
      task
        .due
        .map(|due| {
          to_project_date(due)
            <= now_local_date
        })
        .unwrap_or(false)
    }
    | VirtualTag::Overdue => {
      task
        .due
        .map(|due| due < now)
        .unwrap_or(false)
    }
    | VirtualTag::Today => {
      task
        .due
        .map(|due| {
          to_project_date(due)
            == now_local_date
        })
        .unwrap_or(false)
    }
    | VirtualTag::Tomorrow => {
      let tomorrow = now_local_date
        .checked_add_days(Days::new(1))
        .unwrap_or(now_local_date);
      task
        .due
        .map(|due| {
          to_project_date(due)
            == tomorrow
        })
        .unwrap_or(false)
    }
  }
}

fn parse_virtual_tag(
  tag: &str
) -> Option<VirtualTag> {
  match tag {
    | "PENDING" => {
      Some(VirtualTag::Pending)
    }
    | "WAITING" => {
      Some(VirtualTag::Waiting)
    }
    | "COMPLETED" => {
      Some(VirtualTag::Completed)
    }
    | "DELETED" => {
      Some(VirtualTag::Deleted)
    }
    | "ACTIVE" => {
      Some(VirtualTag::Active)
    }
    | "READY" => {
      Some(VirtualTag::Ready)
    }
    | "BLOCKED" => {
      Some(VirtualTag::Blocked)
    }
    | "UNBLOCKED" => {
      Some(VirtualTag::Unblocked)
    }
    | "DUE" => Some(VirtualTag::Due),
    | "OVERDUE" => {
      Some(VirtualTag::Overdue)
    }
    | "TODAY" => {
      Some(VirtualTag::Today)
    }
    | "TOMORROW" => {
      Some(VirtualTag::Tomorrow)
    }
    | _ => None
  }
}

fn expr_has_explicit_status_filter(
  expr: &Expr
) -> bool {
  match expr {
    | Expr::True => false,
    | Expr::Pred(pred) => {
      matches!(
        pred,
        Pred::StatusEq(_)
          | Pred::Waiting
          | Pred::VirtualTagInclude(_)
          | Pred::VirtualTagExclude(_)
      )
    }
    | Expr::And(nodes)
    | Expr::Or(nodes) => {
      nodes.iter().any(
        expr_has_explicit_status_filter
      )
    }
  }
}

fn expr_has_identity_selector(
  expr: &Expr
) -> bool {
  match expr {
    | Expr::True => false,
    | Expr::Pred(pred) => {
      matches!(
        pred,
        Pred::Id(_) | Pred::Uuid(_)
      )
    }
    | Expr::And(nodes)
    | Expr::Or(nodes) => {
      nodes
        .iter()
        .any(expr_has_identity_selector)
    }
  }
}

#[cfg(test)]
mod tests {
  use chrono::{
    Duration,
    TimeZone,
    Utc
  };

  use super::Filter;
  use crate::task::Task;

  #[test]
  fn boolean_precedence_and_parentheses()
   {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 16, 5, 0, 0
      )
      .unwrap();
    let mut x = Task::new_pending(
      "x".to_string(),
      now,
      1
    );
    x.tags = vec!["x".to_string()];

    let mut y = Task::new_pending(
      "y".to_string(),
      now,
      2
    );
    y.tags = vec!["y".to_string()];

    let mut xy = Task::new_pending(
      "xy".to_string(),
      now,
      3
    );
    xy.tags = vec![
      "x".to_string(),
      "y".to_string(),
    ];

    let filter = Filter::parse(
      &[
        "(".to_string(),
        "+x".to_string(),
        "or".to_string(),
        "+y".to_string(),
        ")".to_string(),
        "and".to_string(),
        "+y".to_string()
      ],
      now
    )
    .unwrap();

    assert!(!filter.matches(&x, now));
    assert!(filter.matches(&y, now));
    assert!(filter.matches(&xy, now));
  }

  #[test]
  fn virtual_tags_pending_waiting_and_active()
   {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 16, 5, 0, 0
      )
      .unwrap();
    let mut active = Task::new_pending(
      "active".to_string(),
      now,
      1
    );
    active.start = Some(now);

    let mut waiting = Task::new_pending(
      "waiting".to_string(),
      now,
      2
    );
    waiting.wait =
      Some(now + Duration::hours(2));

    let pending_filter = Filter::parse(
      &["+PENDING".to_string()],
      now
    )
    .unwrap();
    let waiting_filter = Filter::parse(
      &["+WAITING".to_string()],
      now
    )
    .unwrap();
    let active_filter = Filter::parse(
      &["+ACTIVE".to_string()],
      now
    )
    .unwrap();

    assert!(
      pending_filter
        .matches(&active, now)
    );
    assert!(
      !pending_filter
        .matches(&waiting, now)
    );

    assert!(
      !waiting_filter
        .matches(&active, now)
    );
    assert!(
      waiting_filter
        .matches(&waiting, now)
    );

    assert!(
      active_filter
        .matches(&active, now)
    );
    assert!(
      !active_filter
        .matches(&waiting, now)
    );
  }

  #[test]
  fn id_selector_matches_waiting_task()
  {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 16, 5, 0, 0
      )
      .unwrap();
    let mut waiting = Task::new_pending(
      "waiting".to_string(),
      now,
      2
    );
    waiting.wait =
      Some(now + Duration::hours(2));

    let filter = Filter::parse(
      &["2".to_string()],
      now
    )
    .unwrap();
    assert!(
      filter.has_identity_selector()
    );
    assert!(
      filter.matches(&waiting, now)
    );
  }

  #[test]
  fn raw_matching_can_include_waiting()
  {
    let now = Utc
      .with_ymd_and_hms(
        2026, 2, 16, 5, 0, 0
      )
      .unwrap();
    let mut waiting = Task::new_pending(
      "waiting".to_string(),
      now,
      1
    );
    waiting.wait =
      Some(now + Duration::hours(2));

    let filter = Filter::default();
    assert!(
      !filter.matches(&waiting, now)
    );
    assert!(
      filter
        .matches_without_waiting_guard(
          &waiting, now
        )
    );
  }
}
