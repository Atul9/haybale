use inkwell::basic_block::BasicBlock;
use inkwell::values::*;
use log::debug;
use std::collections::HashMap;
use std::fmt;
use z3::ast::{BV, Bool};

use crate::utils::*;

type VarMap<'ctx> = HashMap<AnyValueEnum, BVorBool<'ctx>>;

// Our VarMap stores both BVs and Bools
#[derive(Clone, PartialEq, Eq)]
enum BVorBool<'ctx> {
    BV(BV<'ctx>),
    Bool(Bool<'ctx>),
}

impl<'ctx> fmt::Debug for BVorBool<'ctx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            BVorBool::BV(bv) => write!(f, "BV( {} )", bv),
            BVorBool::Bool(b) => write!(f, "Bool( {} )", b),
        }
    }
}

impl<'ctx> From<BV<'ctx>> for BVorBool<'ctx> {
    fn from(bv: BV<'ctx>) -> BVorBool<'ctx> {
        BVorBool::BV(bv)
    }
}

impl<'ctx> From<Bool<'ctx>> for BVorBool<'ctx> {
    fn from(b: Bool<'ctx>) -> BVorBool<'ctx> {
        BVorBool::Bool(b)
    }
}

impl<'ctx> From<BVorBool<'ctx>> for BV<'ctx> {
    fn from(b: BVorBool<'ctx>) -> BV<'ctx> {
        match b {
            BVorBool::BV(bv) => bv,
            _ => panic!("Can't convert {:?} to BV", b),
        }
    }
}

impl<'ctx> From<BVorBool<'ctx>> for Bool<'ctx> {
    fn from(b: BVorBool<'ctx>) -> Bool<'ctx> {
        match b {
            BVorBool::Bool(b) => b,
            _ => panic!("Can't convert {:?} to Bool", b),
        }
    }
}

// these are basically From impls, but for converting ref to ref
impl<'ctx> BVorBool<'ctx> {
    fn as_bv(&self) -> &BV<'ctx> {
        match self {
            BVorBool::BV(bv) => &bv,
            _ => panic!("Can't convert {:?} to BV", self),
        }
    }

    fn as_bool(&self) -> &Bool<'ctx> {
        match self {
            BVorBool::Bool(b) => &b,
            _ => panic!("Can't convert {:?} to Bool", self),
        }
    }
}

pub struct State<'ctx> {
    pub ctx: &'ctx z3::Context,
    solver: z3::Solver<'ctx>,
    vars: VarMap<'ctx>,
    backtrack_points: Vec<BacktrackPoint<'ctx>>,
}

struct BacktrackPoint<'ctx> {
  // BasicBlock to resume execution at
  // We use owned BasicBlocks because copy should be cheap (I'm not sure why it's not a Copy type in inkwell)
  next_bb: BasicBlock,
  // BasicBlock executed just prior to the BacktrackPoint
  prev_bb: BasicBlock,
  // Constraint to add before restarting execution at next_bb
  // (intended use of this is to constrain the branch in that direction)
  // We use owned Bools because:
  //   a) it seems necessary to not use refs, and
  //   b) it seems reasonable for callers to give us ownership of these Bools.
  //       If/when that becomes not reasonable, we should probably use boxed
  //       Bools here rather than making callers copy.
  constraint: Bool<'ctx>,
}

impl<'ctx> BacktrackPoint<'ctx> {
    fn new(next_bb: BasicBlock, prev_bb: BasicBlock, constraint: Bool<'ctx>) -> Self {
        BacktrackPoint{
            next_bb,
            prev_bb,
            constraint,
        }
    }
}

impl<'ctx> fmt::Display for BacktrackPoint<'ctx> {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "<BacktrackPoint to execute bb {} with constraint {}>",
            get_bb_name(self.next_bb), self.constraint)
    }
}

impl<'ctx> State<'ctx> {
    pub fn new(ctx: &'ctx z3::Context) -> Self {
        State {
            ctx,
            solver: z3::Solver::new(ctx),
            vars: HashMap::new(),
            backtrack_points: Vec::new(),
        }
    }

