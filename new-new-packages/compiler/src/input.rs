use std::{fs::read_to_string, path::PathBuf, sync::Arc};

use salsa::query_group;

#[query_group(InputDbStorage)]
pub trait InputDb: InputWatcher {
    fn get_input(&self, input: Input) -> Option<Arc<String>>;
    fn get_open_input(&self, input: Input) -> Option<Arc<String>>;
}

fn get_input(db: &dyn InputDb, input: Input) -> Option<Arc<String>> {
    if let Some(content) = db.get_open_input(input.clone()) {
        return Some(content);
    };

    match input {
        Input::File(path) => match read_to_string(&path) {
            Ok(content) => Some(Arc::new(content)),
            Err(error) if matches!(error.kind(), std::io::ErrorKind::NotFound) => None,
            _ => panic!("Unexpected error when reading file {:?}.", path),
        },
        Input::Untitled(_) => None,
    }
}
fn get_open_input(db: &dyn InputDb, input: Input) -> Option<Arc<String>> {
    // The following line of code shouldn't be neccessary, but it is.
    //
    // We call `GetOpenInputQuery.in_db_mut(self).invalidate(input);`
    // in `Database.did_open_input(…)`, `.did_change_input(…)`, and
    // `.did_close_input(…)` which correctly forces Salsa to re-run this query
    // function the next time this input is used. However, even though the
    // return value changes, Salsa doesn't record an updated `changed_at` value
    // in its internal `ActiveQuery` struct. `Runtime.report_untracked_read()`
    // manually sets this to the current revision.
    db.salsa_runtime().report_untracked_read();

    let content = db.get_open_input_raw(&input)?;
    Some(Arc::new(content))
}

pub trait InputWatcher {
    fn get_open_input_raw(&self, input: &Input) -> Option<String>;
}

#[derive(Debug, PartialEq, Eq, Clone, Hash, PartialOrd, Ord)]
pub enum Input {
    File(PathBuf),
    Untitled(String),
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::{
        compiler::{
            cst::{self, Cst, CstKind},
            string_to_cst::StringToCst,
        },
        database::Database,
    };

    #[test]
    fn on_demand_input_works() {
        let mut db = Database::default();
        let input = Input::File(PathBuf::from("/foo.rs"));

        db.did_open_input(&input, "123".to_owned());
        assert_eq!(
            db.get_input(input.clone()).unwrap().as_ref().to_owned(),
            "123"
        );
        assert_eq!(
            db.cst(input.clone()).unwrap().as_ref().to_owned(),
            vec![Cst {
                id: cst::Id(0),
                kind: CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(Cst {
                        id: cst::Id(1),
                        kind: CstKind::Int {
                            offset: 0,
                            value: 123,
                            source: "123".to_owned(),
                        }
                    })
                }
            }],
        );

        db.did_change_input(&input, "456".to_owned());
        assert_eq!(
            db.get_input(input.clone()).unwrap().as_ref().to_owned(),
            "456"
        );
        assert_eq!(
            db.cst(input.clone()).unwrap().as_ref().to_owned(),
            vec![Cst {
                id: cst::Id(0),
                kind: CstKind::LeadingWhitespace {
                    value: "\n".to_owned(),
                    child: Box::new(Cst {
                        id: cst::Id(1),
                        kind: CstKind::Int {
                            offset: 0,
                            value: 456,
                            source: "456".to_owned(),
                        }
                    })
                }
            }],
        );

        db.did_close_input(&input);
    }
}
