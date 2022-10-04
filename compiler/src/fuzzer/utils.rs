use crate::{
    compiler::hir::{self, Expression, HirDb, Lambda},
    database::Database,
    vm::tracer::{FullTracer, Tracer},
};

pub fn did_need_in_closure_cause_panic(db: &Database, closure_id: &hir::Id) -> bool {
    todo!();
    // let entry = if let Some(entry) = tracer.events.last() {
    //     entry
    // } else {
    //     // The only way there's no trace log before the panic is when there's an
    //     // error from an earlier compilation stage that got lowered into the
    //     // LIR. That's also definitely the fault of the function.
    //     return false;
    // };
    // if let Event::InFiber {
    //     event: InFiberEvent::NeedsStarted { id, .. },
    //     ..
    // } = &entry.data
    // {
    //     let mut id = id.parent().unwrap();
    //     loop {
    //         if &id == closure_id {
    //             return true;
    //         }

    //         match db
    //             .find_expression(id.clone())
    //             .expect("Parent of a `needs` call is a parameter.")
    //         {
    //             Expression::Lambda(Lambda { fuzzable, .. }) => {
    //                 if fuzzable {
    //                     return false; // The needs is in a different fuzzable lambda.
    //                 }
    //             }
    //             _ => panic!("Only lambdas can be the parent of a `needs` call."),
    //         };

    //         match id.parent() {
    //             Some(parent_id) => id = parent_id,
    //             None => return false,
    //         }
    //     }
    // }
    false
}
