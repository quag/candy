use crate::{
    compiler::{ast_to_hir::AstToHir, hir},
    database::Database,
    input::InputDb,
    language_server::utils::{LspPositionConversion, TupleToPosition},
};
use lsp_types::Position;

pub fn id_to_end_of_line(db: &Database, id: hir::Id) -> Option<Position> {
    let span = db.hir_id_to_display_span(id.clone())?;

    let line = db
        .offset_to_lsp(id.input.clone(), span.start)
        .to_position()
        .line;
    let line_start_offsets = db.line_start_utf8_byte_offsets(id.input.clone());
    let last_character_of_line = if line as usize == line_start_offsets.len() - 1 {
        db.get_input(id.input.clone()).unwrap().len()
    } else {
        line_start_offsets[(line + 1) as usize] - 1
    };
    let position = db
        .offset_to_lsp(id.input.clone(), last_character_of_line)
        .to_position();
    Some(position)
}