    pub fn assert(&self, cond: &Bool<'ctx>) {
        debug!("asserting {}", cond);
        self.solver.assert(cond);
    }

    pub fn check(&self) -> bool {
        debug!("Solving with constraints:\n{}", self.solver);
        self.solver.check()
    }

    pub fn check_with_extra_constraints(&self, conds: &[&Bool<'ctx>]) -> bool {
        self.solver.push();
        for cond in conds {
          self.solver.assert(cond);
        }
        let retval = self.solver.check();
        self.solver.pop(1);
        retval
    }

    pub fn get_model(&self) -> z3::Model<'ctx> {
        let model = self.solver.get_model();
        debug!("Returned model:\n{}", model);
        model
    }

    // Associate the given value with the given BV
    pub fn add_bv_var(&mut self, v: impl AnyValue + Copy, bv: BV<'ctx>) {
        debug!("Adding var {} = {}", get_value_name(v), bv);
        self.vars.insert(v.as_any_value_enum(), bv.into());
    }

    // Associate the given value with the given Bool
    pub fn add_bool_var(&mut self, v: impl AnyValue + Copy, b: Bool<'ctx>) {
        debug!("Adding var {} = {}", get_value_name(v), b);
        self.vars.insert(v.as_any_value_enum(), b.into());
    }

