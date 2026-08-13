//! Resolving a world's dates together.
//!
//! Anchored dates form a dependency graph. Resolution walks it depth-first so a date
//! is only computed once everything it hangs off has been, and reports a cycle as the
//! full ring rather than a bare "cycle detected" — the writer needs to know *which*
//! events to break apart.

use std::collections::{BTreeMap, HashMap};

use crate::calendar::Calendar;
use crate::date::{DateExpr, Fuzz, Resolved};
use crate::error::{Error, Result};
use crate::interval::FuzzyInterval;

/// Dates are keyed by node id. An event contributes one node (`evt_siege`); an entity
/// with an existence interval contributes two (`act_aldric.birth`, `act_aldric.death`).
pub type NodeMap = BTreeMap<String, DateExpr>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Mark {
    Unvisited,
    InProgress,
    Done,
}

pub struct Resolver<'a> {
    pub calendar: &'a Calendar,
    pub fuzz: Fuzz,
}

impl<'a> Resolver<'a> {
    pub fn new(calendar: &'a Calendar) -> Self {
        Self { calendar, fuzz: Fuzz::default() }
    }

    pub fn with_fuzz(mut self, fuzz: Fuzz) -> Self {
        self.fuzz = fuzz;
        self
    }

    /// Resolve a single date that carries no anchor.
    pub fn resolve_local(&self, expr: &DateExpr) -> Result<Resolved> {
        expr.resolve_local(self.calendar, self.fuzz)
    }

    /// Resolve every date in the world at once.
    pub fn resolve_all(&self, nodes: &NodeMap) -> Result<BTreeMap<String, Resolved>> {
        let mut marks: HashMap<&str, Mark> =
            nodes.keys().map(|k| (k.as_str(), Mark::Unvisited)).collect();
        let mut out: BTreeMap<String, Resolved> = BTreeMap::new();

        for root in nodes.keys() {
            if marks[root.as_str()] != Mark::Unvisited {
                continue;
            }

            // (node, deps_already_pushed)
            let mut stack: Vec<(&str, bool)> = vec![(root.as_str(), false)];
            let mut path: Vec<&str> = Vec::new();

            while let Some((id, ready)) = stack.pop() {
                if ready {
                    path.pop();
                    let resolved = self.resolve_node(id, &nodes[id], &out)?;
                    out.insert(id.to_string(), resolved);
                    marks.insert(id, Mark::Done);
                    continue;
                }

                if marks[id] == Mark::Done {
                    continue;
                }

                marks.insert(id, Mark::InProgress);
                path.push(id);
                stack.push((id, true));

                let Some(dep) = nodes[id].depends_on() else { continue };
                let Some((dep_key, _)) = nodes.get_key_value(dep) else {
                    return Err(Error::DanglingAnchor {
                        from: id.to_string(),
                        missing: dep.to_string(),
                    });
                };

                match marks[dep_key.as_str()] {
                    Mark::InProgress => return Err(Error::AnchorCycle { path: ring(&path, dep) }),
                    Mark::Unvisited => stack.push((dep_key.as_str(), false)),
                    Mark::Done => {}
                }
            }
        }

        Ok(out)
    }

    /// Resolve a date against nodes that are already resolved.
    ///
    /// Fact endpoints use this: a fact may anchor to an event or an entity boundary,
    /// but nothing anchors to a fact, so they resolve in a second pass rather than
    /// joining the graph. `owner` only names the holder in error messages.
    pub fn resolve_in(
        &self,
        owner: &str,
        expr: &DateExpr,
        context: &BTreeMap<String, Resolved>,
    ) -> Result<Resolved> {
        self.resolve_node(owner, expr, context)
    }

    fn resolve_node(
        &self,
        id: &str,
        expr: &DateExpr,
        done: &BTreeMap<String, Resolved>,
    ) -> Result<Resolved> {
        match expr {
            DateExpr::Anchor { node, offset, approx } => {
                let base = done.get(node.as_str()).ok_or_else(|| Error::DanglingAnchor {
                    from: id.to_string(),
                    missing: node.clone(),
                })?;
                Ok(Resolved::anchored_from(base, self.calendar, offset, *approx, self.fuzz))
            }
            other => other.resolve_local(self.calendar, self.fuzz),
        }
    }

    /// Build the interval a fact is valid over from its two authored ends.
    pub fn interval(&self, from: &DateExpr, to: &DateExpr) -> Result<FuzzyInterval> {
        Ok(FuzzyInterval::new(&self.resolve_local(from)?, &self.resolve_local(to)?))
    }
}

