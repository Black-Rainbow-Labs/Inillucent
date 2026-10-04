//! What `CREATE TABLE` and `CREATE INDEX` refuse while they are compiled.
//!
//! Invariant: **every refusal here is one SQLite makes before it stores
//! anything, in SQLite's words, and none of them reads a row or another
//! table.** The declarations are checked by binding them, so a name that does
//! not resolve, an aggregate or window function in the wrong place, and a
//! function nobody has are reported by the same code that reports them in a
//! query. What binding cannot decide is done here from the syntax: an unknown
//! collation, and a `DEFAULT` that is not constant.
//!
//! ## Why this is its own module
//!
//! The statement was accepted whole and its text stored, so a table SQLite
//! would have refused existed in this engine, and the first insert or read
//! reported the problem instead (or never did). `directive.rs` and `bind.rs`
//! are at the size `policy.rs` records for them, so the checks live here.

use inillucent_value::Affinity;

use super::{no_such_collation, refused, Binder, BoundExpr, BoundSource, IndexChoice, SourceRows};
use crate::ast::{self, ColumnConstraint, ColumnDef, Expr, ExprId, IndexedColumn, NameId};
use crate::catalog_view::{ColumnInfo, TableInfo, TableKind};
use crate::diagnostic::ParseError;
use crate::lexer::Span;

