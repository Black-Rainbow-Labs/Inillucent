//! `inillucent tokenizer`: list, install and remove the tokenizer packs a search
//! table can name with `tokenize = '<name>'`.
//!
//! Invariant: **the verb writes the one folder the engine reads packs from.**
//! Every action uses `inillucent_core::install::tokenizer_folder`, which is
//! `INILLUCENT_TOKENIZER_DIR` when that is set and the install root's
//! `tokenizers` folder otherwise. Installing into the install root while the
//! engine read the variable's folder would report success and leave `CREATE`
//! saying the pack is not installed, which is the split the install module's
//! own invariant exists to rule out.
//!
//! A pack is checked by `Pack::parse` before a byte is written, and written as
//! `<name>.json.partial` and renamed, so a half written pack never exists under
//! its name. What is written is the pack's canonical JSON, the same text a
//! table stores, so installing the same pack twice is recognised as the same.

use std::path::Path;

use inillucent_core::install;
use inillucent_core::tokenize::pack::{Pack, PORTER};
use inillucent_core::tokenize::ENGLISH_STOPWORDS;
use inillucent_driver::Status;

use crate::command::outcome::{columns_from, table};
use crate::command::{Arguments, Context, Failed, Outcome};
use crate::json::{self, Json};

/// Runs `inillucent tokenizer [list|install|remove]`.
///
/// @param context - the surface, which may be confined or read only
/// @param arguments - the action, and `file` or `name`
pub fn tokenizer(context: &mut Context, arguments: &Arguments) -> Result<Outcome, Failed> {
    match arguments.text("action").unwrap_or("list") {
        "list" => Ok(list()),
        "install" => install_pack(context, arguments),
        "remove" => remove_pack(context, arguments),
        other => Err(Failed::misuse(format!(
            "'tokenizer' does list, install or remove, not '{other}'."
        ))),
    }
}

/// Refuses an action that writes the tokenizer folder, on a surface that may
/// not.
///
/// **Under `--root`, because the folder is outside the root**, the same reason
/// `migrate` from a server is refused there. Under `--readonly`, because a
/// person who asked for no writes asked for none, even outside the database.
///
/// @param context - the surface
/// @param action - `install` or `remove`, for the message
fn refuse_a_write(context: &Context, action: &str) -> Result<(), Failed> {
    if context.confined() {
        return Err(Failed::said(
            Status::InvalidState,
            format!(
                "this surface is confined to a directory with --root, and 'tokenizer {action}' \
                 writes the tokenizer folder outside it. Run it from an unconfined command line."
            ),
        ));
    }
    if context.readonly() {
        return Err(Failed::said(
            Status::ReadOnly,
            format!("'tokenizer {action}' writes the tokenizer folder, and this is read only."),
        ));
    }
    Ok(())
}

/// Lists `porter` and every installed pack, one row each.
///
/// A pack file that does not read is listed with its problem rather than
/// hiding the others, and a missing folder lists `porter` alone, which is what
/// a machine that never installed a pack has.
fn list() -> Outcome {
    let mut rows: Vec<Vec<Json>> = vec![vec![
        json::text(PORTER),
        json::text("english"),
        Json::Bool(false),
        Json::Int(ENGLISH_STOPWORDS.len() as i64),
        Json::Int(0),
        json::text("built in: English, with PostgreSQL's English stopwords"),
        Json::Null,
    ]];
    for name in install::installed_tokenizers() {
        let row = match install::tokenizer_pack(&name) {
            Ok(Some(pack)) => vec![
                json::text(pack.name.clone()),
                json::text(pack.stemmer.clone()),
                Json::Bool(pack.fold),
                Json::Int(pack.stopwords.len() as i64),
                Json::Int(pack.suffixes.len() as i64),
                json::text(pack.description.clone()),
                Json::Null,
            ],
            // Removed between the listing and the read.
            Ok(None) => continue,
            Err(problem) => vec![
                json::text(name),
                Json::Null,
                Json::Null,
                Json::Null,
                Json::Null,
                Json::Null,
                json::text(problem.to_string()),
            ],
        };
        rows.push(row);
    }
    let names: Vec<String> = [
        "name",
        "stemmer",
        "fold",
        "stopwords",
        "suffixes",
        "description",
        "problem",
    ]
    .iter()
    .map(|name| name.to_string())
    .collect();
    let columns = columns_from(&names, &rows);
    let folder = install::tokenizer_folder();
    let text = format!(
        "{}\nPacks are read from {}",
        table(&columns, &rows, ""),
        folder.display()
    );
    Outcome {
        command: "tokenizer".to_string(),
        total: rows.len(),
        rows,
        columns,
        more: false,
        changes: 0,
        last_insert_rowid: 0,
        elapsed_ms: 0.0,
        text,
        extra: vec![(
            "folder".to_string(),
            json::text(folder.display().to_string()),
        )],
    }
}

