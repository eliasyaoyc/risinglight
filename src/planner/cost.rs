// Copyright 2023 RisingLight Project Authors. Licensed under Apache-2.0.

//! Cost functions to extract the best plan.

use egg::Language;
use tracing::debug;

use super::*;

/// The main cost function.
pub struct CostFn<'a> {
    pub egraph: &'a EGraph,
}

impl egg::CostFunction<Expr> for CostFn<'_> {
    type Cost = f32;
    fn cost<C>(&mut self, enode: &Expr, mut costs: C) -> Self::Cost
    where
        C: FnMut(Id) -> Self::Cost,
    {
        use Expr::*;
        let id = &self.egraph.lookup(enode.clone()).unwrap();
        let mut costs = |i: &Id| costs(*i);
        let rows = |i: &Id| self.egraph[*i].data.rows;
        let cols = |i: &Id| self.egraph[*i].data.schema.len() as f32;
        let nlogn = |x: f32| x * (x + 1.0).log2();
        // The cost of output chunks of a plan.
        let out = || rows(id) * cols(id);

        let c = match enode {
            Scan(_) | Values(_) => out(),
            Order([_, c]) => nlogn(rows(c)) + out() + costs(c),
            Proj([exprs, c]) | Filter([exprs, c]) => costs(exprs) * rows(c) + out() + costs(c),
            Agg([exprs, groupby, c]) => {
                (costs(exprs) + costs(groupby)) * rows(c) + out() + costs(c)
            }
            Limit([_, _, c]) => out() + costs(c),
            TopN([_, _, _, c]) => (rows(id) + 1.0).log2() * rows(c) + out() + costs(c),
            Join([_, on, l, r]) => costs(on) * rows(l) * rows(r) + out() + costs(l) + costs(r),
            HashJoin([_, _, _, l, r]) => {
                (rows(l) + 1.0).log2() * (rows(l) + rows(r)) + out() + costs(l) + costs(r)
            }
            Insert([_, _, c]) | CopyTo([_, c]) => rows(c) * cols(c) + costs(c),
            Empty(_) => 0.0,
            // for expressions, the cost is 0.1x AST size
            _ => enode.fold(0.1, |sum, id| sum + costs(&id)),
        };
        debug!(
            "{id}\t{enode:?}\tcost={c}, rows={}, cols={}",
            rows(id),
            cols(id)
        );
        c
    }
}
