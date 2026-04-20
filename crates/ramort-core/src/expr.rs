use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet};
use std::fmt;
use std::str::FromStr;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum ParseLinExprError {
    #[error("empty term")]
    EmptyTerm,
    #[error("unsupported term: {0}")]
    UnsupportedTerm(String),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct LinExpr {
    pub constant: i64,
    pub terms: BTreeMap<String, i64>,
}

impl LinExpr {
    pub fn zero() -> Self {
        Self::default()
    }
    pub fn one() -> Self {
        Self::constant(1)
    }
    pub fn constant(c: i64) -> Self {
        Self {
            constant: c,
            terms: BTreeMap::new(),
        }
    }

    pub fn var(name: impl Into<String>, coeff: i64) -> Self {
        let mut terms = BTreeMap::new();
        if coeff != 0 {
            terms.insert(name.into(), coeff);
        }
        Self { constant: 0, terms }
    }

    pub fn add(&self, rhs: &Self) -> Self {
        let mut out = self.clone();
        out.constant += rhs.constant;
        for (k, v) in &rhs.terms {
            *out.terms.entry(k.clone()).or_insert(0) += *v;
        }
        out.normalize()
    }

    pub fn sub(&self, rhs: &Self) -> Self {
        self.add(&rhs.scale(-1))
    }

    pub fn scale(&self, k: i64) -> Self {
        let mut out = LinExpr::constant(self.constant * k);
        for (n, c) in &self.terms {
            let v = c * k;
            if v != 0 {
                out.terms.insert(n.clone(), v);
            }
        }
        out
    }

    pub fn normalize(mut self) -> Self {
        self.terms.retain(|_, v| *v != 0);
        self
    }

    pub fn coeff(&self, name: &str) -> i64 {
        *self.terms.get(name).unwrap_or(&0)
    }

    pub fn vars(&self) -> BTreeSet<String> {
        self.terms.keys().cloned().collect()
    }

    pub fn substitute(&self, values: &BTreeMap<String, i64>) -> Self {
        let mut out = LinExpr::constant(self.constant);
        for (n, c) in &self.terms {
            if let Some(v) = values.get(n) {
                out.constant += c * v;
            } else if let Some((lhs, rhs)) = n.split_once('*') {
                match (values.get(lhs), values.get(rhs)) {
                    (Some(l), Some(r)) => out.constant += c * l * r,
                    (Some(l), None) => out = out.add(&LinExpr::var(rhs, c * l)),
                    (None, Some(r)) => out = out.add(&LinExpr::var(lhs, c * r)),
                    (None, None) => out = out.add(&LinExpr::var(n, *c)),
                }
            } else {
                out = out.add(&LinExpr::var(n, *c));
            }
        }
        out.normalize()
    }

    pub fn rename_vars<F: FnMut(&str) -> String>(&self, mut f: F) -> Self {
        let mut out = LinExpr::constant(self.constant);
        for (n, c) in &self.terms {
            out = out.add(&LinExpr::var(f(n), *c));
        }
        out.normalize()
    }

    /// Exact sufficient check for `self <= rhs` under all remaining variables >= 0.
    pub fn leq_under_nonnegative_vars(&self, rhs: &Self) -> ExactCheck {
        let slack = rhs.sub(self).normalize();
        let proven = slack.constant >= 0 && slack.terms.values().all(|c| *c >= 0);
        ExactCheck { proven, slack }
    }

    pub fn to_big_o(&self) -> String {
        let vars: Vec<_> = self
            .terms
            .keys()
            .filter(|k| !k.starts_with("a_") && !k.starts_with("c_"))
            .cloned()
            .collect();
        if vars.is_empty() {
            "O(1)".to_string()
        } else {
            format!("O({})", vars.join(" + "))
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExactCheck {
    pub proven: bool,
    pub slack: LinExpr,
}

impl FromStr for LinExpr {
    type Err = ParseLinExprError;
    fn from_str(src: &str) -> Result<Self, Self::Err> {
        parse_lin_expr(src)
    }
}

pub fn parse_lin_expr(src: &str) -> Result<LinExpr, ParseLinExprError> {
    let mut s = src.trim().replace(' ', "");
    if s.is_empty() {
        return Ok(LinExpr::zero());
    }
    if s == "O(1)" {
        return Ok(LinExpr::one());
    }
    if s.starts_with("O(") && s.ends_with(')') {
        s = s[2..s.len() - 1].to_string();
    }

    let mut out = LinExpr::zero();
    let mut buf = String::new();
    let mut sign = 1;

    fn flush(buf: &mut String, sign: i64, out: &mut LinExpr) -> Result<(), ParseLinExprError> {
        if buf.is_empty() {
            return Ok(());
        }
        let tok = std::mem::take(buf);
        let term = parse_term(&tok, sign)?;
        *out = out.add(&term);
        Ok(())
    }

    for ch in s.chars() {
        match ch {
            '+' => {
                flush(&mut buf, sign, &mut out)?;
                sign = 1;
            }
            '-' => {
                flush(&mut buf, sign, &mut out)?;
                sign = -1;
            }
            _ => buf.push(ch),
        }
    }
    flush(&mut buf, sign, &mut out)?;
    Ok(out.normalize())
}

fn parse_term(tok: &str, sign: i64) -> Result<LinExpr, ParseLinExprError> {
    if tok.is_empty() {
        return Err(ParseLinExprError::EmptyTerm);
    }
    if let Ok(n) = tok.parse::<i64>() {
        return Ok(LinExpr::constant(sign * n));
    }
    if let Some((a, b)) = tok.split_once('*') {
        if let Ok(n) = a.parse::<i64>() {
            return Ok(LinExpr::var(b, sign * n));
        }
        if let Ok(n) = b.parse::<i64>() {
            return Ok(LinExpr::var(a, sign * n));
        }
        return Err(ParseLinExprError::UnsupportedTerm(tok.to_string()));
    }
    Ok(LinExpr::var(tok, sign))
}

impl fmt::Display for LinExpr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut parts = Vec::new();
        if self.constant != 0 || self.terms.is_empty() {
            parts.push(self.constant.to_string());
        }
        for (n, c) in &self.terms {
            parts.push(match *c {
                1 => n.clone(),
                -1 => format!("-{n}"),
                k => format!("{k}*{n}"),
            });
        }
        write!(f, "{}", parts.join(" + ").replace("+ -", "- "))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn substitute_expands_encoded_product_terms() {
        let expr = LinExpr::constant(1)
            .add(&LinExpr::var("a_self_back*B", 2))
            .add(&LinExpr::var("a_self_back", 3));
        let values = BTreeMap::from([("a_self_back".to_string(), 4)]);

        assert_eq!(
            expr.substitute(&values),
            LinExpr::constant(13).add(&LinExpr::var("B", 8))
        );
    }
}
