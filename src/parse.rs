use std::{cmp::Ordering, sync::Arc};

use pest::{
    error::LineColLocation,
    iterators::{Pair, Pairs},
    Parser,
};
use pest_derive::Parser;

use crate::base::{Capture, Command, Context, Expression, Process};

#[derive(Clone, Debug)]
pub struct Name {
    string: String,
    location: Location,
}

#[derive(Clone, Debug)]
pub struct ParseError {
    message: String,
    location: Location,
}

#[derive(Clone, Debug)]
pub struct Location {
    line: usize,
    column: usize,
}

#[derive(Parser)]
#[grammar = "par.pest"]
pub struct Par;

pub fn parse_program<X>(source: &str) -> Result<Context<Arc<Name>, X>, ParseError> {
    let mut context = Context::empty();
    for pair in Par::parse(Rule::program, source)?
        .next()
        .unwrap()
        .into_inner()
    {
        if pair.as_rule() == Rule::EOI {
            continue;
        }
        let mut pairs = pair.into_inner();
        let mut free = Vec::new();
        let name = parse_name(&mut pairs)?;
        let expr = parse_expression(&mut pairs, &mut free)?;
        if let Some(_) = context.statics.insert(name.clone(), expr) {
            return Err(ParseError {
                message: format!("\"{}\" is already defined", name.string),
                location: name.location.clone(),
            });
        }
    }
    Ok(context)
}

fn parse_name(pairs: &mut Pairs<'_, Rule>) -> Result<Arc<Name>, ParseError> {
    let pair = pairs.next().unwrap();
    Ok(Arc::new(pair.into()))
}

fn parse_expression(
    pairs: &mut Pairs<'_, Rule>,
    free: &mut Vec<Arc<Name>>,
) -> Result<Arc<Expression<Arc<Name>>>, ParseError> {
    let pair = pairs.next().unwrap().into_inner().next().unwrap();
    match pair.as_rule() {
        Rule::fork => {
            let mut pairs = pair.into_inner();
            let object = parse_name(&mut pairs)?;
            let process = parse_process(&mut pairs, free)?;
            if let Some(index) = free.iter().position(|v| v == &object) {
                free.swap_remove(index);
            }
            Ok(Arc::new(Expression::Fork(
                Capture {
                    variables: free.clone(),
                },
                object,
                process,
            )))
        }
        Rule::reference => {
            let name = Arc::new(pair.into());
            free.push(Arc::clone(&name));
            Ok(Arc::new(Expression::Ref(name)))
        }
        _ => unreachable!(),
    }
}

fn parse_process(
    pairs: &mut Pairs<'_, Rule>,
    free: &mut Vec<Arc<Name>>,
) -> Result<Arc<Process<Arc<Name>>>, ParseError> {
    let pair = pairs.next().unwrap().into_inner().next().unwrap();
    let rule = pair.as_rule();
    let mut pairs = pair.into_inner();
    match rule {
        Rule::p_let => {
            let name = parse_name(&mut pairs)?;
            let expr = parse_expression(&mut pairs, free)?;
            let proc = parse_process(&mut pairs, free)?;
            if let Some(index) = free.iter().position(|v| v == &name) {
                free.swap_remove(index);
            }
            Ok(Arc::new(Process::Let(name, expr, proc)))
        }
        Rule::p_link => {
            let subject = parse_name(&mut pairs)?;
            let argument = parse_expression(&mut pairs, free)?;
            free.push(subject.clone());
            Ok(Arc::new(Process::Link(subject, argument)))
        }
        Rule::p_break => {
            let subject = parse_name(&mut pairs)?;
            free.push(subject.clone());
            Ok(Arc::new(Process::Do(subject, Command::Break)))
        }
        Rule::p_continue => {
            let subject = parse_name(&mut pairs)?;
            let then = parse_process(&mut pairs, free)?;
            free.push(subject.clone());
            Ok(Arc::new(Process::Do(subject, Command::Continue(then))))
        }
        Rule::p_send => {
            let subject = parse_name(&mut pairs)?;
            let argument = parse_expression(&mut pairs, free)?;
            let then = parse_process(&mut pairs, free)?;
            Ok(Arc::new(Process::Do(
                subject,
                Command::Send(argument, then),
            )))
        }
        Rule::p_receive => {
            let subject = parse_name(&mut pairs)?;
            let parameter = parse_name(&mut pairs)?;
            let then = parse_process(&mut pairs, free)?;
            if let Some(index) = free.iter().position(|v| v == &parameter) {
                free.swap_remove(index);
            }
            Ok(Arc::new(Process::Do(
                subject,
                Command::Receive(parameter, then),
            )))
        }
        Rule::p_select => {
            let subject = parse_name(&mut pairs)?;
            let branch = parse_name(&mut pairs)?;
            let then = parse_process(&mut pairs, free)?;
            Ok(Arc::new(Process::Do(
                subject,
                Command::Select(branch, then),
            )))
        }
        Rule::p_case => {
            let subject = parse_name(&mut pairs)?;
            let pair = pairs.next().unwrap();
            assert_eq!(pair.as_rule(), Rule::p_branches);
            let mut branches = Vec::new();
            for mut pairs in pair.into_inner().map(Pair::into_inner) {
                let branch = parse_name(&mut pairs)?;
                let process = parse_process(&mut pairs, free)?;
                branches.push((branch, process));
            }
            let otherwise = match pairs.next() {
                Some(pair) => Some(parse_process(&mut Pairs::single(pair), free)?),
                None => None,
            };
            Ok(Arc::new(Process::Do(
                subject,
                Command::Case(branches, otherwise),
            )))
        }
        _ => unreachable!(),
    }
}

impl From<Pair<'_, Rule>> for Location {
    fn from(value: Pair<'_, Rule>) -> Self {
        let (line, column) = value.line_col();
        Self { line, column }
    }
}

impl From<Pair<'_, Rule>> for Name {
    fn from(value: Pair<'_, Rule>) -> Self {
        Self {
            string: value.as_str().to_string(),
            location: value.into(),
        }
    }
}

impl From<pest::error::Error<Rule>> for ParseError {
    fn from(value: pest::error::Error<Rule>) -> Self {
        Self {
            message: value.to_string(),
            location: match value.line_col {
                LineColLocation::Pos((line, column)) => Location { line, column },
                LineColLocation::Span((line, column), _) => Location { line, column },
            },
        }
    }
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.string)
    }
}

impl PartialOrd for Name {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

impl Ord for Name {
    fn cmp(&self, other: &Self) -> Ordering {
        self.string.cmp(&other.string)
    }
}

impl PartialEq for Name {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other) == Ordering::Equal
    }
}

impl Eq for Name {}