/// Installs the pack a file holds, under the name the pack gives itself.
///
/// The same pack installed twice is a success that writes nothing. A different
/// pack under a name already taken is refused, because replacing it in place
/// would let two machines with the same name mean two different things; the
/// person removes the old one first and knows they did.
///
/// @param context - the surface
/// @param arguments - `file`, the pack to install
fn install_pack(context: &mut Context, arguments: &Arguments) -> Result<Outcome, Failed> {
    refuse_a_write(context, "install")?;
    let Some(file) = arguments.text("file") else {
        return Err(Failed::misuse(
            "'tokenizer install' needs --file, the pack's JSON file.",
        ));
    };
    let path = context.confine(file)?;
    let text = match install::read_pack_file(&path) {
        Ok(Some(text)) => text,
        Ok(None) => {
            return Err(Failed::said(
                Status::NotFound,
                format!("there is no file at {file}."),
            ))
        }
        Err(problem) => return Err(Failed::misuse(format!("{file}: {problem}"))),
    };
    let pack = Pack::parse(&text)
        .map_err(|problem| Failed::misuse(format!("{file} is not a tokenizer pack: {problem}")))?;
    let name = pack.name.clone();
    let folder = install::tokenizer_folder();
    match install::tokenizer_pack(&name) {
        Ok(Some(installed)) if installed == pack => {
            return Ok(installed_outcome(&name, &folder, false));
        }
        Ok(Some(_)) | Err(_) => {
            return Err(Failed::said(
                Status::InvalidState,
                format!(
                    "a different tokenizer pack named {name} is installed. Tables made with it \
                     keep their own copy, so run 'inillucent tokenizer remove --name {name}' \
                     first and install this one after."
                ),
            ))
        }
        Ok(None) => {}
    }
    write_pack(&folder, &pack)
        .map_err(|problem| Failed::said(Status::Io, format!("installing {name}: {problem}")))?;
    Ok(installed_outcome(&name, &folder, true))
}

/// Writes a pack into the folder beside its destination, then renames it into
/// place.
///
/// @param folder - the tokenizer folder, made when it is not there
/// @param pack - the checked pack
fn write_pack(folder: &Path, pack: &Pack) -> std::io::Result<()> {
    std::fs::create_dir_all(folder)?;
    let destination = folder.join(format!("{}.json", pack.name));
    let partial = folder.join(format!("{}.json.partial", pack.name));
    std::fs::write(&partial, format!("{}\n", pack.to_json()))?;
    std::fs::rename(&partial, &destination)
}

/// Returns what a successful install reports.
///
/// @param name - the pack's name
/// @param folder - where it is installed
/// @param wrote - whether this run wrote it, or found it already there
fn installed_outcome(name: &str, folder: &Path, wrote: bool) -> Outcome {
    let said = if wrote {
        format!(
            "Installed the tokenizer pack {name} in {}.",
            folder.display()
        )
    } else {
        format!(
            "The tokenizer pack {name} is already installed in {}.",
            folder.display()
        )
    };
    Outcome::said(
        "tokenizer",
        format!("{said} Create a search table with tokenize = '{name}' to use it."),
    )
    .with("name", json::text(name))
    .with("folder", json::text(folder.display().to_string()))
    .with("installed", Json::Bool(wrote))
}

/// Removes an installed pack.
///
/// Safe for every table that already exists: each one holds its own copy of
/// the pack it was made with, so removing the file stops only new tables from
/// naming it.
///
/// @param context - the surface
/// @param arguments - `name`, the pack to remove
fn remove_pack(context: &mut Context, arguments: &Arguments) -> Result<Outcome, Failed> {
    refuse_a_write(context, "remove")?;
    let Some(name) = arguments.text("name") else {
        return Err(Failed::misuse(
            "'tokenizer remove' needs --name, the pack to remove.",
        ));
    };
    if name == PORTER {
        return Err(Failed::misuse(
            "porter is built into the engine and is not a pack, so there is nothing to remove.",
        ));
    }
    if !inillucent_core::tokenize::pack::valid_name(name) {
        return Err(Failed::misuse(format!(
            "'{name}' is not a tokenizer pack name: a name is lowercase letters, digits, - and _."
        )));
    }
    let folder = install::tokenizer_folder();
    let path = folder.join(format!("{name}.json"));
    if !path.is_file() {
        return Err(Failed::said(
            Status::NotFound,
            format!("no tokenizer pack named {name} is installed."),
        ));
    }
    std::fs::remove_file(&path)
        .map_err(|problem| Failed::said(Status::Io, format!("removing {name}: {problem}")))?;
    Ok(Outcome::said(
        "tokenizer",
        format!(
            "Removed the tokenizer pack {name}. Tables made with it keep working, because each \
             holds its own copy."
        ),
    )
    .with("name", json::text(name))
    .with("folder", json::text(folder.display().to_string())))
}