impl Binder<'_> {
    /// Refuses the parts of a `CREATE TABLE` that SQLite refuses at compile time.
    ///
    /// They are checked in the order SQLite meets them: each column's
    /// constraints as written, then the table's key collations, then the
    /// `CHECK` and generated expressions, which SQLite resolves once the whole
    /// column list is known.
    ///
    /// @param table - the table's name as written
    /// @param columns - the column definitions
    /// @param constraints - the table constraints
    pub(crate) fn check_table_declarations(
        &mut self,
        table: &[u8],
        columns: &[ColumnDef],
        constraints: &[(Option<NameId>, ast::TableConstraint)],
    ) -> Result<(), ParseError> {
        for column in columns {
            self.check_column_constraints(column)?;
        }
        for (_, constraint) in constraints {
            match constraint {
                ast::TableConstraint::PrimaryKey { columns, .. }
                | ast::TableConstraint::Unique { columns, .. } => {
                    for key in columns {
                        self.check_key_collation(key)?;
                    }
                }
                _ => {}
            }
        }
        let stand_in = self.stand_in_table(table, columns);
        let mut expressions: Vec<(ExprId, &'static str)> = Vec::new();
        for column in columns {
            for (_, constraint) in &column.constraints {
                match constraint {
                    ColumnConstraint::Check(expr) => expressions.push((*expr, "CHECK constraints")),
                    ColumnConstraint::Generated { expr, .. } => {
                        expressions.push((*expr, "generated columns"))
                    }
                    _ => {}
                }
            }
        }
        for (_, constraint) in constraints {
            if let ast::TableConstraint::Check { expr, .. } = constraint {
                expressions.push((*expr, "CHECK constraints"));
            }
        }
        self.bind_declared_expressions(stand_in, &expressions)
    }

    /// Refuses a `CREATE TABLE` whose columns and keys do not fit together.
    ///
    /// A repeated column name, a second primary key, a key that names a column
    /// the table has not got, a foreign key whose child column is not one, and a
    /// table whose every column is generated.
    ///
    /// @param table - the table's name as written
    /// @param columns - the column definitions
    /// @param constraints - the table constraints
    pub(crate) fn check_table_shape(
        &self,
        table: &[u8],
        columns: &[ColumnDef],
        constraints: &[(Option<NameId>, ast::TableConstraint)],
    ) -> Result<(), ParseError> {
        for (at, column) in columns.iter().enumerate() {
            let folded = self.ast.folded(column.name);
            if columns
                .iter()
                .take(at)
                .any(|earlier| self.ast.folded(earlier.name) == folded)
            {
                return Err(refused(
                    format!(
                        "duplicate column name: {}",
                        String::from_utf8_lossy(self.ast.text(column.name))
                    ),
                    Span::default(),
                ));
            }
        }
        let column_keys = columns
            .iter()
            .flat_map(|column| column.constraints.iter())
            .filter(|(_, constraint)| matches!(constraint, ColumnConstraint::PrimaryKey { .. }))
            .count();
        let table_keys = constraints
            .iter()
            .filter(|(_, constraint)| matches!(constraint, ast::TableConstraint::PrimaryKey { .. }))
            .count();
        if column_keys + table_keys > 1 {
            return Err(refused(
                format!(
                    "table \"{}\" has more than one primary key",
                    String::from_utf8_lossy(table)
                ),
                Span::default(),
            ));
        }
        for (_, constraint) in constraints {
            match constraint {
                ast::TableConstraint::PrimaryKey { columns: keys, .. }
                | ast::TableConstraint::Unique { columns: keys, .. } => {
                    for key in keys {
                        self.check_key_names_a_column(columns, key)?;
                    }
                }
                ast::TableConstraint::ForeignKey {
                    columns: children, ..
                } => {
                    for child in children {
                        self.check_child_column(columns, *child)?;
                    }
                }
                ast::TableConstraint::Check { .. } => {}
            }
        }
        Ok(())
    }

    /// Refuses a table whose every column is generated.
    ///
    /// @param columns - the column definitions
    pub(crate) fn refuse_all_generated(&self, columns: &[ColumnDef]) -> Result<(), ParseError> {
        let generated = |column: &ColumnDef| {
            column
                .constraints
                .iter()
                .any(|(_, held)| matches!(held, ColumnConstraint::Generated { .. }))
        };
        if !columns.is_empty() && columns.iter().all(generated) {
            return Err(refused(
                "must have at least one non-generated column",
                Span::default(),
            ));
        }
        Ok(())
    }

    /// Refuses a `PRIMARY KEY` or `UNIQUE` key that is an expression, or that
    /// names a column the table has not got.
    ///
    /// @param columns - the table's column definitions
    /// @param key - one term of the key
    fn check_key_names_a_column(
        &self,
        columns: &[ColumnDef],
        key: &IndexedColumn,
    ) -> Result<(), ParseError> {
        let operand = match self.ast.expr(key.expr) {
            Some(Expr::Collate { operand, .. }) => *operand,
            _ => key.expr,
        };
        let Some(Expr::Column {
            table: None,
            column,
            ..
        }) = self.ast.expr(operand)
        else {
            return Err(refused(
                "expressions prohibited in PRIMARY KEY and UNIQUE constraints",
                Span::default(),
            ));
        };
        let folded = self.ast.folded(*column);
        if columns
            .iter()
            .any(|held| self.ast.folded(held.name) == folded)
        {
            return Ok(());
        }
        Err(super::no_such_column(
            self.ast.text(*column),
            Span::default(),
        ))
    }

    /// Refuses a foreign key whose child column is not a column of the table.
    ///
    /// @param columns - the table's column definitions
    /// @param child - the column the key names
    fn check_child_column(&self, columns: &[ColumnDef], child: NameId) -> Result<(), ParseError> {
        let folded = self.ast.folded(child);
        if columns
            .iter()
            .any(|held| self.ast.folded(held.name) == folded)
        {
            return Ok(());
        }
        Err(refused(
            format!(
                "unknown column \"{}\" in foreign key definition",
                String::from_utf8_lossy(self.ast.text(child))
            ),
            Span::default(),
        ))
    }

    /// Refuses the parts of a `CREATE INDEX` that SQLite refuses at compile time.
    ///
    /// @param target - the table the index is over, as the catalog holds it
    /// @param columns - the indexed columns
    /// @param filter - the `WHERE` of a partial index
    pub(crate) fn check_index_declarations(
        &mut self,
        target: &TableInfo,
        columns: &[IndexedColumn],
        filter: Option<ExprId>,
    ) -> Result<(), ParseError> {
        for key in columns {
            self.check_key_collation(key)?;
        }
        let mut expressions: Vec<(ExprId, &'static str)> = Vec::new();
        for key in columns {
            let operand = match self.ast.expr(key.expr) {
                Some(Expr::Collate { operand, .. }) => *operand,
                _ => key.expr,
            };
            expressions.push((operand, "index expressions"));
        }
        if let Some(filter) = filter {
            expressions.push((filter, "partial index WHERE clauses"));
        }
        self.bind_declared_expressions(target.clone(), &expressions)
    }

    /// Refuses a new table or view whose name an index already has.
    ///
    /// Tables, views and indexes share one namespace in SQLite, so the name of
    /// an existing index is taken even though it is not a table. The table
    /// check comes first and `IF NOT EXISTS` does not excuse this one.
    ///
    /// @param database_name - the database the object is created in
    /// @param folded - the new object's folded name
    /// @param name - the new object's name, which the error points at
    pub(crate) fn refuse_index_namesake(
        &self,
        database_name: &[u8],
        folded: &[u8],
        written: &[u8],
    ) -> Result<(), ParseError> {
        if self
            .catalog
            .find_index(Some(database_name), folded)
            .is_none()
        {
            return Ok(());
        }
        Err(refused(
            format!(
                "there is already an index named {}",
                String::from_utf8_lossy(written)
            ),
            Span::default(),
        ))
    }

    /// Checks the name a `CREATE INDEX` gives, and reports whether it is taken.
    ///
    /// **A table of the same name comes first, and `IF NOT EXISTS` does not
    /// excuse it.** Tables and indexes share one namespace in SQLite, and
    /// `sqlite3CreateIndex` asks for a table before it asks for an index. Names
    /// that begin `sqlite_` are reserved.
    ///
    /// @param database_name - the database the index is created in
    /// @param folded - the new index's folded name
    /// @param written - the new index's name as written
    /// @param if_not_exists - whether `IF NOT EXISTS` was written
    pub(crate) fn check_new_index_name(
        &self,
        database_name: &[u8],
        folded: &[u8],
        written: &[u8],
        if_not_exists: bool,
    ) -> Result<bool, ParseError> {
        let name = String::from_utf8_lossy(written);
        if self
            .catalog
            .find_table(Some(database_name), folded)
            .is_some()
        {
            return Err(refused(
                format!("there is already a table named {name}"),
                Span::default(),
            ));
        }
        if written.to_ascii_lowercase().starts_with(b"sqlite_") {
            return Err(refused(
                format!("object name reserved for internal use: {name}"),
                Span::default(),
            ));
        }
        let exists = self
            .catalog
            .find_index(Some(database_name), folded)
            .is_some();
        if exists && !if_not_exists {
            return Err(refused(
                format!("index {name} already exists"),
                Span::default(),
            ));
        }
        Ok(exists)
    }

    /// Refuses `DROP TABLE` of a table SQLite owns, and of a view.
    ///
    /// SQLite's own tables go with the feature that owns them; only the
    /// statistics tables and `sqlite_parameters` may be dropped.
    ///
    /// @param table - the object the statement names
    /// @param written - the name as written
    pub(crate) fn refuse_dropping_own_table(
        &self,
        table: &TableInfo,
        written: &[u8],
    ) -> Result<(), ParseError> {
        if table.folded.starts_with(b"sqlite_")
            && !table.folded.starts_with(b"sqlite_stat")
            && !table.folded.starts_with(b"sqlite_parameters")
        {
            return Err(refused(
                format!(
                    "table {} may not be dropped",
                    String::from_utf8_lossy(&table.name)
                ),
                Span::default(),
            ));
        }
        if table.kind == TableKind::View {
            return Err(refused(
                format!(
                    "use DROP VIEW to delete view {}",
                    String::from_utf8_lossy(written)
                ),
                Span::default(),
            ));
        }
        Ok(())
    }

    /// Refuses `DROP INDEX` of an index a `UNIQUE` or `PRIMARY KEY` constraint
    /// made, which goes with the constraint and nothing else removes it.
    ///
    /// @param database_name - the database the index is in
    /// @param folded - the index's folded name
    pub(crate) fn refuse_dropping_constraint_index(
        &self,
        database_name: &[u8],
        folded: &[u8],
    ) -> Result<(), ParseError> {
        let Some((_, held)) = self.catalog.find_index(Some(database_name), folded) else {
            return Ok(());
        };
        if !matches!(
            held.origin,
            crate::catalog_view::IndexOrigin::Unique | crate::catalog_view::IndexOrigin::PrimaryKey
        ) {
            return Ok(());
        }
        Err(refused(
            "index associated with UNIQUE or PRIMARY KEY constraint cannot be dropped",
            Span::default(),
        ))
    }

    /// Refuses an index over a view or over one of SQLite's own tables.
    ///
    /// @param target - the table the index is over
    pub(crate) fn refuse_unindexable(&self, target: &TableInfo) -> Result<(), ParseError> {
        let reason = if target.kind == TableKind::View {
            Some("views may not be indexed".to_string())
        } else if target.folded.starts_with(b"sqlite_") {
            Some(format!(
                "table {} may not be indexed",
                String::from_utf8_lossy(&target.name)
            ))
        } else {
            None
        };
        match reason {
            Some(message) => Err(refused(message, Span::default())),
            None => Ok(()),
        }
    }

    /// Builds the refusal for an index whose table is not where it was looked for.
    ///
    /// A `TEMP` index over a table in another database says so; one over
    /// SQLite's own schema table, which has no entry in the catalog, is refused
    /// as unindexable; anything else is a missing table, named with the schema
    /// it was looked for in.
    ///
    /// @param database - the database the index is created in
    /// @param table_folded - the table's folded name
    /// @param table - the table's name as written
    pub(crate) fn index_without_table(
        &self,
        database: usize,
        table_folded: &[u8],
        table: NameId,
    ) -> ParseError {
        let written = self.ast.text(table);
        let elsewhere = self.catalog.find_table(None, table_folded).cloned();
        if let Some(found) = elsewhere {
            if self.catalog.database_name(database) == b"temp" && found.database != database {
                return refused(
                    format!(
                        "cannot create a TEMP index on non-TEMP table \"{}\"",
                        String::from_utf8_lossy(&found.name)
                    ),
                    Span::default(),
                );
            }
        }
        let schema = [
            b"sqlite_master".as_slice(),
            b"sqlite_schema",
            b"sqlite_temp_master",
            b"sqlite_temp_schema",
        ];
        if schema.iter().any(|name| name.eq_ignore_ascii_case(written)) {
            return refused(
                format!(
                    "table {} may not be indexed",
                    String::from_utf8_lossy(written)
                ),
                Span::default(),
            );
        }
        // SQLite names the schema it looked in: "no such table: main.t".
        let missing = [self.catalog.database_name(database), b".", written].concat();
        super::no_such_table(&missing, Span::default())
    }

    /// Builds the refusal for a new table or view whose name is taken.
    ///
    /// SQLite names what is there: a view in the way of a table is `view v
    /// already exists`.
    ///
    /// @param database_name - the database the object is created in
    /// @param folded - the new object's folded name
    /// @param name - the new object's name, which the error points at
    pub(crate) fn already_exists(
        &self,
        database_name: &[u8],
        folded: &[u8],
        name: crate::ast::NameId,
    ) -> ParseError {
        let kind = match self
            .catalog
            .find_table(Some(database_name), folded)
            .map(|held| held.kind)
        {
            Some(TableKind::View) => "view",
            _ => "table",
        };
        // SQLite points at the name, so the shell prints the statement under it.
        let span = self
            .ast
            .name(name)
            .map_or(Span::default(), |entry| entry.span);
        let written = String::from_utf8_lossy(self.ast.text(name));
        refused(format!("{kind} {written} already exists"), span)
    }

    /// Refuses a view whose query holds a parameter marker.
    ///
    /// The statement's arena holds only the `CREATE VIEW`, so any parameter in
    /// it is the view's.
    pub(crate) fn refuse_view_parameters(&self) -> Result<(), ParseError> {
        let parameter = (0..self.ast.expr_count())
            .filter_map(|at| self.ast.expr(ExprId(at as u32)))
            .any(|expr| matches!(expr, Expr::Parameter { .. }));
        if parameter {
            return Err(refused(
                "parameters are not allowed in views",
                Span::default(),
            ));
        }
        Ok(())
    }

    /// Refuses a trigger body that writes to a table through a schema name.
    ///
    /// A trigger lives in one database and its body reaches tables there by
    /// their plain names, so `INSERT INTO main.t` is refused in a trigger that
    /// is not `TEMP`. Only the written target counts: a qualified name in a
    /// `FROM` clause or a subquery is allowed.
    ///
    /// @param body - the trigger's statements
    pub(crate) fn refuse_qualified_trigger_targets(
        &self,
        body: &[ast::Statement],
    ) -> Result<(), ParseError> {
        let qualified = |term: ast::FromTermId| {
            matches!(
                self.ast.from_term(term).map(|held| &held.source),
                Some(ast::FromSource::Table {
                    database: Some(_),
                    ..
                })
            )
        };
        for statement in body {
            let refused_target = match statement {
                ast::Statement::Insert(insert) => insert.database.is_some(),
                ast::Statement::Update(update) => qualified(update.target),
                ast::Statement::Delete(delete) => qualified(delete.target),
                _ => false,
            };
            if refused_target {
                return Err(refused(
                    "qualified table names are not allowed on INSERT, UPDATE, and DELETE statements within triggers",
                    Span::default(),
                ));
            }
        }
        Ok(())
    }

    /// Refuses a column constraint that names a collation nobody has, or a
    /// `DEFAULT` that is not constant.
    ///
    /// @param column - the column definition
    pub(crate) fn check_column_constraints(&self, column: &ColumnDef) -> Result<(), ParseError> {
        for (_, constraint) in &column.constraints {
            match constraint {
                ColumnConstraint::Collate(name) => {
                    let text = self.ast.text(*name);
                    if self.collation_named(text).is_none() {
                        return Err(no_such_collation(text, Span::default()));
                    }
                }
                ColumnConstraint::Default(expr) if !self.default_is_constant(*expr) => {
                    return Err(refused(
                        format!(
                            "default value of column [{}] is not constant",
                            String::from_utf8_lossy(self.ast.text(column.name))
                        ),
                        Span::default(),
                    ));
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// Refuses an indexed column that names a collation nobody has.
    ///
    /// The name may be written after the column or wrapped around it.
    ///
    /// @param key - one indexed column of a key or an index
    fn check_key_collation(&self, key: &IndexedColumn) -> Result<(), ParseError> {
        let wrapped = match self.ast.expr(key.expr) {
            Some(Expr::Collate { collation, .. }) => Some(*collation),
            _ => None,
        };
        for name in [key.collation, wrapped].into_iter().flatten() {
            let text = self.ast.text(name);
            if self.collation_named(text).is_none() {
                return Err(no_such_collation(text, Span::default()));
            }
        }
        Ok(())
    }

    /// Reports whether a `DEFAULT` expression is constant, as SQLite counts it.
    ///
    /// A literal is, and so is a function of constants. A column, a subquery
    /// and a parameter are not. `DEFAULT (nosuch)` is a column reference as far
    /// as the parser can tell, which is why it is refused.
    ///
    /// @param id - the expression
    fn default_is_constant(&self, id: ExprId) -> bool {
        !self.any_node(id, &|node| {
            matches!(
                node,
                Expr::Parameter { .. }
                    | Expr::Column { .. }
                    | Expr::Star { .. }
                    | Expr::Exists { .. }
                    | Expr::Subquery(_)
                    | Expr::Raise { .. }
                    | Expr::Function { over: Some(_), .. }
                    | Expr::In {
                        rhs: ast::InRhs::Select(_) | ast::InRhs::Table { .. },
                        ..
                    }
            )
        })
    }

    /// Returns the refusal an expression earns for what it contains, if any.
    ///
    /// A subquery is refused everywhere a declaration can hold an expression,
    /// and a function whose answer changes between calls is refused everywhere
    /// but a `CHECK`, which SQLite evaluates on every write anyway.
    ///
    /// @param id - the expression
    /// @param place - where it was written, in SQLite's words
    fn prohibited_in(&self, id: ExprId, place: &str) -> Option<String> {
        let subquery = |node: &Expr| {
            matches!(
                node,
                Expr::Exists { .. }
                    | Expr::Subquery(_)
                    | Expr::In {
                        rhs: ast::InRhs::Select(_),
                        ..
                    }
            )
        };
        if self.any_node(id, &subquery) {
            return Some(format!("subqueries prohibited in {place}"));
        }
        // A qualified name is refused in an index expression and a generated
        // column, and allowed in a `CHECK` and in a partial index's `WHERE`.
        let qualified = |node: &Expr| matches!(node, Expr::Column { table: Some(_), .. });
        let dot_refused = place == "index expressions" || place == "generated columns";
        if dot_refused && self.any_node(id, &qualified) {
            return Some(format!("the \".\" operator prohibited in {place}"));
        }
        let volatile = |node: &Expr| match node {
            Expr::Function { name, .. } => crate::function::is_volatile(self.ast.folded(*name)),
            Expr::Literal(ast::Literal::CurrentDate)
            | Expr::Literal(ast::Literal::CurrentTime)
            | Expr::Literal(ast::Literal::CurrentTimestamp) => true,
            _ => false,
        };
        if place != "CHECK constraints" && self.any_node(id, &volatile) {
            return Some(format!("non-deterministic functions prohibited in {place}"));
        }
        None
    }

    /// Reports whether any node of an expression tree satisfies a test.
    ///
    /// @param id - the root of the tree
    /// @param test - the question to ask of each node
    fn any_node(&self, id: ExprId, test: &dyn Fn(&Expr) -> bool) -> bool {
        let Some(expr) = self.ast.expr(id) else {
            return false;
        };
        test(expr)
            || expr_children(expr)
                .into_iter()
                .any(|child| self.any_node(child, test))
    }

    /// Builds a table that has the names of a `CREATE TABLE`'s columns and
    /// nothing else, for binding its declarations against.
    ///
    /// @param table - the table's name as written
    /// @param columns - the column definitions
    fn stand_in_table(&self, table: &[u8], columns: &[ColumnDef]) -> TableInfo {
        let described = columns.iter().map(|column| {
            let name = self.ast.text(column.name).to_vec();
            ColumnInfo {
                folded: self.ast.folded(column.name).to_vec(),
                name,
                declared_type: Vec::new(),
                affinity: Affinity::Blob,
                collation: b"binary".to_vec(),
                not_null: false,
                not_null_conflict: None,
                primary_key_conflict: None,
                default_sql: None,
                primary_key_position: None,
                hidden: false,
                generated: false,
                stored: false,
                generated_sql: None,
            }
        });
        TableInfo {
            name: table.to_vec(),
            folded: table.to_ascii_lowercase(),
            database: 0,
            root: 0,
            columns: described.collect(),
            rowid_alias: None,
            without_rowid: false,
            strict: false,
            autoincrement: false,
            kind: TableKind::Table,
            create_sql: Vec::new(),
            indexes: Vec::new(),
            view: None,
            triggers: Vec::new(),
            analysed_rows: None,
            foreign_key_triggers: Vec::new(),
            foreign_keys: Vec::new(),
            checks: Vec::new(),
            module: None,
        }
    }

    /// Binds declared expressions with one table in scope, and discards them.
    ///
    /// The point is the refusals binding makes. A parameter is refused here
    /// too, in the words SQLite uses for the place it was written.
    ///
    /// **An unknown collation inside the expression is let through.** SQLite
    /// resolves a collation named in a `CHECK` when the check runs, so
    /// `CHECK (a COLLATE nosuch > 1)` is created and fails at the first insert.
    ///
    /// @param table - the table the expressions are written against
    /// @param expressions - each expression with the place it was written, for
    ///   the message about parameters
    fn bind_declared_expressions(
        &mut self,
        table: TableInfo,
        expressions: &[(ExprId, &'static str)],
    ) -> Result<(), ParseError> {
        if expressions.is_empty() {
            return Ok(());
        }
        let saved_scopes = core::mem::take(&mut self.scopes);
        let sources_before = self.sources.len();
        let alias = table.name.clone();
        let id = sources_before;
        self.sources.push(BoundSource {
            index_hint: IndexChoice::Any,
            id,
            rows: SourceRows::Table,
            table: std::rc::Rc::new(table),
            alias,
            join: ast::JoinKind::Comma,
            constraint: None,
            suppressed: Vec::new(),
            index_exprs: Vec::new(),
            written_schema: None,
            derived: Default::default(),
        });
        self.scopes.push(vec![id]);
        // Bound as top-level SQL, not as schema text: a function only direct
        // SQL may call is refused when the declaration is *used*, so a table
        // that names one is stored (see `schema_function_policy`).
        let mut outcome = Ok(());
        for (expr, place) in expressions {
            if let Some(reason) = self.prohibited_in(*expr, place) {
                outcome = Err(refused(reason, Span::default()));
                break;
            }
            match self.bind_expr(*expr) {
                Ok(bound) if holds_parameter(&bound) => {
                    outcome = Err(refused(
                        format!("parameters prohibited in {place}"),
                        Span::default(),
                    ));
                }
                Ok(_) => {}
                Err(error) if error.message().starts_with("no such collation sequence") => {}
                Err(error) => outcome = Err(error),
            }
            if outcome.is_err() {
                break;
            }
        }
        self.sources.truncate(sources_before);
        self.scopes = saved_scopes;
        outcome
    }
}

/// Reports whether a bound expression contains a parameter marker.
///
/// @param expr - the bound expression
fn holds_parameter(expr: &BoundExpr) -> bool {
    matches!(expr, BoundExpr::Parameter(_)) || expr.children().into_iter().any(holds_parameter)
}

/// Returns the expressions an expression is made of.
///
/// A subquery is a leaf: what it holds is its own query, which the callers
/// that care about one refuse by name.
///
/// @param expr - the expression
fn expr_children(expr: &Expr) -> Vec<ExprId> {
    match expr {
        Expr::Literal(_)
        | Expr::Parameter { .. }
        | Expr::Column { .. }
        | Expr::Star { .. }
        | Expr::Exists { .. }
        | Expr::Subquery(_) => Vec::new(),
        Expr::Unary { operand, .. }
        | Expr::Collate { operand, .. }
        | Expr::Cast { operand, .. }
        | Expr::IsNull { operand, .. } => vec![*operand],
        Expr::Binary { left, right, .. } | Expr::Is { left, right, .. } => vec![*left, *right],
        Expr::Pattern {
            operand,
            pattern,
            escape,
            ..
        } => {
            let mut all = vec![*operand, *pattern];
            all.extend(*escape);
            all
        }
        Expr::Between {
            operand, low, high, ..
        } => vec![*operand, *low, *high],
        Expr::In { operand, rhs, .. } => {
            let mut all = vec![*operand];
            if let ast::InRhs::List(list) = rhs {
                all.extend(list.iter().copied());
            }
            all
        }
        Expr::Case {
            operand,
            branches,
            otherwise,
        } => {
            let mut all: Vec<ExprId> = operand.iter().copied().collect();
            for (when, then) in branches {
                all.push(*when);
                all.push(*then);
            }
            all.extend(*otherwise);
            all
        }
        Expr::Function {
            arguments, filter, ..
        } => {
            let mut all: Vec<ExprId> = arguments.iter().flatten().copied().collect();
            all.extend(*filter);
            all
        }
        Expr::RowValue(list) => list.clone(),
        Expr::Raise { message, .. } => message.iter().copied().collect(),
    }
}