/// The cycle, from where it closes back around to itself.
fn ring(path: &[&str], closes_at: &str) -> Vec<String> {
    let start = path.iter().position(|n| *n == closes_at).unwrap_or(0);
    path[start..]
        .iter()
        .map(|s| s.to_string())
        .chain(std::iter::once(closes_at.to_string()))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::calendar::CivilDate;
    use crate::parse::parse_date;

    fn nodes(pairs: &[(&str, &str)]) -> NodeMap {
        pairs.iter().map(|(id, expr)| (id.to_string(), parse_date(expr).unwrap())).collect()
    }

    #[test]
    fn resolves_a_chain_of_anchors() {
        let cal = Calendar::gregorian();
        let out = Resolver::new(&cal)
            .resolve_all(&nodes(&[
                ("evt_sundering", "0500-01-01"),
                ("evt_war", "@evt_sundering+40y"),
                ("evt_treaty", "@evt_war+10y"),
            ]))
            .unwrap();

        assert_eq!(cal.from_day(out["evt_treaty"].nominal.unwrap()), CivilDate::ymd(550, 1, 1));
    }

    /// The reason relative anchoring exists: re-timing history is one edit, not a rewrite.
    #[test]
    fn moving_the_anchor_moves_everything_pinned_to_it() {
        let cal = Calendar::gregorian();
        let resolver = Resolver::new(&cal);
        let chain = [("evt_war", "@evt_sundering+40y"), ("evt_treaty", "@evt_war+10y")];

        let year_of = |sundering: &str| {
            let mut n = nodes(&chain);
            n.insert("evt_sundering".into(), parse_date(sundering).unwrap());
            let out = resolver.resolve_all(&n).unwrap();
            cal.from_day(out["evt_treaty"].nominal.unwrap()).year
        };

        assert_eq!(year_of("0500-01-01"), 550);
        assert_eq!(year_of("0600-01-01"), 650);
        assert_eq!(year_of("0432-01-01"), 482);
    }

    #[test]
    fn vagueness_propagates_down_the_chain() {
        let cal = Calendar::gregorian();
        let out = Resolver::new(&cal)
            .resolve_all(&nodes(&[
                ("evt_sundering", "0500"), // a whole year
                ("evt_war", "@evt_sundering+40y"),
                ("evt_treaty", "@evt_war+10y~"), // and now approximate
            ]))
            .unwrap();

        // Shifting preserves the *civil* endpoints, not the day count: the window still
        // covers exactly one whole year, even though 540 is a leap year and 500 is not.
        assert_eq!(cal.from_day(out["evt_war"].earliest.unwrap()), CivilDate::ymd(540, 1, 1));
        assert_eq!(cal.from_day(out["evt_war"].latest.unwrap()), CivilDate::ymd(540, 12, 31));
        assert!(
            out["evt_treaty"].uncertainty_days().unwrap()
                > out["evt_war"].uncertainty_days().unwrap(),
            "`~` widens further"
        );
    }

    #[test]
    fn resolution_order_does_not_matter() {
        let cal = Calendar::gregorian();
        // Declared leaf-last, so the resolver must reorder rather than trust input order.
        let out = Resolver::new(&cal)
            .resolve_all(&nodes(&[
                ("a_treaty", "@b_war+10y"),
                ("b_war", "@c_sundering+40y"),
                ("c_sundering", "0500-01-01"),
            ]))
            .unwrap();
        assert_eq!(cal.from_day(out["a_treaty"].nominal.unwrap()).year, 550);
    }

    #[test]
    fn reports_the_full_cycle() {
        let cal = Calendar::gregorian();
        let err = Resolver::new(&cal)
            .resolve_all(&nodes(&[
                ("evt_a", "@evt_b+1y"),
                ("evt_b", "@evt_c+1y"),
                ("evt_c", "@evt_a+1y"),
            ]))
            .unwrap_err();

        let Error::AnchorCycle { path } = err else { panic!("expected a cycle, got {err:?}") };
        assert_eq!(path.first(), path.last(), "the ring must close");
        assert_eq!(path.len(), 4, "three events plus the closing repeat");
        for id in ["evt_a", "evt_b", "evt_c"] {
            assert!(path.contains(&id.to_string()), "{id} missing from {path:?}");
        }
    }

    #[test]
    fn catches_a_date_anchored_to_itself() {
        let cal = Calendar::gregorian();
        let err = Resolver::new(&cal).resolve_all(&nodes(&[("evt_a", "@evt_a+1y")])).unwrap_err();
        assert!(matches!(err, Error::AnchorCycle { .. }));
    }

    #[test]
    fn catches_anchors_to_nothing() {
        let cal = Calendar::gregorian();
        let err =
            Resolver::new(&cal).resolve_all(&nodes(&[("evt_a", "@evt_ghost+1y")])).unwrap_err();
        assert_eq!(
            err,
            Error::DanglingAnchor { from: "evt_a".into(), missing: "evt_ghost".into() }
        );
    }

    #[test]
    fn shared_ancestors_resolve_once_for_all_dependents() {
        let cal = Calendar::gregorian();
        let out = Resolver::new(&cal)
            .resolve_all(&nodes(&[
                ("evt_root", "0500-01-01"),
                ("evt_x", "@evt_root+10y"),
                ("evt_y", "@evt_root+20y"),
                ("evt_z", "@evt_y+5y"),
            ]))
            .unwrap();

        assert_eq!(out.len(), 4);
        assert_eq!(cal.from_day(out["evt_x"].nominal.unwrap()).year, 510);
        assert_eq!(cal.from_day(out["evt_z"].nominal.unwrap()).year, 525);
    }
}