    // Look up the BV previously created for the given value
    pub fn lookup_bv_var(&self, v: impl AnyValue + Copy) -> &BV<'ctx> {
        debug!("Looking up var {}", get_value_name(v));
        self.vars.get(&v.as_any_value_enum()).unwrap_or_else(|| {
            let keys: Vec<&AnyValueEnum> = self.vars.keys().collect();
            panic!("Failed to find value {:?} in map with keys {:?}", v, keys);
        }).as_bv()
    }

    // Look up the Bool previously created for the given value
    pub fn lookup_bool_var(&self, v: impl AnyValue + Copy) -> &Bool<'ctx> {
        debug!("Looking up var {}", get_value_name(v));
        self.vars.get(&v.as_any_value_enum()).unwrap_or_else(|| {
            let keys: Vec<&AnyValueEnum> = self.vars.keys().collect();
            panic!("Failed to find value {:?} in map with keys {:?}", v, keys);
        }).as_bool()
    }

    // Convert a Value to the appropriate BV
    // Should be an operand, that is, an RHS value
    // (that way, we know it's either a constant or a variable we previously added to the state)
    pub fn operand_to_bv(&self, v: impl BasicValue + Copy) -> BV<'ctx> {
        match v.as_basic_value_enum() {
            BasicValueEnum::IntValue(iv) => {
                if iv.is_const() {
                    BV::from_u64(self.ctx, iv.get_zero_extended_constant().unwrap(), iv.get_type().get_bit_width())
                } else {
                    self.lookup_bv_var(v).clone()
                }
            },
            v => unimplemented!("operand_to_bv() for {:?}", v)
        }
    }

    // Convert an IntValue to the appropriate Bool
    // Should be an operand, that is, an RHS value
    // (that way, we know it's either a constant or a variable we previously added to the state)
    // This will panic if the Value isn't an LLVM i1 type
    pub fn operand_to_bool(&self, v: IntValue) -> Bool<'ctx> {
        assert_eq!(v.get_type().get_bit_width(), 1);
        if v.is_const() {
            Bool::from_bool(self.ctx, v.get_zero_extended_constant().unwrap() != 0)
        } else {
            self.lookup_bool_var(v).clone()
        }
    }

    // again, we require owned BasicBlocks because copy should be cheap.  Caller can clone if necessary.
    // The constraint will be added only if we end up backtracking to this point, and only then
    pub fn save_backtracking_point(&mut self, next_bb: BasicBlock, prev_bb: BasicBlock, constraint: Bool<'ctx>) {
        debug!("Saving a backtracking point, which would enter bb {:?} with constraint {}", get_bb_name(next_bb), constraint);
        self.solver.push();
        self.backtrack_points.push(BacktrackPoint::new(next_bb, prev_bb, constraint));
    }

    // returns the BasicBlock where execution should continue and the BasicBlock executed before that
    // or None if there are no saved backtracking points left
    pub fn revert_to_backtracking_point(&mut self) -> Option<(BasicBlock, BasicBlock)> {
        if let Some(bp) = self.backtrack_points.pop() {
            debug!("Reverting to backtracking point {}", bp);
            self.solver.pop(1);
            debug!("Constraints are now:\n{}", self.solver);
            self.assert(&bp.constraint);
            Some((bp.next_bb, bp.prev_bb))
            // thanks to SSA, we don't need to roll back the VarMap; we'll just overwrite existing entries as needed.
            // Code on the backtracking path will never reference variables which we assigned on the original path.
            // This will become not true when we get to loops, but we don't support loops yet anyway
        } else {
            None
        }
    }

    // in lieu of an actual Display or Debug for State (for now)
    pub fn prettyprint_constraints(&self) {
        println!("{}", self.solver);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sat() {
        let ctx = z3::Context::new(&z3::Config::new());
        let state = State::new(&ctx);

        // empty state should be sat
        assert!(state.check());

        // adding True constraint should still be sat
        state.assert(&Bool::from_bool(&ctx, true));
        assert!(state.check());

        // adding x > 0 constraint should still be sat
        let x = ctx.named_bitvector_const("x", 64);
        state.assert(&x.bvsgt(&BV::from_i64(&ctx, 0, 64)));
        assert!(state.check());
    }

    #[test]
    fn unsat() {
        let ctx = z3::Context::new(&z3::Config::new());
        let state = State::new(&ctx);

        // adding False constraint should be unsat
        state.assert(&Bool::from_bool(&ctx, false));
        assert!(!state.check());
    }

    #[test]
    fn unsat_with_extra_constraints() {
        let ctx = z3::Context::new(&z3::Config::new());
        let state = State::new(&ctx);

        // adding x > 3 constraint should still be sat
        let x = ctx.named_bitvector_const("x", 64);
        state.assert(&x.bvugt(&BV::from_u64(&ctx, 3, 64)));
        assert!(state.check());

        // adding x < 3 constraint should make us unsat
        let bad_constraint = x.bvult(&BV::from_u64(&ctx, 3, 64));
        assert!(!state.check_with_extra_constraints(&[&bad_constraint]));

        // the state itself should still be sat, extra constraints weren't permanently added
        assert!(state.check());
    }

    #[test]
    fn get_model() {
        let ctx = z3::Context::new(&z3::Config::new());
        let state = State::new(&ctx);

        // add x > 3 constraint
        let x = ctx.named_bitvector_const("x", 64);
        state.assert(&x.bvugt(&BV::from_u64(&ctx, 3, 64)));

        // get model
        assert!(state.check());
        let model = state.get_model();

        // check that the computed value of x is > 3
        let x_value = model.eval(&x).unwrap().as_u64().unwrap();
        assert!(x_value > 3);
    }

    #[test]
    fn lookup_vars() {
        let ctx = z3::Context::new(&z3::Config::new());
        let mut state = State::new(&ctx);

        // create Inkwell values
        // We need non-constant values, which seem to be
        // surprisingly hard to create. We use function parameters.
        // First create the function type itself: (i64, bool) -> i64
        let valty = inkwell::types::IntType::i64_type();
        let boolty = inkwell::types::IntType::bool_type();
        use inkwell::types::BasicType;
        let functy = valty.fn_type(&[valty.as_basic_type_enum(), boolty.as_basic_type_enum()], false);
        // Then create a function of that type
        let inkwellmod = inkwell::module::Module::create("test_mod");
        let func = inkwellmod.add_function("test_func", functy, None);
        // Finally, get the parameters of that function
        let inkwellval = func.get_nth_param(0).unwrap().into_int_value();
        let inkwellboolval = func.get_nth_param(1).unwrap().into_int_value();

        // create Z3 values
        let x = ctx.named_bitvector_const("x", 64);
        let boolvar = ctx.named_bool_const("bool");

        // associate Inkwell values with Z3 values
        state.add_bv_var(inkwellval, x.clone());  // these clone()s wouldn't normally be necessary but we want to compare against the original values later
        state.add_bool_var(inkwellboolval, boolvar.clone());

        // check that looking up the Inkwell values gives the correct Z3 ones
        assert_eq!(state.lookup_bv_var(inkwellval), &x);
        assert_eq!(state.lookup_bool_var(inkwellboolval), &boolvar);

        // a different way of looking up
        assert_eq!(state.operand_to_bv(inkwellval), x);
        assert_eq!(state.operand_to_bool(inkwellboolval), boolvar);
    }

    #[test]
    fn const_bv() {
        let ctx = z3::Context::new(&z3::Config::new());
        let state = State::new(&ctx);

        // create an Inkwell value which is constant 3
        let constint = inkwell::types::IntType::i64_type().const_int(3, false);

        // this should create a corresponding Z3 value which is also constant 3
        let bv = state.operand_to_bv(constint);

        // check that the Z3 value was evaluated to 3
        assert!(state.check());
        assert_eq!(state.get_model().eval(&bv).unwrap().as_i64().unwrap(), 3);
    }

    #[test]
    fn const_bool() {
        let ctx = z3::Context::new(&z3::Config::new());
        let state = State::new(&ctx);

        // create Inkwell constants true and false
        let booltype = inkwell::types::IntType::bool_type();
        let consttrue = booltype.const_int(1, false);
        let constfalse = booltype.const_int(0, false);

        // this should create Z3 values true and false
        let bvtrue = state.operand_to_bool(consttrue);
        let bvfalse = state.operand_to_bool(constfalse);

        // assert the first one, which should be true, so we should still be sat
        state.assert(&bvtrue);
        assert!(state.check());

        // assert the second one, which should be false, so we should be unsat
        state.assert(&bvfalse);
        assert!(!state.check());
    }

    #[test]
    fn backtracking() {
        let ctx = z3::Context::new(&z3::Config::new());
        let mut state = State::new(&ctx);

        // assert x > 11
        let x = ctx.named_bitvector_const("x", 64);
        state.assert(&x.bvsgt(&BV::from_i64(&ctx, 11, 64)));

        // create some Inkwell BasicBlocks
        let inkwellmod = inkwell::module::Module::create("test_mod");
        let functy = inkwell::types::IntType::i64_type().fn_type(&[], false);
        let func = inkwellmod.add_function("test_func", functy, None);
        let bb1 = func.append_basic_block("bb1");
        let bb2 = func.append_basic_block("bb2");

        // create a backtrack point with constraint y > 5
        let y = ctx.named_bitvector_const("y", 64);
        let constraint = y.bvsgt(&BV::from_i64(&ctx, 5, 64));
        state.save_backtracking_point(bb2, bb1, constraint);

        // check that the constraint y > 5 wasn't added: adding y < 4 should keep us sat
        assert!(state.check_with_extra_constraints(&[&y.bvslt(&BV::from_i64(&ctx, 4, 64))]));

        // assert x < 8 to make us unsat
        state.assert(&x.bvslt(&BV::from_i64(&ctx, 8, 64)));
        assert!(!state.check());

        // roll back to backtrack point; check that we got the right bbs
        let (bb_a, bb_b) = state.revert_to_backtracking_point().unwrap();
        assert_eq!(bb_a, bb2);
        assert_eq!(bb_b, bb1);

        // check that the constraint x < 8 was removed: we're sat again
        assert!(state.check());

        // check that the constraint y > 5 was added: y evaluates to something > 5
        assert!(state.get_model().eval(&y).unwrap().as_i64().unwrap() > 5);

        // check that the first constraint remained in place: x > 11
        assert!(state.get_model().eval(&x).unwrap().as_u64().unwrap() > 11);

        // check that trying to backtrack again returns None
        assert_eq!(state.revert_to_backtracking_point(), None);
    }
}
